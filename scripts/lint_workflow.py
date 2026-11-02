"""Validate Actions syntax/expressions with a pinned, hash-verified actionlint."""
import hashlib
import io
from pathlib import Path
import subprocess
import urllib.request
import zipfile

version = "1.7.12"
sha256 = "6e7241b51e6817ea6a047693d8e6fed13b31819c9a0dd6c5a726e1592d22f6e9"
url = f"https://github.com/rhysd/actionlint/releases/download/v{version}/actionlint_{version}_windows_amd64.zip"
with urllib.request.urlopen(url, timeout=45) as response:
    payload = response.read(16 * 1024 * 1024 + 1)
assert len(payload) <= 16 * 1024 * 1024
assert hashlib.sha256(payload).hexdigest() == sha256, "actionlint asset digest mismatch"
root = Path("work/workflow-lint")
root.mkdir(parents=True, exist_ok=True)
exe = (root / "actionlint.exe").resolve()
with zipfile.ZipFile(io.BytesIO(payload)) as archive:
    exe.write_bytes(archive.read("actionlint.exe"))
subprocess.run([str(exe), "-color", ".github/workflows/ci.yml"], check=True, timeout=30)
print(f"PASS actionlint {version} (verified SHA-256)")
