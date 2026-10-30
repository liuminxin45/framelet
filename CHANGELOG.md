# Changelog

## 0.2.0-dev — main

- Bilingual README and About emphasize the small, self-contained executable and AI access through MCP.
- Native GIF frame extraction: time-uniform or visual-change candidates, full compositing, PNGs, contact sheet, timestamp manifest and context ranges.
- Two new MCP tools for extraction and PNG viewing; screenshots include bounded image content.
- Shared GIF compositing for extraction and trimming, including Previous/Background disposal.
- Independent pixel tests and official MCP SDK interoperability checks; no extra runtime install required.
- Published v0.1.0 assets remain unchanged. Visual-change candidates are not semantic highlights.

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
