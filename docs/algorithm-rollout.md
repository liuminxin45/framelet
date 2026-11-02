# Review algorithm rollout / 复核算法分批优化

This development series keeps the current 23-tool API and all input/content validation. No runtime dependencies are added. Published v0.4.0 assets remain immutable; candidate binaries are identified by SHA-256.

Batch 1: group probes and overviews use one index snapshot and validate each unique selected part once. A request retains input leases until decoding/export completes; the group index may still advance independently. Timestamps use binary interval lookup, preserve caller ordering/duplicates, and reject uncommitted gaps. `work.index_reads` and `work.segment_validations` report actual request-level work.

第一批：分组批量抽帧和总览按请求取得一次索引快照，每个选中分段只校验一次。文件租约维持到本次处理结束，录制仍可发布新索引。保持时间点顺序、重复点、缺口拒绝和篡改校验；运行时不新增数据库或常驻服务。

Verification: `python scripts/verify_review_algorithms.py target/release/framelet.exe --output work/review-fresh`. Results must distinguish native timings from model inference, and decoder work from RGB conversion. Later batches and measured results are added only after acceptance.

Batch 1 acceptance: alternating 5 warm requests per EXE on the same 64×64 committed segment. 64 spread timestamps: 367.404 → 27.807 ms (92.43% reduction); 64 duplicate timestamps: 512.679 → 22.756 ms (95.56%). Before: released v0.4.0 SHA `65a11571debe89cd9c2058f99d2a17d104fe2fabd437c7f6d3fdc64aca775acf`; after: `2d1403eaa671a600f5895dff9fe4c6efbc234bce501ba3c22a5ad99eb1833e41`. These are local MCP durations, not AI task speedups. Synthetic cross-part/order/gap/tamper tests and 32 unit tests pass. The development core suite now includes review regressions on both Windows CI images.

Batch 2: analysis converts only overlapping window frames plus one preceding frame; trimming converts only retained frames. GIF prefix decoding/compositing is unchanged. Edit responses expose `work` separately from persisted recording statistics. Independent Pillow checks cover transparency, Background/Previous disposal, variable holds and non-frame-aligned trimming; window change scores match full analysis.

第二批：分析保留前驱比较，剪辑仅转换保留画面。19.5–20 秒窗口的分析 RGB 转换 600 → 16，剪辑 600 → 15；仍需解码 600 帧。与第一批二进制交替测量：局部分析 2,195.510 → 793.694 ms（减少 63.85%），尾部剪辑 904.195 → 672.331 ms（减少 25.64%）。全片分析 1,723.007 → 1,681.184 ms，2.43% 差异视为正常波动，不宣称全片提速。第二批 SHA：`c803b03684069963500d77b6e0ba122c15ddbb8ef77479728b7ca0a73adecd6e`。
