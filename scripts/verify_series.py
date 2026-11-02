"""Real Windows recording, rotation and interrupted-checkpoint acceptance tests.

The 650-second test uses real GDI capture at 1 FPS with a full-region mask.
It validates lifecycle/durability, not high-motion capture performance.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import threading
import time

import psutil


def load(path):
    return json.loads(Path(path).read_text(encoding="utf-8"))


def run(exe, *args, timeout=25):
    p = subprocess.run([exe, *map(str, args)], capture_output=True, text=True,
                       encoding="utf-8", timeout=timeout)
    assert p.returncode == 0, (args, p.stdout, p.stderr)
    return json.loads(p.stdout)["result"]


class MCP:
    def __init__(self, exe, root):
        self.seq = 0
        self.q = queue.Queue()
        self.log = (root / f"mcp-stderr-{time.time_ns()}.log").open("w", encoding="utf-8")
        self.p = subprocess.Popen([exe, "mcp"], stdin=subprocess.PIPE,
                                  stdout=subprocess.PIPE, stderr=self.log,
                                  text=True, encoding="utf-8",
                                  env={**os.environ, "FRAMELET_MCP_ROOT": str(root)})
        def read():
            for line in self.p.stdout:
                self.q.put(json.loads(line))
        threading.Thread(target=read, daemon=True).start()
        self.rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                "clientInfo": {"name": "series-acceptance", "version": "1"}})

    def rpc(self, method, params=None):
        self.seq += 1
        self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.seq,
                                      "method": method, "params": params or {}}) + "\n")
        self.p.stdin.flush()
        r = self.q.get(timeout=25)
        assert r["id"] == self.seq, r
        return r["result"]

    def tool(self, name, args=None, ok=True):
        r = self.rpc("tools/call", {"name": name, "arguments": args or {}})
        assert (not r.get("isError", False)) == ok, r
        return r.get("structuredContent", {}).get("result")

    def status(self, sid):
        return self.tool("framelet_recording_status", {"session_id": sid})

    def wait(self, sid, predicate, seconds=20):
        until = time.monotonic() + seconds
        while time.monotonic() < until:
            value = self.status(sid)
            if predicate(value):
                return value
            time.sleep(.05)
        raise AssertionError(("status timeout", value))

    def close(self):
        self.p.stdin.close()
        self.p.wait(timeout=22)
        self.log.close()
        assert self.p.returncode == 0


def validate(exe, directory, expected=None):
    root = Path(directory)
    index = load(root / "recording.json")
    assert index["schema"] == 1
    parts = index["parts"]
    if index["state"]=="completed":
        assert not list((root/".work").iterdir()),"successful recording left unfinished/empty part artifacts"
    if expected is not None:
        assert [p["recording"]["duration_ms"] for p in parts] == expected, parts
    ids = set()
    start = 0
    for number, part in enumerate(parts, 1):
        s = part["recording"]
        assert part["number"] == number and part["start_ms"] == start
        assert s["output"].startswith(index["prefix"] + ".part-")
        assert 0 < s["duration_ms"] <= 300_000
        start += s["duration_ms"]
        gif = root / s["output"]
        info = run(exe, "inspect", gif)
        assert info["duration_ms"] == s["duration_ms"]
        assert s["timeline"] and s["manifest"], s
        d = load(root / s["timeline"])
        assert d["session_id"] == index["session_id"]
        observations = [p["id"] for p in d["samples"]] + [p["id"] for p in d["events"]]
        assert all(v is not None for v in observations)
        assert not ids.intersection(observations), "observation identity reset at rotation"
        ids.update(observations)
        run(exe, "timeline", gif, "--at", "0")  # strict GIF/hash/mapping validation
        run(exe, "frame-at", gif, "--at", "0", "--output", root.parent / (root.stem + f"-part-{number}.png"))
    return index


def short(exe, root):
    common = {"output_dir": str(root), "region": {"x": 0, "y": 0, "width": 64, "height": 64},
              "fps": 5, "masks": [{"x": 0, "y": 0, "width": 64, "height": 64}]}
    m = MCP(exe, root)
    try:
        args = {**common, "name": "boundary", "duration": 3, "segment_seconds": 1, "request_id": "boundary-001"}
        s = m.tool("framelet_start_series", args)
        sid = s["session_id"]
        assert m.tool("framelet_start_series", args)["session_id"] == sid
        other=MCP(exe,root)
        try:
            existing=other.tool("framelet_start_series",args)
            assert existing["existing"] and not existing["terminal"] and not existing["owned_by_this_connection"],existing
            assert not psutil.Process(other.p.pid).children(),"duplicate request started another worker"
        finally:other.close()
        m.tool("framelet_start_series", {**args, "fps": 4}, ok=False)
        done = m.wait(sid, lambda s: s["terminal"])
        assert done["result"]["state"] == "completed", done
        directory = s["directory"]
        validate(exe, directory, [1000, 1000, 1000])
        assert m.tool("framelet_start_series", args)["existing"]
        query = m.tool("framelet_frame_at", {"input": directory, "at": 1.2,
                                             "output": str(root / "global-probe.png")})
        assert query["segment_start_ms"] == 1000
        m.tool("framelet_timeline", {"input": directory, "at": 1.2})
        overview=m.tool("framelet_extract_frames",{"input":directory,"output":str(root/"group-overview"),"count":3,"mode":"uniform"})
        assert len(overview["frames"])==3 and Path(overview["contact_sheet"]).is_file()
        listing = m.tool("framelet_recording_list")
        assert listing["recordings"]
        m.tool("framelet_start_series", {**common, "duration": 1, "segment_seconds": 301}, ok=False)
        m.tool("framelet_start_series", {**common, "duration": 1, "name": ".."}, ok=False)

        # Checkpoint cadence must not consume the held source before a low-FPS sampling deadline.
        low=m.tool("framelet_start_series",{**common,"name":"lowfps-checkpoint","duration":13,"segment_seconds":6,"fps":1})
        low_done=m.wait(low["session_id"],lambda s:s["terminal"],seconds=22)
        assert low_done["result"]["state"]=="completed",low_done
        validate(exe,low["directory"],[6000,6000,1000])

        # Pause does not consume the per-part media budget.
        s = m.tool("framelet_start_series", {**common, "name": "paused", "until_stop": True, "segment_seconds": 1})
        sid = s["session_id"]
        m.wait(sid, lambda s: s.get("recorded_ms", 0) >= 250)
        m.tool("framelet_pause_recording", {"session_id": sid})
        p = m.wait(sid, lambda s: s.get("state") == "paused")
        time.sleep(1.2)
        q = m.status(sid)
        assert abs(q["recorded_ms"] - p["recorded_ms"]) < 30, (p, q)
        m.tool("framelet_resume_recording", {"session_id": sid})
        m.wait(sid, lambda s: s.get("recorded_ms", 0) >= 1300)
        start = time.monotonic()
        m.tool("framelet_stop_recording", {"session_id": sid})
        assert time.monotonic() - start < 1.5
        m.wait(sid, lambda s: s["terminal"])
        validate(exe, s["directory"])

        # Worker crash after a real durable checkpoint, before any 30-second segment completes.
        s = m.tool("framelet_start_series", {**common, "name": "crash", "duration": 30, "segment_seconds": 30})
        sid = s["session_id"]
        m.wait(sid, lambda s: s.get("recorded_ms", 0) >= 5800)
        m.tool("framelet_recover", {"input": s["directory"]}, ok=False)
        children = psutil.Process(m.p.pid).children()
        assert len(children) == 1
        children[0].kill()
        children[0].wait(timeout=5)
        done = m.wait(sid, lambda s: s["terminal"])
        assert done["result"]["state"] == "failed", done
        recovered = m.tool("framelet_recover", {"input": s["directory"]})
        assert recovered["recovered_parts"] == 1 and recovered["state"] == "partial", recovered
        validate(exe, s["directory"])
        assert m.tool("framelet_recover", {"input": s["directory"]})["recovered_parts"] == 0
        assert m.rpc("ping") == {}

        # Cancel preserves committed parts, but does not make the cancelled tail recoverable.
        s=m.tool("framelet_start_series",{**common,"name":"cancel","until_stop":True,"segment_seconds":1})
        sid=s["session_id"]
        m.wait(sid,lambda s:s.get("recorded_ms",0)>=1350)
        m.tool("framelet_cancel_recording",{"session_id":sid})
        done=m.wait(sid,lambda s:s["terminal"])
        assert done["result"]["state"]=="cancelled",done
        idx=validate(exe,s["directory"],[1000])
        assert len(idx["parts"])==1
        assert m.tool("framelet_recover",{"input":s["directory"]})["recovered_parts"]==0

        # A recording must stop/save before a total disk budget runs away.
        s=m.tool("framelet_start_series",{**common,"name":"budget","until_stop":True,"segment_seconds":30,"max_mib":1,"total_mib":3})
        sid=s["session_id"]
        m.wait(sid,lambda s:s.get("recorded_ms",0)>=400)
        (Path(s["directory"])/"budget-test-filler.bin").write_bytes(b"x"*(3*1024*1024))
        done=m.wait(sid,lambda s:s["terminal"])
        assert done["result"]["recording"]["reason"]=="disk_budget",done
        validate(exe,s["directory"])

        # A normal stdio EOF saves the active tail.
        s=m.tool("framelet_start_series",{**common,"name":"eof","until_stop":True,"segment_seconds":30})
        m.wait(s["session_id"],lambda s:s.get("recorded_ms",0)>=400)
    finally:
        m.close()
    eof=load(Path(s["directory"])/"recording.json")
    assert eof["state"]=="completed" and eof["reason"]=="user_stop" and eof["parts"],eof
    print("PASS real short rotations, identity, strict timelines, exact boundary, pause/stop, request idempotency, MCP global probes and crash/checkpoint recovery", flush=True)


def long(exe, root,seconds=650):
    log = root / "long-stderr.log"
    cmd = [exe, "record-series", "--output-dir", str(root), "--name", f"real-{seconds}s",
           "--duration", str(seconds), "--region", "0,0,64,64", "--mask", "0,0,64,64", "--fps", "1"]
    began = time.monotonic()
    peak = 0
    samples = []
    with log.open("w", encoding="utf-8") as err:
        p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=err, text=True, encoding="utf-8")
        while p.poll() is None:
            elapsed = time.monotonic() - began
            rss = sum(x.memory_info().rss for x in [psutil.Process(p.pid), *psutil.Process(p.pid).children()]
                      if x.is_running())
            peak = max(peak, rss)
            samples.append({"elapsed": elapsed, "summed_rss": rss})
            if len(samples) % 15 == 1:
                print(f"REAL long recording: {elapsed:.0f}s elapsed; host+worker RSS {rss/1048576:.1f} MiB", flush=True)
            assert elapsed < seconds+50, "long recording exceeded deadline"
            time.sleep(2)
        stdout = p.stdout.read()
        assert p.returncode == 0, log.read_text(encoding="utf-8")
    result = json.loads(stdout)["result"]
    assert result["state"] == "completed", result
    expected=[300_000]*(seconds//300)+([seconds%300*1000] if seconds%300 else [])
    index = validate(exe, result["directory"], expected)
    proof = {"ok": True, "exe_sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(),
             "elapsed_seconds": time.monotonic() - began, "summed_rss_peak": peak,
             "memory_note": "Host + worker RSS sum includes shared pages; not unique physical memory.",
             "scenario": "real 64x64 GDI capture, full privacy mask, 1 FPS; lifecycle test, not motion benchmark",
             "result": result, "index": index, "memory_samples": samples}
    (root / "long-verification.json").write_text(json.dumps(proof, ensure_ascii=False, indent=2), encoding="utf-8")
    print(f"PASS real {seconds}-second capture -> {expected}ms; all parts independently decode and validate", flush=True)


def long_mcp(exe,root):
    m=MCP(exe,root)
    samples=[]
    began=time.monotonic()
    args={"output_dir":str(root),"name":"real-mcp-650s","request_id":"mcp-real-650s","duration":650,"fps":1,
          "region":{"x":0,"y":0,"width":64,"height":64},"masks":[{"x":0,"y":0,"width":64,"height":64}]}
    try:
        s=m.tool("framelet_start_series",args)
        assert time.monotonic()-began<2,"start must return immediately"
        sid=s["session_id"]
        while True:
            tick=time.monotonic()
            status=m.status(sid)
            assert time.monotonic()-tick<2,"status blocked behind recording"
            elapsed=time.monotonic()-began
            rss=0
            for proc in [psutil.Process(m.p.pid),*psutil.Process(m.p.pid).children()]:
                try:rss+=proc.memory_info().rss
                except psutil.NoSuchProcess:pass
            samples.append({"elapsed":elapsed,"summed_rss":rss,"status":status})
            if len(samples)%15==1:print(f"REAL MCP recording: {elapsed:.0f}s elapsed; {status.get('recorded_ms',0)}ms media; RSS {rss/1048576:.1f} MiB",flush=True)
            if status["terminal"]:break
            assert elapsed<700,"MCP long recording deadline"
            time.sleep(2)
        assert status["result"]["state"]=="completed",status
        idx=validate(exe,s["directory"],[300000,300000,50000])
        probe=m.tool("framelet_frame_at",{"input":s["directory"],"at":420,"output":str(root/"global-420s.png")})
        assert probe["segment_start_ms"]==300000
        m.tool("framelet_timeline",{"input":s["directory"],"at":420})
        sheet=m.tool("framelet_extract_frames",{"input":s["directory"],"output":str(root/"global-overview"),"count":3,"mode":"uniform"})
        assert len(sheet["frames"])==3
        assert m.tool("framelet_start_series",args)["existing"]
        proof={"ok":True,"transport":"real MCP stdio","exe_sha256":hashlib.sha256(Path(exe).read_bytes()).hexdigest(),
               "elapsed_seconds":time.monotonic()-began,"scenario":"64x64 real GDI capture, full privacy mask, 1 FPS; lifecycle/durability test, not a motion benchmark",
               "result":status,"index":idx,"memory_samples":samples,"summed_rss_peak":max(x["summed_rss"] for x in samples),
               "memory_note":"Host + worker RSS includes shared pages; not unique physical memory."}
        (root/"long-verification.json").write_text(json.dumps(proof,ensure_ascii=False,indent=2),encoding="utf-8")
        print("PASS real MCP 650s -> 300 + 300 + 50; global 420s probe, timeline, contact sheet, idempotency",flush=True)
    finally:m.close()


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("exe")
    p.add_argument("--long", action="store_true")
    p.add_argument("--mcp-long",action="store_true")
    p.add_argument("--seconds",type=int,default=650)
    p.add_argument("--output", required=True)
    a = p.parse_args()
    root = Path(a.output).resolve()
    root.mkdir(parents=True, exist_ok=False)
    exe = str(Path(a.exe).resolve())
    long_mcp(exe,root) if a.mcp_long else long(exe, root,a.seconds) if a.long else short(exe, root)
