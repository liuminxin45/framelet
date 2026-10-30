# Design and implementation

## Direction

Small native utility, with the recording area as the main interface. A real window-region hole gives click-through interaction. Segoe UI and standard Win32 controls preserve keyboard operation and accessible names. Neutral light chrome, a quiet boundary and red recording state. No web renderer, decorative dashboard or model setup wizard.

UI/UX Pro Max returned Minimalism & Swiss Style for the desktop recorder query. Its landing-page section suggestions were not used; the native utility uses a compact capture-frame composition. The desktop target does not use mobile touch sizes or phone layout assumptions.

## Core

`capture.rs`: physical-pixel, per-monitor DPI-aware virtual desktop and window discovery. GDI screen DC + reusable 32-bit top-down DIB. Optional cursor compositing accounts for icon hotspots. GDI resources have deterministic Drop cleanup. Regions are checked against the desktop and a 16 MP limit.

`encode.rs`: a pending GIF frame plus a previous RGB buffer. Identical samples accumulate delay. Changed samples produce a cropped local-palette frame with Keep disposal. GIF data streams to a sibling partial file; successful finalization flushes and synchronizes, then publishes with a hard link that cannot replace an existing output. Partial files are cleaned on handled failure. Forced process termination can leave a partial file. No automatic partial-file recovery is claimed.

`cli.rs`: clap front end, timed recording, cancellation, PNG, named OS stop events, JSON results. CLI and MCP use the same capture and encoder paths. Named events are scoped to the current Windows session; concurrent recordings cannot claim an existing session name.

`gui.rs`: Win32 window with a subtracted capture aperture, native controls, a capture/encode worker, channel commands and periodic UI updates. Pause removes inactive time. Auto-save makes Stop a concrete commit. Save as copies to a new path. Recording geometry is frozen. Screen content is visible through the hole, rather than repeatedly rendered into a preview.

`mcp.rs`: newline-delimited JSON-RPC; bounded 1 MiB requests, seven declared tools, structured success and MCP tool errors. No shell-execution or cloud-upload tool. Blocking recording bounds duration to 300 seconds. CLI is also directly usable by agents.

## Tradeoffs

GDI works well for short desktop/UI GIFs and keeps the implementation small. It is not a high-frame-rate game capture backend. GPU-protected media, secure desktops, locked sessions, HDR color management and hidden/occluded windows are outside this release's verified scope. Desktop Duplication / Windows Graphics Capture, a visual editor and MP4/audio can be added behind the same CLI contract.

GIF color quantization uses the open-source gif/color_quant crates. Delta cropping is an independent implementation; it is not evidence that GifCam's encoder uses the same algorithm.
