# Framelet agent entry point

For reliable long recordings, read docs/long-recording.md. Use record-series / framelet_start_series: a task owns a numbered .framelet group; each GIF is at most 300 seconds. Ordinary single-GIF APIs keep their previous bounds. Discover groups through framelet_recording_list, query global times, and recover interrupted groups only after the owner exits.

Read docs/agent-guide.md before operating Framelet. Read docs/timeline.md for default interaction timestamps and time/coordinate mapping. Build, package and verification instructions are in docs/development.md; MCP installation and path rules are in docs/mcp.md. The previous full usage reference is preserved in docs/usage-reference.md.

At runtime initialize MCP and call framelet_context first: it returns documentation locations, recording roots, FPS presets and the editing recipe. Query framelet_timeline before requesting many images. Published versions may lack development features; inspect server version/catalog before use.

Read docs/architecture.md for module boundaries and the failure contract. Read docs/code-audit.md for findings and remaining limitations. On failure inspect structured failure, warnings, output paths and diagnostics; do not assume a failed metadata step means the GIF was lost or repeat a user interaction automatically.

Schema 2 defaults to keyboard/mouse/frame observation. Prefer event_id queries, preserve original IDs across edits, and use asset/frame identity for a derived GIF. Call context to discover interrupted captures; recover only after the owner exits. Check manifest integrity and input-loss warnings before claiming a complete trace.
