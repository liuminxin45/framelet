# Development and verification / 开发与验证

## Build and verify

Building needs Rust stable, MSVC C++ Build Tools and Windows SDK. Running the portable EXE needs none of those.

```powershell
cargo build --release --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python -m pip install pillow psutil
python scripts/verify.py target/release/framelet.exe --desktop
python scripts/verify_frames.py target/release/framelet.exe
python scripts/verify_timeline.py target/release/framelet.exe --desktop
python scripts/verify_defense.py target/release/framelet.exe
python scripts/verify_sync.py target/release/framelet.exe
python scripts/verify_series.py target/release/framelet.exe --output work/series-short
# Explicit real-time acceptance (about 11 minutes each; creates a fresh evidence directory):
python scripts/verify_series.py target/release/framelet.exe --long --output work/series-cli-650
python scripts/verify_series.py target/release/framelet.exe --mcp-long --output work/series-mcp-650
python scripts/fuzz_regression.py target/release/framelet.exe --iterations 1000 --seed 42 --output work/fuzz-results.json
python scripts/audit_dependencies.py --output work/dependency-audit.json
python scripts/verify_robustness.py target/release/framelet.exe --desktop
npm install --prefix work/mcp-client @modelcontextprotocol/sdk@1.32.2 --ignore-scripts --no-audit --no-fund
node scripts/verify_mcp.mjs target/release/framelet.exe --desktop
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
```

Omit `--desktop` on headless CI. Pillow and the official MCP SDK are independent development verifiers, never recorder runtime dependencies. [Design](design.md) · [Changelog](../CHANGELOG.md) · [Report an issue](https://github.com/liuminxin45/framelet/issues)

Built by [Neo / 刘民心](https://liuminxin.cn). Independent implementation inspired by [GifCam](https://www.bahraniapps.com/apps/gifcam/gifcam.php), with no affiliation or redistributed reference executable, icons or decompiled source. [Research notes](research.md) · [About](../ABOUT.md) · MIT.


[Architecture and failure contract](architecture.md) · [Code audit](code-audit.md). Fault injection includes hostile sidecars, bounded diagnostic rotation, unwritable logs and a small deterministic GIF mutation corpus. It is not a sustained coverage-guided fuzzer.
