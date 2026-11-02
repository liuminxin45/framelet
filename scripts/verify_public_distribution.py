"""Verify public assets and GitHub marketplace installation without model inference."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import urllib.request

REPO = "liuminxin45/framelet"


def fetch(url, limit=32 * 1024 * 1024):
    request = urllib.request.Request(url, headers={"User-Agent": "Framelet-public-acceptance"})
    with urllib.request.urlopen(request, timeout=60) as response:
        data = response.read(limit + 1)
    assert len(data) <= limit, "Public response exceeds byte budget"
    return data


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True)
    parser.add_argument("--claude", required=True)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parent.parent
    out = Path(args.output).resolve()
    out.mkdir(parents=True, exist_ok=False)
    proof = {"ok": False, "model_turns": 0, "gui_installation_claimed": False}
    try:
        server = json.loads((repo / "server.json").read_text(encoding="utf-8"))
        source = json.loads((repo / ".claude-plugin/marketplace.json").read_text(encoding="utf-8"))["plugins"][0]["source"]
        tag = "v" + server["version"]
        prefix = f"https://github.com/{REPO}/releases/download/{tag}/"
        assert source["source"] == "archive"
        assert source["url"] == prefix + f"framelet-{tag}-claude-plugin.zip"
        release = json.loads(fetch(f"https://api.github.com/repos/{REPO}/releases/tags/{tag}"))
        assert not release["draft"]
        proof.update(release=release["html_url"], catalog_commit=subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=repo, text=True, timeout=10).strip(), assets=[])
        digests = {}
        for asset in release["assets"]:
            assert asset["browser_download_url"].startswith(prefix)
            digest = hashlib.sha256(fetch(asset["browser_download_url"])).hexdigest()
            assert asset["digest"] == "sha256:" + digest, "Public asset digest mismatch"
            digests[asset["name"]] = digest
            proof["assets"].append({"name": asset["name"], "sha256": digest})
        assert digests[source["url"].rsplit("/", 1)[1]] == source["sha256"]
        package = server["packages"][0]
        assert package["identifier"].startswith(prefix)
        assert digests[package["identifier"].rsplit("/", 1)[1]] == package["fileSha256"]
        env = {**os.environ, "PYTHONUTF8": "1", "CLAUDE_CONFIG_DIR": str(out / "claude"),
               "APPDATA": str(out / "appdata"), "LOCALAPPDATA": str(out / "local")}
        claude = str(Path(args.claude).resolve())
        commands = [["plugin", "marketplace", "add", REPO],
                    ["plugin", "install", "framelet@framelet", "--config",
                     "capture_directory=" + str(out / "captures"), "--json"], ["mcp", "list"]]
        for index, command in enumerate(commands):
            result = subprocess.run([claude, *command], env=env, cwd=out, capture_output=True,
                                    text=True, encoding="utf-8", timeout=150)
            (out / f"claude-{index}.log").write_text(result.stdout + "\n" + result.stderr, encoding="utf-8")
            assert result.returncode == 0, result.stdout + result.stderr
            if index == 1:
                assert json.loads(result.stdout)["configApplied"] is True, "Capture directory was not configured"
        assert "framelet" in result.stdout and "Connected" in result.stdout, result.stdout
        catalog = out / "claude/plugins/marketplaces/framelet/.claude-plugin/marketplace.json"
        assert json.loads(catalog.read_text(encoding="utf-8"))["plugins"][0]["source"] == source
        cached = list((out / "claude/plugins/cache/framelet/framelet").rglob("framelet.exe"))
        assert len(cached) == 1, "Expected one extracted native MCPB executable"
        digest = hashlib.sha256(cached[0].read_bytes()).hexdigest()
        assert (cached[0].parent / "SHA256SUMS.txt").read_text(encoding="ascii").split()[0] == digest
        proof.update(ok=True, exe_sha256=digest, claude_github_marketplace_connected=True,
                     capture_directory_configured=True, archive_sha256_verified=True)
    except Exception as error:
        proof["error"] = str(error)[:2048]
    (out / "verification.json").write_text(json.dumps(proof, indent=2), encoding="utf-8")
    print(json.dumps(proof))
    if not proof["ok"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
