---
name: framelet
license: MIT
description: Record and review Windows software tests, bug reproductions and product demos with Framelet; locate keyboard/mouse actions, batch-probe frames and trim GIFs. 用于 Windows 软件验收、问题复现、演示录屏、键鼠帧对齐与 GIF 抽帧剪辑。
---

# Framelet

Produce a reviewable recording or test artifact that matches the user's goal. Framelet captures the visible Windows desktop; use the host's browser/desktop tools to operate the app. A remote host needs an authorized executor on that Windows computer.

## Connect and choose evidence

Discover the available Framelet tools and call `framelet_context` once per connection. Use its installed documentation root, supported limits and FPS guidance; do not memorize an API catalog in this skill. If tools are missing, read `references/installation.json` when present for this machine's executable/client, otherwise locate the portable executable or `framelet` on PATH. Run `integrate check --client <client>` with that executable. Register it with `integrate install` when installation is within the user's requested work, then reconnect the host. Do not invent a cloud URL or start a network daemon.

For one stable final state, a PNG `framelet_shot` may be sufficient. For recordings, choose an explicit physical-pixel region and FPS according to the evidence needed. For human-facing demos, read the demo recipe and select 30 FPS explicitly; README placement does not imply a low-FPS recording. Read only the matching recipe:

- [Software tests and bug reproduction](references/testing.md)
- [Product demos and walkthroughs](references/demos.md)
- [GIF review and editing](references/editing.md)

## Recording lifecycle

Start a bounded recording, then `framelet_wait_recording(until:"ready")`. Operate the app only after readiness; inspect an early terminal result as a failure or already completed task. For more than one GIF's budget use the rotating-series tool and the current context limits.

Request stop when the intended interactions finish, then await terminal state. A wait timeout ends only the wait; it does not stop the recording. A successful tool envelope alone does not prove successful capture: inspect the terminal recording state, saved paths, warnings and integrity metadata. Cancellation discards an unfinished single GIF; stop saves it. Do not close the connection as a substitute for stop.

If an operation fails, inspect its structured failure and already saved artifacts. Recover only after the owner exits. Do not automatically repeat UI clicks or keys; they may already have taken effect.

## Review efficiently

Query action events before requesting many pictures. Analyze unknown or repeated motion once for candidate intervals; batch uncertain timestamps through `framelet_probe_frames`. Default intermediate probes to numeric output. Request a contact sheet or selected images when visual judgment is needed, and check opening/closing context before editing. Candidate changes are not semantic highlights. Human-facing demos require the demo quality/trim workflow plus opening/action/closing visual review before delivery; raw capture success alone is insufficient.

Preserve original event/sample identities when reporting tests. State requested versus measured sampling and any missing events. Default keyboard/mouse observation remains enabled; visual masks do not redact keyboard input. Keep raw traces local unless sharing them is requested, and review media before publication.

Return the saved artifact paths, what was exercised or shown, verification evidence and material limitations. Honor the user's requested format; do not claim a UI action or new recording when only existing media was analyzed.
