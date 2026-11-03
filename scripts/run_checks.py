"""One bounded acceptance entry point for developers and GitHub Actions."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

import psutil


def terminate_tree(p):
    try:
        children = psutil.Process(p.pid).children(recursive=True)
    except psutil.NoSuchProcess:
        children = []
    for child in reversed(children):
        try:
            child.kill()
        except psutil.NoSuchProcess:
            pass
    if p.poll() is None:
        p.kill()
    p.wait(timeout=10)
    _, alive = psutil.wait_procs(children, timeout=10)
    if alive:
        raise RuntimeError(f"test timeout left live children: {[c.pid for c in alive]}")


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("exe")
    parser.add_argument("--output", required=True)
    parser.add_argument("--suite", choices=["core", "long-cli", "long-mcp"], default="core")
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args()
    exe = str(Path(args.exe).resolve())
    root = Path(args.output).resolve()
    root.mkdir(parents=True, exist_ok=False)
    desktop = ["--desktop"] if args.desktop else []
    py = sys.executable
    if args.suite == "core":
        commands = [(name, [py, f"scripts/{name}.py", exe, *extra], 180) for name, extra in [
            ("verify", desktop), ("verify_frames", []), ("verify_timeline", desktop),
            ("verify_defense", []), ("verify_sync", []), ("verify_robustness", desktop),
            ("verify_series", ["--output", str(root / "series")]),
            ("verify_60fps", ["--output", str(root / "fps60")]),
            ("verify_interruptions", ["--output", str(root / "interruptions")]),
            ("verify_performance", ["--output",str(root / "performance")]),
            ("verify_review_algorithms", ["--output",str(root / "review-algorithms")]),
            ("verify_demo", ["--output",str(root / "demo")]),
            ("verify_integration", ["--output",str(root / "integration")]),
            ("fuzz_regression", ["--iterations", "1000", "--seed", "20261102", "--output", str(root / "fuzz.json")]),
        ]]
        commands.append(("mcp-sdk", ["node", "scripts/verify_mcp.mjs", exe, *desktop], 180))
    else:
        mode = "--long" if args.suite == "long-cli" else "--mcp-long"
        commands = [(args.suite, [py, "scripts/verify_series.py", exe, mode, "--output", str(root / "long")], 760)]
    proof = {"ok": False, "suite": args.suite, "desktop": args.desktop,
             "exe_sha256": hashlib.sha256(Path(exe).read_bytes()).hexdigest(), "checks": []}
    summary = root / "summary.json"
    for name, cmd, timeout in commands:
        began = time.monotonic()
        record = {"name": name, "timeout_seconds": timeout}
        with (root / f"{name}.log").open("w", encoding="utf-8") as log:
            p = None
            try:
                p = subprocess.Popen(cmd, stdout=log, stderr=subprocess.STDOUT,
                                     env={**os.environ, "PYTHONUTF8": "1"})
                record["exit_code"] = p.wait(timeout=timeout)
            except subprocess.TimeoutExpired:
                record["timeout"] = True
                try:
                    terminate_tree(p)
                except Exception as e:
                    record["cleanup_error"] = str(e)
            except Exception as e:
                record["runner_error"] = str(e)
                if p is not None and p.poll() is None:
                    try:
                        terminate_tree(p)
                    except Exception as cleanup:
                        record["cleanup_error"] = str(cleanup)
        record["elapsed_seconds"] = round(time.monotonic() - began, 3)
        record["ok"] = record.get("exit_code") == 0 and not any(
            record.get(key) for key in ["timeout", "runner_error", "cleanup_error"])
        proof["checks"].append(record)
        summary.write_text(json.dumps(proof, indent=2), encoding="utf-8")
        print(f"{'PASS' if record['ok'] else 'FAIL'} {name} ({record['elapsed_seconds']}s)", flush=True)
        if record.get("cleanup_error"):
            break  # Do not start another test beside a process we could not control.
    proof["ok"] = all(c["ok"] for c in proof["checks"])
    summary.write_text(json.dumps(proof, indent=2), encoding="utf-8")
    if os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(os.environ["GITHUB_STEP_SUMMARY"], "a", encoding="utf-8") as f:
            f.write(f"\n{args.suite} EXE SHA-256: `{proof['exe_sha256']}`\n\n")
            for c in proof["checks"]:
                f.write(f"- {'PASS' if c['ok'] else 'FAIL'} {c['name']}: {c['elapsed_seconds']}s\n")
    return 0 if proof["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
