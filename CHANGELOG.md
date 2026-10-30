# Changelog

## v0.2.0-rc.1 — 2026-10-30

Acceptance preview; v0.1.0 remains the stable release.

- 17 MCP tools, including background start/status/pause/resume/stop/cancel and context/path discovery. Recording readiness follows the first successful sample.
- Native GIF timestamp probes (`frame-at` / `framelet_frame_at`) return one composited frame for midpoint/bisection review; GIF prefix decoding remains sequential.
- Native scene/time-uniform frame extraction, full GIF compositing, PNG contact sheets and bounded MCP image previews.
- CLI/MCP countdown, aspect-preserving downscale, fixed pre-encoding privacy masks and output byte budgets.
- Scoped MCP input/output roots, explicit capture targets, no-overwrite guards, atomic PNG/GIF publication and bounded GIF processing.
- Recovery from malformed/oversized JSON-RPC requests, EOF cancellation, idempotent terminal controls and bounded session history.
- Real Bilibili recording through the official MCP SDK: 29.88-second source, 12 midpoint probes, 7.63-second native GIF and independent pixel/timing verification.
- Rewritten English/Chinese README and About; media is separate from the compact portable ZIP. No extra runtime installation required.
- Desktop/session, protocol/path, compositor, timestamp and official SDK regression checks.

## v0.1.0 — 2026-10-30

First public release of Framelet for Windows x64.

- Native Win32 transparent capture frame with click-through region, resize/move and F8 recording control.
- Record, pause/resume, single-frame capture, cursor capture and Save As.
- GIF recording and PNG screenshots through the same Rust core used by the GUI.
- CLI with JSON results, JSONL progress, named stop sessions and GIF trim/speed editing.
- Local MCP stdio server with seven tools and a versioned tool catalog.
- Streaming GIF encoding with identical-frame delay merging and changed-region cropping.
- Portable executable with a static CRT, MIT license and bundled dependency notices.

Requires an interactive Windows 10 2004+ / Windows 11 x64 desktop. Audio, AVI/MP4, a visual timeline and drawing/text overlays are outside this release.
