# Changelog

## v0.4.0 — 2026-11-02

- Native user-scoped `integrate install/check/uninstall` for Codex, Claude Code, Cursor and Claude Desktop; managed skill discovery where supported, private backups, explicit adoption, executable-path upgrades and restartable transactions. Uninstall preserves captures and restores adopted entries.
- Official workflow Skill with focused testing/demo/editing recipes; five embedded MCP guide resources. Binary MCPB, portable, Skill and client plugin packages, registry metadata and a Claude Code repository marketplace.
- Isolated integration/failure regressions, real Codex/Claude Code host checks, bundle/schema validators and distribution CI.


- Breaking: remove blocking MCP recording; 23 current tools, catalog 3.0. Only timeline schema 2 is accepted; no implicit interval or identity conversion.
- Single GIF capture is uniformly limited to 300 seconds across CLI/MCP/GUI; longer recordings use groups. Control session IDs are random, never PID/counter-based.
- Stop/cancel are immediate, first intent wins; host/worker/pipe failures remain bounded. Recovery can resume after each publication boundary and uses process creation identity.
- Add process interruption regressions, two Windows CI images, extracted-package validation, weekly real 650-second CLI/MCP jobs and mandatory engineering rules.

- Buffer durable JSON I/O with explicit flush errors. Separate supervised analysis from recording controls, with bounded queues, deadlines and cancellation including blocked pipe writes.
- Add `framelet_wait_recording`, `framelet_probe_frames` and `framelet_analyze`; filter/page input events, preserve transient changes and batch timestamps in one decode pass. Structured output avoids duplicate JSON; new analysis images are opt-in.
- Cache validated immutable snapshots under content hashes and entry/byte/time bounds. Reuse capture/index buffers and exact palettes for at most 256 colors; report stage timings and maintain checkpoint SHA incrementally.
- Default single recording duration is 60 seconds across CLI/MCP. Long recordings still rotate at 300 seconds per part. Add dense-trace, cancellation, mutation, compositing and performance CI gates.

## v0.3.0-rc.1 — 2026-11-02

- Raise CLI/MCP/GUI sampling ceiling to 60 FPS, use deadline-aware waits, and report encoded short-delay frames separately from trace warnings. Chromium playback can stretch 10ms GIF frames; native inspection is recommended, and browser sharing defaults to 30 FPS.
- Add independent GIF timing tests and real CLI/MCP 60 FPS checks. Pixel/color/output-FPS presets remain a design proposal, not a released feature.

- Reliable rotating recordings: `record-series` / `framelet_start_series`, GIF parts of at most 300 seconds, named `.framelet` groups, global observation identity and relative-path indexes.
- Durable frame-prefix SHA-256 checkpoints, independently committed segment receipts, index backup/recovery, bounded asynchronous finalization and normal MCP EOF save; cancel preserves committed parts.
- Group discovery, global timestamp/frame queries and bounded contact-sheet overviews. Native scene ranking and editing remain per GIF. Catalog 1.7 exposes 21 tools.
- Real rotation/fault tests and opt-in CLI/MCP 650-second acceptance scripts; checkpoints do not promise zero loss under power or storage failure.
- Default keyboard/mouse/frame synchronization: shared observation IDs, key transitions, sample/frame links, signed offsets and exact event-ID queries.
- Supervised recording child processes and kill-on-close jobs shared by CLI, MCP and GUI; worker crash/hang recovery and a new MCP recovery tool.
- Directory/input leases, no-overwrite rename publication, native SHA-256 binding and capture completion/recovery manifests.
- Per-process diagnostic rotation, license subtree preservation, pinned CI Actions, scheduled mutation campaigns and dependency checks; workload-specific resource benchmark.
- Product-focused bilingual README/About for AI software automation testing, demonstrations and frame extraction/editing.

- Split MCP/timeline responsibilities, move recording models/services out of CLI, and add architecture/audit docs.
- Add unwind task boundaries, structured failures, rotating diagnostics and staged no-overwrite copying.
- Harden GUI worker/reentrancy/failure reporting, GDI synchronization, bounded mouse shutdown and sidecar frame/coordinate validation.
- Add failure-injection, malformed-metadata and small GIF mutation regression checks.

- Default GUI/CLI/MCP interaction sidecars: original UTC/monotonic capture intervals, mouse events, merged-sample mapping and bounded positional dwell queries.
- Native trim/speed remapping preserves original capture/event identity; GUI Save As copies metadata.
- Scoped, bounded MCP timeline queries, automatic document discovery and AGENTS.md; product-focused bilingual READMEs.
- Timeline regression checks cover rounded playback intervals, repeated edits, privacy masks and malformed/mismatched metadata.


## v0.2.0-rc.2 — 2026-11-02

- Add bilingual FPS recommendations to README and the agent guide: 1 FPS for stable states, 3–5 for development workflows, 10–15 for compact GIFs, 30 for polished demos and 40–50 for fast motion checks.
- Expose machine-readable presets and selection/performance caveats through `framelet_context.capture_guidance`. Agents can choose a rate based on required evidence rather than treating all development tests as static.
- Clarify sampling ceilings, missed transient states, GIF timing resolution and how to estimate sampling speed without confusing merged GIF frames with captured samples.
- Use explicit UTF-8 in Python protocol verifiers on Windows, and add an official MCP SDK regression check for the published guidance. Capture defaults and the supported 1–50 FPS range are unchanged.

帧率建议现已提供中英文文档和 MCP 可读预设；默认采集参数与 1–50 FPS 范围保持不变。此版本仍为预览版，v0.1.0 保持稳定版。

## v0.2.0-rc.1 — 2026-10-30

Acceptance preview; v0.1.0 remains the stable release.

- 17 MCP tools, including background start/status/pause/resume/stop/cancel and context/path discovery. Recording readiness follows the first successful sample.
- Native reference/region comparisons, optional zero-image/zero-PNG probes, frame-interval caching guidance and an automatically advertised agent editing recipe. Native Bilibili replay reduced midpoint calls from 12 to 8.
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
