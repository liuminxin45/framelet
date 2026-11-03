# Demo workflow acceptance / 演示工作流验收

Local development candidate **v0.5.0-dev**. This is source delivery and a local installation; published v0.4.1 assets are unchanged.

- EXE SHA-256: `af14d76664da2b823363d4ba065ae30b141eb88c3769288c089182cc4cd1d421` (3,966,976 bytes, approximately 3.8 MiB).
- Rustfmt, warning-free Clippy, 36 Rust unit tests, pinned actionlint and Skill validation pass.
- All **15 core groups** pass: independent media/time checks, live desktop capture, official MCP SDK, input synchronization, hostile metadata, worker/host/transport interruption, rotation, 60 FPS sampling, performance bounds, review algorithms, demo workflow, integration and 1,000 fixed-seed mutations.
- CLI and MCP each completed a real 650-second capture, independently validated as **300 + 300 + 50 seconds** with original identities and valid manifests. These are 64×64 fully masked GDI captures at 1 FPS, testing durability rather than motion throughput.
- Actual Codex app-server and Claude Code connect to 25 tools; installed Skill is discovered. Exact native/Skill bytes match across portable/MCPB/client-plugin bundles; official/pinned validators pass. A separate global Codex process outside the repository discovers the upgraded user installation without model inference.
- The private real 32.23-second / 8-frame recording was tested on a GIF-only copy. Preparation produces **2.29 seconds**, retains all 8 frames with **zero pixel differences**, and preserves the original GIF and schema-2 sidecar. This does not recreate missing motion or claim playback approval.

The demo regression additionally verifies native begin/end acknowledgements and no injected OS input; default 30 FPS with >=24 measured FPS in the short masked action window; original sampling rate after speed edits; cross-part marker identity; pause/terminal/ID/label/queue refusal; a frozen worker with a full control pipe; unconfirmed anchors and next-task success; hostile/unpaired markers; cancellation during editing; and a foreign sidecar raced after staging. The latter preserves the published GIF and foreign file, returns `candidate_saved_review_failed` with structured failure, refuses overwrite and permits the next analysis.

An initial run exposed an old test synchronization race: writer staging appeared before the recorder's second collision preflight. The publication-collision fixture now waits for the durable pending transaction. Its focused retest and the full final 15-group suite pass; no production check or assertion was weakened.

Native marker acknowledgement proves observation in worker memory, not durable disk persistence. Only the saved validated timeline/manifest proves persistence. Unpaired markers and missing traces remain explicit. Partial/pending files after abrupt death are preserved as incomplete evidence; no recovery invents missing input or frames. A candidate still requires opening/action/closing visual review, and a marker is not evidence that an application responded.

[Full machine-readable evidence](verification/demo-workflow-v0.5.0-dev.json) · [Workflow and interruption contract](demo-workflow.md).
