# P0–P4 performance delivery

Implemented P0–P4 scope for the 2026-11-02 assessment. [Current AI workflow and limits](performance.md). Each stage keeps current-format validation, default input observation, masks, no-overwrite publication and interruption recovery.

| Stage | Delivery | Gate |
| --- | --- | --- |
| P0 | Buffered JSON read/write throughout durable storage | Large trace I/O work bounds, explicit flush failure, existing recovery tests |
| P1 | Supervised analysis worker and responsive MCP control; event filtering/pagination | Pipelined controls, cancellation/worker death/EOF; dense mouse stream click recall |
| P2 | Single-pass batch probes and group overviews, selective RGB conversion and bounded content-verified caches | Independent compositing, repeat lookup, cache invalidation and byte budgets |
| P3 | Event/visual motion analysis, transient-aware selection, compact AI output and workflow | Small transient, repeated/reversed actions, output schemas, SDK and bounded image output |
| P4 | Capture/encoding stage metrics, buffer/exact-palette reuse and streaming checkpoint hashes; automated performance acceptance | Masks/identity/timing, latency/byte/decode work evidence, full regressions and real long captures |

No new runtime dependency or historical API/format branch. GPU/DXGI, embedded models/OCR, database and FPS above 60 are not required for these measured improvements. Proposed sparse pixel snapshots or compressed-block editing must not replace correct compositing; batch decoding and immutable snapshot validation deliver the first bounded solution.

The shared core gate now has 12 check groups, including the official SDK and the new performance regression. Rust has 27 unit tests. Long acceptance uses the actual binary for both real-time 650-second modes, with EXE hashes in each generated summary; only a completed matching-SHA report is evidence of a pass. The same suite runs on Windows 2022 and 2025 in GitHub Actions.
