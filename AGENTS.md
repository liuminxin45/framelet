# Framelet agent entry point

For review algorithms and work counters, read [docs/algorithm-rollout.md](docs/algorithm-rollout.md); current release measurements are in [docs/benchmarks-v0.4.1.md](docs/benchmarks-v0.4.1.md). Analyze previews are bounded visual candidates with opening/closing context; request exact timestamps through batch probes when needed.

For installation/distribution use [docs/installation.md](docs/installation.md). The official workflow skill is [skills/framelet/SKILL.md](skills/framelet/SKILL.md). Native integrate install/check/uninstall owns only its receipt-bound client entry and skill files; preserve private backups and captures.

For efficient current MCP calls, read [docs/performance.md](docs/performance.md): await recording readiness/terminal state without model polling, discover action events, analyze intervals once and batch numeric probes before sending images. MCP 2025-11-25, current catalog 3.0 / 23 tools; responses may arrive out of order.

Read [docs/engineering-standard.md](docs/engineering-standard.md) before modifying any code, tests, interface or documentation. It is the mandatory engineering contract: one current API/format, bounded failure handling, restartable recovery and SHA-bound verification.

For reliable long recordings, read docs/long-recording.md. Use record-series / framelet_start_series: a task owns a numbered .framelet group; each GIF is at most 300 seconds. Single GIF recording and rotating groups are current modes sharing one supervised capture core. Discover groups through framelet_recording_list, query global times, and recover interrupted groups only after the owner exits.

Read docs/agent-guide.md before operating Framelet. Read docs/timeline.md for default interaction timestamps and time/coordinate mapping. Build, package and verification instructions are in docs/development.md; MCP installation and path rules are in docs/mcp.md.

At runtime initialize MCP and call framelet_context first: it returns documentation locations, recording roots, FPS presets and the editing recipe. Query framelet_timeline before requesting many images. Use the current catalog as the API contract; unsupported names and versions fail explicitly.

Read docs/architecture.md for module boundaries and the failure contract. Read docs/code-audit.md for findings and remaining limitations. On failure inspect structured failure, warnings, output paths and diagnostics; do not assume a failed metadata step means the GIF was lost or repeat a user interaction automatically.

Schema 2 defaults to keyboard/mouse/frame observation. Prefer event_id queries, preserve original IDs across edits, and use asset/frame identity for a derived GIF. Call context to discover interrupted captures; recover only after the owner exits. Check manifest integrity and input-loss warnings before claiming a complete trace.
