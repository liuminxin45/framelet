# Human-facing demo workflow / 面向人类的演示工作流

Current release contract: v0.5.1, catalog 4.0 / 25 MCP tools, timeline 3. The public v0.4.1 artifacts and measurements remain historical and immutable. This change adds no runtime dependency. CLI/MCP/GUI default capture is 30 FPS; tests can explicitly choose 1–60 FPS. Old timeline schemas are rejected, never silently migrated; their GIF pixels can still be inspected with inspect/probe. Keep older portable versions for historical trace queries.

## Record and mark / 录制与标记

Prepare the page/window and interaction plan before starting. Await ready, operate the app with host tools, wait for the intended response, stop promptly and await terminal. Real input event IDs are natural action anchors. An input timestamp is not a response timestamp.

`framelet_mark_recording` accepts session_id, request_id (1–80 ASCII letters/digits/_/-), kind (`begin`, `end`, `point`) and optional label (at most 80 UTF-8 bytes, no control characters). It is an MCP session control because its owner is the live stdio connection; CLI recordings use real input anchors and the same post-save analyze/prepare-demo workflow. The tool never injects an OS click/key. Markers are Device::Marker observations with the same random session, shared observation counter, UTC and monotonic clock as samples/input. Timeline 3 also retains the original capture_fps.

The immediate receipt is queued, not observed. Native recorded acknowledgement proves an in-memory observation, not durable publication; only a saved validated timeline/manifest proves persistence. A failed/killed capture may lose in-memory observations. Inspect status.markers for recorded (native observation_id / utc_us / mono_us), rejected or unconfirmed. There are at most 64 requests per task and 8 outstanding. Same request_id + same arguments is idempotent during an active session; conflicts fail. A begin cannot nest, end requires begin, and new markers require ready/unpaused/nonfinalizing capture. Point labels are intent hints, not semantic recognition. After termination consult terminal receipts/timeline; do not insert late anchors. This API never accepts a guessed retrospective timestamp.

A marker is stamped when the native recorder processes it, not when the model formulates a request. Tool delays still exist; use actual input and visual transitions for precision. Stop processes already received requests before finalization; accepted but unacknowledged requests remain explicitly unconfirmed after worker failure. Series markers share original IDs/clocks across rotation and are stored in their observed part. A begin/end pair spanning parts is incomplete in a per-part analysis; review the group and provide explicit part ranges.

## Analyze and prepare / 分析与成片

```
framelet analyze raw.gif
framelet prepare-demo raw.gif --output demo.gif
framelet prepare-demo raw.gif --start 17.28 --end 19.57 --output reviewed-demo.gif
```

MCP equivalents are framelet_analyze and framelet_prepare_demo. Preparation requires one committed GIF, a new output path, and both start/end or neither. It uses one visual/input analysis, then the existing durable editor, then a numeric candidate check. It never operates the desktop or removes internal waits. No output is created when markers are incomplete or candidate intervals were truncated, unless a reviewed explicit range is supplied. No motion/action suggestion requires an explicit range (e.g. intentional static evidence).

quality contains:
- capture_integrity: unknown / trace_available / incomplete, based on available trace warnings and loss; not proof every transient was sampled;
- holds: opening/closing and up to eight longest composited-image holds, not semantic inactivity;
- sampling: original requested_fps, action-window monotonic measured_action_fps and longest_action_gap_us; original capture intervals survive trims/speed edits, GIF frame count is not FPS;
- markers, marked_ranges_ms, trim_basis and suggested_trim_ms;
- findings and visual_review_required. File save success never grants visual approval.

Existing 16×16 RGB block scoring catches localized changes; the same comparison pass now exposes changed-pixel count/fraction, mean absolute RGB error and a changed bounding box. RGB tolerance remains 24 and default local change score 0.04. Stable-reference region comparisons are available through batch probes when gradual motion makes adjacent-frame boundaries ambiguous. No perceptual hash is trusted as exact equality; no neural network, optical flow or FFmpeg runtime is added. Existing lossless duplicate merging / changed-rectangle encoding remains separate from perceptual review decisions.

Natural action/visual candidates retain approximately 0.5s opening / 1s closing context (including the existing analysis padding). Complete explicit markers retain 0.5s before / 1s after intent, bounded to the file. These are initial context suggestions, not semantic cuts. Beginning/ending holds over 2s and internal holds over 3s generate review hints. Targets below 30 FPS, action sampling below 80% of target, or a capture gap over three target periods generate presentation hints. Tests may intentionally use low FPS or long waits. A no-response click and a brief single-frame notification must not be discarded just because visual motion is small or short.

Batch-probe the opening, each main action and final result, then review playback with an available viewer. If only frames were inspected, report that limitation. Under-sampled motion needs re-recording; copying frames cannot repair it. Marker ranges are intent evidence and not automatic content approval. User publication authorization remains separate.

## Interruption contract / 中断处理

| Node | Failure / preserved evidence / next action |
| --- | --- |
| Start/validation | No capture starts on invalid arguments; active owner is unchanged. |
| Marker queue/control | Bounded queue, idempotent request IDs, no injected input. Stop/status stay responsive; native acknowledgement is required. |
| Native crash/hang/EOF | Supervised kill-on-close job reaps the native worker; single EOF cancels unless stop accepted; groups save/checkpoint. Terminal receipts expose unconfirmed markers. |
| Analysis | One bounded worker independent of capture controls. Cancel/timeout kills it; source is read-leased and never changed; the following request can restart the analysis worker. |
| Edit before publish | Errors/unwind clean owned staging. Abrupt process death can retain a partial/pending file as incomplete evidence; do not treat it as a valid GIF or delete user files. Known pending files are discoverable through context. |
| GIF published, metadata fails | Preserve GIF and pending transaction; return saved paths/warnings. Recover only after owner PID + creation time no longer match; absent trace remains explicitly incomplete. |
| Candidate postcheck fails | Return candidate_saved_review_failed with saved path and structured failure; never hide/delete a valid published candidate. |
| Response lost / same-path retry | Never overwrite. Inspect output, timeline/manifest and pending state; recover if complete evidence permits, otherwise use a distinct candidate path. No UI action is retried automatically. |

Power loss, filesystem damage, unsampled frames and lost input cannot be repaired by inference. Tests verify bounded cleanup/restart and preservation, not immunity to all hardware failures. Developer regression: `python scripts/verify_demo.py target/release/framelet.exe --output work/demo-fresh`; it is part of the shared two-image Windows CI suite and emits SHA-bound summary evidence without uploading raw capture/input traces.
