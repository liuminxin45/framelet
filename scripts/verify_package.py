"""Smoke-test the extracted deliverable without requiring a capture desktop."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(sys.argv[1]).resolve()
exe = root / "framelet.exe"
checksums = (root / "SHA256SUMS.txt").read_text(encoding="ascii").split()
assert checksums[0] == hashlib.sha256(exe.read_bytes()).hexdigest()
assert checksums[1] == "framelet.exe"
for name in ["AGENTS.md", "docs/engineering-standard.md", "docs/agent-guide.md", "docs/performance.md", "docs/mcp.md", "THIRD_PARTY_NOTICES.md", "dependencies.json"]:
    assert (root / name).is_file(), name
r = subprocess.run([str(exe), "schema"], capture_output=True, text=True, encoding="utf-8", timeout=15, cwd=root)
assert r.returncode == 0, r.stderr
catalog = json.loads(r.stdout)
assert catalog["schema_version"] == "3.0"
assert len(catalog["tools"]) == 23
assert all(t["name"] != "framelet_record" for t in catalog["tools"])
print(json.dumps({"ok": True, "version": catalog["version"], "tools": len(catalog["tools"]), "exe_sha256": checksums[0]}))
