<div align="center">

# Framelet

**A tiny screen recorder that your AI can actually use.**<br>
**约 1.1 MiB · 无需额外运行时 · AI 可通过 MCP 调用**

[English](README.md) · [简体中文](README.zh-CN.md)

[![Windows](https://img.shields.io/badge/Windows-10%20%2F%2011-0078D4)](#download)
[![Preview](https://img.shields.io/badge/preview-v0.2.0--rc.2-0891B2)](https://github.com/liuminxin45/framelet/releases/tag/v0.2.0-rc.2)
[![Windows CI](https://github.com/liuminxin45/framelet/actions/workflows/ci.yml/badge.svg)](https://github.com/liuminxin45/framelet/actions/workflows/ci.yml)
[![MIT](https://img.shields.io/badge/license-MIT-52525B)](LICENSE)

[**Download portable**](https://github.com/liuminxin45/framelet/releases/download/v0.2.0-rc.2/framelet-v0.2.0-rc.2-windows-x64.zip) · [Quick start](#quick-start) · [MCP setup](docs/mcp.md) · [Examples](#example-1--real-window-and-recording)

</div>

Framelet turns a visible part of your Windows desktop into a GIF or PNG. Double-click for a small native capture frame, use the CLI, or let an AI agent start, stop, inspect and trim recordings through **MCP stdio**.

Its biggest advantage: **one small EXE, about 1.1 MiB, with zero extra runtime installs**. No Python, Node.js, .NET, FFmpeg, browser runtime, account or model API key is required to run Framelet. Windows system APIs do the capture; Rust and a static CRT keep it self-contained.

## Example 1 · Real window and recording

![Framelet's native transparent capture frame](docs/images/framelet-window.jpg)

The original small demo is preserved: **620×300, 3 seconds, 16 FPS, 48 frames, 15,250 bytes**. It is a real Framelet recording of a mostly static test scene with a moving marker, rather than a general compression benchmark.

![Original GIF recorded by Framelet](docs/images/framelet-demo.gif)

## Download

[**v0.2.0-rc.2 · Windows x64 portable ZIP**](https://github.com/liuminxin45/framelet/releases/download/v0.2.0-rc.2/framelet-v0.2.0-rc.2-windows-x64.zip) · [SHA-256](https://github.com/liuminxin45/framelet/releases/download/v0.2.0-rc.2/SHA256SUMS.txt) · [All releases](https://github.com/liuminxin45/framelet/releases)

Extract and run `framelet.exe`. Requires an interactive Windows 10 2004+ / Windows 11 x64 desktop. The preview includes the new MCP sessions, timestamp probes and guardrails; [v0.1.0](https://github.com/liuminxin45/framelet/releases/tag/v0.1.0) remains the stable release. The portable ZIP is under 1 MiB; demo media is distributed separately. Build/test tools are development dependencies only.

## Quick start

```powershell
# Small native capture frame: Record / F8, Pause, Frame, Save as
.\framelet.exe

# Record a region; wait 2 seconds, then downscale while preserving aspect ratio
.\framelet.exe record --region 100,100,1280,720 --duration 15 --fps 12 --delay 2 --max-width 960 --output demo.gif

# Keep a useful segment, then export four PNGs for a tutorial or slide deck
.\framelet.exe edit demo.gif --start 2 --end 8 --output clip.gif
.\framelet.exe frames clip.gif --mode uniform --count 4 --output slides
```

Coordinates are physical pixels; `monitors` and `windows` discover bounds. Output must be new, and its parent directory must exist. Capture reads visible desktop pixels; an overlapping window can appear in a recording. Requested FPS is a ceiling, and GIF delays preserve elapsed time when capture/encoding is slower.

### Choose FPS by intent (CLI / MCP)

Supported range: **1–50 FPS**. These are starting points; choose the evidence you need to preserve:

| Intent | Suggested FPS | Why |
| --- | --- | --- |
| Stable UI acceptance, long-running progress | **1** | Stable states only; use `shot` for just the final result |
| Development workflow, forms and state changes | **3–5** | Before/after states, rather than brief glitches or animation |
| Compact README GIF, step-by-step tutorial | **10–15** | Favor size when smooth motion is secondary |
| Polished software demo, scrolling, dragging, transitions | **30** | Default for audience-facing demos; preserve motion continuity |
| Fast motion, brief visual defects | **40–50** | Check performance with a short trial; shorter transients may still be missed |

**Development testing does not automatically mean 1 FPS is enough.** At 1 FPS, sub-second dialogs, flicker or errors may be missed. Default to 30 FPS for polished demos; deliberately use 10–15 FPS when compact output matters more.

Start with a short trial at `max_width:960` or `1280`. Estimate actual sampling speed from `recording_status` samples and recorded elapsed time, not encoded GIF frame count: static frames may be merged. If capture is slow, reduce the region or output size first; explain the compromise if lowering demo FPS is necessary. Encoded width × height × FPS must be ≤80,000,000. Requested FPS is a ceiling; GIF centisecond delay resolution also means 30 FPS does not guarantee uniform 33.33 ms intervals. MCP exposes the same advice in `framelet_context.capture_guidance`.

| Make something | Use Framelet for |
| --- | --- |
| README / social post | A short GIF, controlled width and a precise time cut |
| Tool / website walkthrough | Background MCP recording while the AI operates the UI |
| Slides / tutorial | Full-resolution PNGs, a contact sheet and timestamps |
| A cleaner demo | Countdown, pause/resume, speed changes and fixed privacy masks |

<details>
<summary>Native window, keyboard controls and CLI details</summary>

Move/resize the transparent frame; its opening is click-through. **Record / F8** starts or finishes; **Pause / Resume** omits waiting time. **Frame** starts a paused single-frame session or appends another image. **Save as...** copies a finished GIF without replacing existing files. Auto-save: `%LOCALAPPDATA%\Framelet\Captures`. Geometry stays fixed during recording; closing first saves, then a second close exits. GUI FPS choices: 10 / 16 / 20 / 25, with optional cursor capture. Countdown, masks and downscale are CLI/MCP options.

```powershell
.\framelet.exe doctor
.\framelet.exe monitors
.\framelet.exe windows
.\framelet.exe shot --region 100,100,640,360 --output shot.png
# Masks use coordinates relative to the source region; repeat --mask for more
.\framelet.exe record --region 100,100,1280,720 --duration 15 --mask 1000,0,280,70 --max-width 960 --max-mib 64 --output private-demo.gif
.\framelet.exe record --duration 60 --session demo --events --output long.gif
# From another process; Ctrl+C also finalizes a CLI recording
.\framelet.exe stop --session demo
```

Negative origin: `--region=-1920,0,1280,720`. Without `--region`, CLI recording captures the virtual desktop within its limits. CLI duration: (0,3600] seconds; FPS: 1–50; quality: 1–30 (1 best, 30 fastest). JSON results go to stdout; `--events` sends JSONL progress to stderr. Operation errors exit 1 with JSON; command syntax errors exit 2. Help/version are text.

</details>

## AI through MCP

**The EXE is the server.** Your AI client launches the configured absolute command with `mcp`; there is no separate daemon to start. The process runs on the Windows computer being captured. Initialization alone never records. [Bilingual installation guide and all 17 tools →](docs/mcp.md)

```json
{"mcpServers":{"framelet":{"command":"C:\\Tools\\Framelet\\framelet.exe","args":["mcp"]}}}
```

The agent first calls `framelet_context` to discover the executable and output root, then uses:

```text
windows / monitors → start_recording → recording_status → operate the browser
                   → stop_recording → frame_at / extract_frames → edit
```

Start returns a `session_id`. Wait for `recording` with at least one sample before operating the UI. Stop saves; cancel discards unfinished output; pause excludes waiting time. MCP images are real image content blocks, bounded to 960px / 1 MiB. The client controls the browser; Framelet supplies recording and image tools.

## Example 2 · Codex browses Bilibili

[![Real Bilibili scrolling recorded and trimmed through Framelet MCP](docs/images/bilibili-demo.gif)](docs/demo.md)

**Real browser. Real MCP calls. Native GIF output.** 29.88 s → 7.63 s, 960×505, 30 GIF frames, 3.61 MiB. [Static preview](docs/images/bilibili-poster.png) · [Recording & evidence](docs/demo.md)

Open [Bilibili](https://www.bilibili.com/) and the homepage brings together music, knowledge, technology and other categories. As you scroll into recommendations, its large header gives way to a compact fixed navigation bar. A still screenshot misses that transition; a short screen recording preserves it.

For this walkthrough, Codex called Framelet over MCP, scrolled the public homepage in Edge, and stopped the recording. It then found the action boundaries with **12 midpoint probes plus 3 anchor probes**, rather than exporting all 137 source frames for the model. Independent PNG region comparisons guided the search; the AI visually reviewed five boundary images. The result keeps the start and finish of the scroll, with a little context: **14.56–22.19 seconds at original speed**.

![The real recording's retained interval](docs/images/bilibili-trim.svg)

This bisection recipe assumes one forward scrolling segment and stable opening/closing page positions. It is not semantic highlight detection. GIF prefix decoding is still sequential: the saving is in images exported and sent to the model, not guaranteed logarithmic decoder work. [Full bilingual case study, raw recording and machine-readable evidence →](docs/demo.md)

<details>
<summary>Four PNG frames from the finished clip</summary>

![Four frames extracted through MCP](docs/images/bilibili-storyboard.png)

The MCP extraction returned four full-resolution PNGs, this contact sheet and a timestamp manifest. `framelet_read_image` was also tested on the returned image.

</details>

## Review frames without flooding the model

[Agent editing recipe →](docs/agent-guide.md) · Also returned by `framelet_context`.

The efficient path is **stable-region difference + playback-time bisection + frame-interval caching**. `framelet_frame_at` can compare against an anchor PNG and return numerical differences with `include_image:false`; omit `output` to avoid writing a PNG. On the same Bilibili source, interval jumps reduced midpoint calls from **12 to 8**, with only **2 anchor images** returned during the numeric search. Final visual review is still required. [Native replay evidence](docs/demos/bilibili-native-bisection.json)


```powershell
# Ask for a single time point, then halve a known static-to-action interval
.\framelet.exe frame-at demo.gif --at 4.5 --output midpoint.png

# Or get a small overview for an unfamiliar recording
.\framelet.exe frames demo.gif --mode scene --count 6 --output review
```

`framelet_frame_at` returns the frame covering a playback timestamp, with its frame index and exact interval. `framelet_extract_frames` exports composited PNGs, a contact sheet, timestamps and suggested context ranges. Uniform mode samples time; scene mode ranks visual change. **Visual change does not establish semantic importance**; an AI must review images to decide what matters. GIF input only.

## Guardrails that matter

- **Explicit capture:** MCP requires a region or `full_screen:true`; nothing records on connect. Fixed masks redact before scaling/encoding, with no automatic secret detection or tracking.
- **Bounded sessions:** one active recording per MCP server; maximum 300 seconds, including pauses, plus optional countdown. EOF cancels unfinished background work. Stop/cancel are safe to repeat on completed sessions.
- **Safe file handling:** no overwrites, atomic GIF/PNG publication, scoped MCP read/output roots, traversal/device/network-path guards and handled-error cleanup. These are local mistake guards, not an OS sandbox.
- **Resource and protocol checks:** dimension, throughput, byte and GIF decoding budgets; malformed or oversized requests do not break subsequent requests. No built-in upload, telemetry, shell execution or audio capture.

[Limits, failure behavior and configuration →](docs/mcp.md). GDI captures visible pixels; secure/protected content, HDR management and hidden-window capture are outside the verified scope. Direct MP4/audio export, a visual timeline and text overlays are not implemented. GIF edits flatten transparent pixels onto black.

## Build and verify

Building needs Rust stable, MSVC C++ Build Tools and Windows SDK. Running the portable EXE needs none of those.

```powershell
cargo build --release --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python -m pip install pillow
python scripts/verify.py target/release/framelet.exe --desktop
python scripts/verify_frames.py target/release/framelet.exe
python scripts/verify_robustness.py target/release/framelet.exe --desktop
npm install --prefix work/mcp-client @modelcontextprotocol/sdk@1.32.2 --ignore-scripts --no-audit --no-fund
node scripts/verify_mcp.mjs target/release/framelet.exe --desktop
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
```

Omit `--desktop` on headless CI. Pillow and the official MCP SDK are independent development verifiers, never recorder runtime dependencies. [Design](docs/design.md) · [Changelog](CHANGELOG.md) · [Report an issue](https://github.com/liuminxin45/framelet/issues)

Built by [Neo / 刘民心](https://liuminxin.cn). Independent implementation inspired by [GifCam](https://www.bahraniapps.com/apps/gifcam/gifcam.php), with no affiliation or redistributed reference executable, icons or decompiled source. [Research notes](docs/research.md) · [About](ABOUT.md) · MIT.
