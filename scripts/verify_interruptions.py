"""Bounded process-level fault tests; real masked Windows capture, no input automation."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time

import psutil
from verify_series import MCP, validate


def gone(children):
    _, alive = psutil.wait_procs(children, timeout=6)
    assert not alive, f"orphan recording workers: {[p.pid for p in alive]}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("exe")
    parser.add_argument("--output", default="work/interruption-tests")
    args = parser.parse_args()
    exe = str(Path(args.exe).resolve())
    root = Path(args.output).resolve()
    root.mkdir(parents=True, exist_ok=False)
    checks = []
    common = {"region": {"x": 0, "y": 0, "width": 64, "height": 64},
              "masks": [{"x": 0, "y": 0, "width": 64, "height": 64}], "fps": 5}
    m = MCP(exe, root)
    try:
        # Removed APIs fail explicitly and must not start a worker.
        m.tool("framelet_record", {**common, "output": "removed.gif"}, ok=False)
        assert not psutil.Process(m.p.pid).children()
        s = m.tool("framelet_start_recording", {**common, "output": "saved.gif", "duration": 30})
        sid = s["session_id"]
        m.wait(sid, lambda v: v.get("samples", 0) >= 2)
        m.tool("framelet_pause_recording", {"session_id": sid})
        m.wait(sid, lambda v: v.get("state") == "paused")
        before = time.monotonic()
        m.tool("framelet_stop_recording", {"session_id": sid})
        assert time.monotonic() - before < 1, "stop blocked the MCP dispatcher"
        for name in ["cancel", "pause", "resume", "stop"]:
            m.tool(f"framelet_{name}_recording", {"session_id": sid})
        done = m.wait(sid, lambda v: v["terminal"])
        assert done["result"]["state"] == "completed", done
        saved = root / "saved.gif"
        digest = hashlib.sha256(saved.read_bytes()).hexdigest()
        assert m.tool("framelet_cancel_recording", {"session_id": sid}) == done
        assert hashlib.sha256(saved.read_bytes()).hexdigest() == digest
        checks.append("stop is nonblocking and wins over later cancel/pause/resume; saved bytes immutable")

        s = m.tool("framelet_start_recording", {**common, "output": "cancelled.gif", "duration": 30, "delay": 5})
        sid = s["session_id"]
        m.tool("framelet_cancel_recording", {"session_id": sid})
        m.tool("framelet_stop_recording", {"session_id": sid})
        assert m.wait(sid, lambda v: v["terminal"])["result"]["state"] == "cancelled"
        assert not (root / "cancelled.gif").exists()
        checks.append("cancel during countdown wins over later stop and permits next task")

        s = m.tool("framelet_start_recording", {**common, "output": "suspended.gif", "duration": 60})
        sid = s["session_id"]
        m.wait(sid, lambda v: v.get("samples", 0) >= 2)
        child = psutil.Process(m.p.pid).children()[0]
        child.suspend()
        before = time.monotonic()
        m.tool("framelet_stop_recording", {"session_id": sid})
        assert time.monotonic() - before < 1
        assert m.rpc("ping") == {}, "server unresponsive while native worker hangs"
        done = m.wait(sid, lambda v: v["terminal"], seconds=22)
        assert done["result"]["state"] == "failed" and "timeout" in done["result"]["error"]
        gone([child])
        s = m.tool("framelet_start_recording", {**common, "output": "next.gif", "duration": .4})
        assert m.wait(s["session_id"], lambda v: v["terminal"])["result"]["state"] == "completed"
        checks.append("suspended worker times out, is reaped, ping and next recording succeed")
    finally:
        m.close()

    # Abrupt host death must kill the child even while paused. The durable
    # checkpoint is then explicitly recoverable by a fresh host, twice safely.
    crash = root / "host-death"
    crash.mkdir()
    m = MCP(exe, crash)
    try:
        s = m.tool("framelet_start_series", {**common, "output_dir": str(crash),
                   "name": "killed-host", "until_stop": True, "segment_seconds": 30})
        m.wait(s["session_id"], lambda v: v.get("recorded_ms", 0) >= 5800)
        m.tool("framelet_pause_recording", {"session_id": s["session_id"]})
        m.wait(s["session_id"], lambda v: v.get("state") == "paused")
        children = [c for c in psutil.Process(m.p.pid).children() if 'worker-record' in c.cmdline()]
        assert len(children) == 1
        m.p.kill()
        m.p.wait(timeout=5)
        gone(children)
        for attempt in range(2):
            result = subprocess.run([exe, "recover", s["directory"]], capture_output=True,
                                    text=True, encoding="utf-8", timeout=25)
            assert result.returncode == 0, result.stderr
            value = json.loads(result.stdout)["result"]
            assert value["recovered_parts"] == (1 if attempt == 0 else 0), value
            assert not value["errors"], value
        validate(exe, s["directory"])
        checks.append("host killed while paused: job kills worker, fresh-host recovery validates and is idempotent")
    finally:
        if m.p.poll() is None:
            m.close()
        else:
            m.p.stdin.close()
            m.log.close()

    # Disconnect the output pipe rather than stdin. The failed response must
    # unwind Sessions and reap the worker, instead of leaving an uncontrolled task.
    for mode in ["single", "series", "stopped-single"]:
        env = {**os.environ, "FRAMELET_MCP_ROOT": str(root), "LOCALAPPDATA": str(root)}
        p = subprocess.Popen([exe, "mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=subprocess.DEVNULL, text=True, encoding="utf-8", env=env)
        children = []
        try:
            def rpc(seq, name, arguments):
                p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": seq, "method": "tools/call",
                              "params": {"name": name, "arguments": arguments}}) + "\n")
                p.stdin.flush()
                return json.loads(p.stdout.readline())["result"]["structuredContent"]["result"]
            if mode != "series":
                s = rpc(1, "framelet_start_recording", {**common, "output": f"broken-pipe-{mode}.gif", "duration": 30})
            else:
                s = rpc(1, "framelet_start_series", {**common, "output_dir": str(root), "name": "broken-pipe", "until_stop": True})
            until = time.monotonic() + 10
            seq = 2
            while True:
                status = rpc(seq, "framelet_recording_status", {"session_id": s["session_id"]})
                seq += 1
                if status.get("samples", 0) >= 2:
                    break
                assert time.monotonic() < until
                time.sleep(.03)
            children = psutil.Process(p.pid).children()
            assert children
            if mode == "stopped-single":
                rpc(seq, "framelet_stop_recording", {"session_id": s["session_id"]})
                seq += 1
            p.stdout.close()
            p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": seq, "method": "ping"}) + "\n")
            p.stdin.flush()
            p.wait(timeout=22)
            assert p.returncode != 0, "broken pipe was not reported as transport failure"
            gone(children)
            if mode == "series":
                assert validate(exe, s["directory"])["parts"]
            elif mode == "stopped-single":
                r = subprocess.run([exe, "timeline", str(root / f"broken-pipe-{mode}.gif")], capture_output=True, timeout=15)
                assert r.returncode == 0, r.stderr
            else:
                assert not (root / f"broken-pipe-{mode}.gif").exists()
            checks.append(f"{mode}: broken response pipe shuts down task and reaps worker")
        finally:
            if p.poll() is None:
                p.kill()
                p.wait(timeout=5)
            p.stdin.close()
            gone(children)

    proof = {"ok": True, "exe_sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(), "checks": checks}
    (root / "verification.json").write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps(proof), flush=True)


if __name__ == "__main__":
    main()
