# Framelet

**Small binary. Zero extra runtime installs. Callable by AI through MCP.**

**小体积 · 无需额外运行时依赖 · AI 可通过 MCP 调用**

[English](README.md) | [简体中文](README.zh-CN.md)

[![Windows CI](https://github.com/liuminxin45/framelet/actions/workflows/ci.yml/badge.svg)](https://github.com/liuminxin45/framelet/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/liuminxin45/framelet)](https://github.com/liuminxin45/framelet/releases/latest)
[![MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

Framelet is a tiny native Windows screen recorder built with Rust and Win32. Its biggest advantage is a small, self-contained executable: **unzip and run**, or let an AI agent call it over local MCP stdio. No Python, Node.js, .NET, FFmpeg, browser runtime, model SDK or account is needed to run the recorder.

The v0.1.0 EXE is about **0.88 MiB**; the development build with frame extraction is around **1 MiB**. Both use Windows system APIs and a static CRT. Build tools and test SDKs are development dependencies only.

## Download

[**Windows x64 portable v0.1.0**](https://github.com/liuminxin45/framelet/releases/download/v0.1.0/framelet-v0.1.0-windows-x64.zip) · [Releases](https://github.com/liuminxin45/framelet/releases) · [SHA-256](https://github.com/liuminxin45/framelet/releases/download/v0.1.0/SHA256SUMS.txt)

Requires an interactive Windows 10 2004+ / Windows 11 x64 desktop. Extract the ZIP and run `framelet.exe`. License texts are included.

**Version note:** v0.1.0 has recording and seven MCP tools. Frame extraction and MCP image previews are in **main / 0.2.0-dev**; build from source to use them. Published v0.1.0 assets remain unchanged.

## Real window and recording

![Framelet's native transparent capture frame](docs/images/framelet-window.jpg)

This real recording is 620×300, 3 seconds, 16 FPS, 48 frames and 15,250 bytes. It is a mostly static test screen with a moving marker, rather than a general compression benchmark.

![GIF recorded by Framelet](docs/images/framelet-demo.gif)

## Window and CLI

Move or resize the transparent frame. **Record / F8** starts and finishes recording. **Pause / Resume** excludes paused time. **Frame** starts a paused single-frame session or appends a frame. **Save as...** copies a finished GIF to a new path; **Open folder** opens auto-saved recordings.

Auto-save: `%LOCALAPPDATA%\Framelet\Captures`. The capture region stays fixed while recording. Closing an active recording first saves it; close again to exit. The GUI offers 10 / 16 / 20 / 25 FPS and cursor capture.

```powershell
.\framelet.exe
.\framelet.exe record --region 100,100,640,360 --duration 3 --fps 16 --cursor --output demo.gif
.\framelet.exe shot --region 100,100,640,360 --output shot.png
.\framelet.exe inspect demo.gif
.\framelet.exe edit demo.gif --start 0.5 --end 2 --speed 2 --output trimmed.gif
.\framelet.exe doctor
.\framelet.exe monitors
.\framelet.exe windows
.\framelet.exe record --duration 60 --session demo --events --output long.gif
# From another process:
.\framelet.exe stop --session demo
```

Coordinates are physical pixels; use `--region=-1920,0,1280,720` for a negative origin. Without a region, capture covers the virtual desktop within a 16 MP limit. Existing outputs are never replaced; create the parent directory first. Capture reads currently visible pixels, with Windows occlusion/capture-protection behavior.

CLI FPS is 1–50, duration is (0,3600] seconds, and quality is 1–30 (1 best, 30 fastest). Ctrl+C finalizes a valid GIF. Identical samples merge their delay; changed frames encode a bounding rectangle and stream to disk. Results are JSON on stdout; `--events` emits JSONL progress on stderr. Operation errors exit 1 with JSON; syntax errors exit 2. Help/version are text.

## AI through MCP

The executable is the MCP server. **No separate server runtime or model API key is required.** Starting it does not record until a capture tool is called.

```powershell
codex mcp add framelet -- C:\Tools\Framelet\framelet.exe mcp
codex mcp get framelet --json
```

Use your EXE's absolute path. Restart the MCP connection/client after registering or updating it. For long recordings, set `tool_timeout_sec = 330` under `[mcp_servers.framelet]` in Codex's `config.toml`; MCP recordings are blocking and limited to 300 seconds.

Other stdio clients can use:

```json
{"mcpServers":{"framelet":{"command":"C:\\Tools\\Framelet\\framelet.exe","args":["mcp"]}}}
```

| Tools | Purpose |
| --- | --- |
| `framelet_doctor`, `framelet_monitors`, `framelet_windows` | Readiness and capture geometry |
| `framelet_record`, `framelet_shot` | GIF recording and PNG screenshot |
| `framelet_inspect`, `framelet_edit` | GIF metadata, trim and playback speed |
| `framelet_extract_frames` *(main)* | PNG candidates, contact-sheet image, timestamps and context ranges |
| `framelet_read_image` *(main)* | View a selected PNG as an MCP image block |

In main, screenshots and extraction contact sheets return **actual MCP image blocks**, alongside structured results. Previews are bounded to 960px and 1 MiB. `framelet schema` prints the versioned catalog. No cloud upload, shell-execution or model-understanding service is embedded.

## Frames for AI review — main / 0.2.0-dev

**Native GIF decoding, without FFmpeg or another runtime.** Extraction reconstructs full frames from delta rectangles, transparency and disposal modes before exporting PNGs.

```powershell
.\framelet.exe frames demo.gif --mode scene --count 8 --output frames
.\framelet.exe frames demo.gif --mode uniform --count 8 --output overview
```

The new directory contains numbered PNGs, `contact-sheet.png` and `manifest.json`, with source frame indexes, timestamps, change scores, selection reasons and suggested context ranges. `--count` is a maximum of 1–64; duplicate uniform time samples are deduplicated. Scene mode keeps opening context and selects changes above `--min-change` (default 0.04). `--min-gap` controls spacing (default 0.5 seconds), and `--padding` controls context (default 0.75 seconds).

An AI can extract frames, review the contact sheet, call `framelet_read_image` for details, then call `framelet_edit` with the chosen start/end in seconds. **Visual change is not semantic importance.** The agent must inspect images to decide which event matters; suggested ranges are context windows, not automatic semantic highlights. Input is currently **GIF only**, not MP4 or other video containers.

![MCP contact sheet from a synthetic READY / ERROR / DONE test GIF](docs/images/framelet-frames.png)

This synthetic interoperability fixture illustrates the workflow: the agent reviewed the ERROR frame at 1.0–2.2 seconds and exported 0.8–2.4 seconds with context through MCP. It is a test example, not an automatic highlight-quality benchmark.

## Build and verify

Rust stable, MSVC C++ Build Tools and the Windows SDK are required **to build**, not to run the portable EXE.

```powershell
git clone https://github.com/liuminxin45/framelet.git
cd framelet
cargo build --release --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python -m pip install pillow
python scripts/verify.py target/release/framelet.exe --desktop
python scripts/verify_frames.py target/release/framelet.exe
# Official SDK interoperability: development dependency only.
npm install --prefix work/mcp-client @modelcontextprotocol/sdk@1.32.2 --ignore-scripts --no-audit --no-fund
node scripts/verify_mcp.mjs target/release/framelet.exe docs/images/framelet-demo.gif --desktop
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
```

Omit `--desktop` on headless runners. Audio, AVI/MP4, a visual timeline, per-frame deletion, overlays, richer palette controls and hidden-window capture are not implemented. GIF edits flatten transparency onto black. The frame excludes itself from capture by default; `FRAMELET_CAPTURE_UI=1` is a screenshot-QA override.

An independent implementation inspired by [GifCam](https://www.bahraniapps.com/apps/gifcam/gifcam.php), with no affiliation or redistributed executable, reference icons or decompiled code. See [investigation notes](docs/research.md), [design](docs/design.md), [changelog](CHANGELOG.md). MIT licensed; [issues](https://github.com/liuminxin45/framelet/issues) and pull requests are welcome.
