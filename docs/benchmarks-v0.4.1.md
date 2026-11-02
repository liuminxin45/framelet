# v0.4.1 release benchmarks / 发布基准

The README shows a small representative table. This page provides the scope, method and tradeoffs; [raw JSON](verification/benchmark-v0.4.1.json) contains every timed request, warmup, work counter and sampled working-set peak. These measurements compare released v0.4.0 directly with the final v0.4.1 binary. Historical [per-batch measurements](algorithm-rollout.md) use different intermediate binaries and five samples; their percentages must not be added or substituted for this release comparison.

README 仅展示三个代表场景；本页和原始 JSON 保留完整数据、环境、参数与内存取舍。这次是 v0.4.0 到正式 v0.4.1 的重新对测，不能把之前各批开发版的收益相加。

## Environment and method / 环境与方法

- CPU: Intel Core i5-9400 @ 2.90 GHz; RAM: 17,009,004,544 bytes; Windows build 10.0.26300.
- Before: v0.4.0, 3,685,888-byte EXE, SHA-256 `65a11571debe89cd9c2058f99d2a17d104fe2fabd437c7f6d3fdc64aca775acf`.
- After: v0.4.1, 3,724,800-byte EXE (about 3.55 MiB), SHA-256 `2338fe38279ee4463874f67f413a162e071e0c8d41cb156490d7431f0a577437`.
- Two real local MCP stdio clients with isolated, byte-identical corpora and export directories. The supervised analysis workers are started before timing; each case has one untimed-in-summary warmup and ten timed requests per binary. The before/after execution order alternates each round.
- Reported latency is the median request-to-response time, including tool work and requested PNG output. `include_image` stays false; transmitting image blocks, model inference, approvals, browser/desktop operations and complete AI task time are not measured.
- Fixtures and font/image generation use the pinned development dependencies. No Python, Node, benchmark utility or additional runtime library is required to run Framelet itself.
- Results are local observations, not a universal speed guarantee or statistical confidence interval. The JSON also records maxima, but ten samples are not presented as a reliable production tail-latency estimate.

## Corpus / 素材

- **Group:** synthetic 64×64 static GIF, 300 seconds, 60 encoded five-second holds, 300 mapped samples and no input events. This is generated test media, not a fresh desktop recording. Group queries use 64 spread timestamps in 0–299 s or 64 copies of 10 s. Both versions validate identical content; the optimization validates each unique part once per request.
- **UI:** deterministic 640×360, 20-second/600-frame moving-marker and code-text GIF. The dense variant has 20,000 synthetic input events and its current-schema timeline/manifest. Tail analysis uses 19.5–20 s on the dense variant; editing uses that window on the plain GIF. Full numeric/preview cases use the plain GIF.
- **Bilibili:** the existing public `docs/images/bilibili-demo.gif`, 960×505, 7.63 seconds, 30 encoded frames. This release re-analyzes that real demo; it does not claim a new Bilibili walkthrough was recorded. No adjacent input timeline is synthesized for it.
- Every source file's SHA-256 is in the raw result. No real keyboard input is used by the benchmark corpus.

## Results / 结果

| Workload | v0.4.0 median | v0.4.1 median | Time reduction |
| --- | ---: | ---: | ---: |
| Group, 64 spread timestamps | 632.722 ms | 52.492 ms | 91.70% |
| Group, 64 duplicate timestamps | 449.840 ms | 25.287 ms | 94.38% |
| Analyze last 0.5 s, dense timeline | 2,106.080 ms | 790.356 ms | 62.47% |
| Trim last 0.5 s | 978.989 ms | 673.468 ms | 31.21% |
| Analyze + preview, UI | 2,829.125 ms | 2,274.085 ms | 19.62% |
| Analyze + preview, Bilibili | 970.896 ms | 565.726 ms | 41.73% |
| Full numeric analysis, UI | 2,194.963 ms | 2,174.618 ms | 0.93% |

The 0.93% numeric-only difference is treated as noise, not a new full-analysis speed claim. Preview results intentionally change selection policy: v0.4.0 samples uniform times across an action span; v0.4.1 selects distinct visual-change/time-coverage candidates with opening/closing context. Both use the same full-resolution change detector. Use batch probes for exact timestamps.

纯数值全片分析约 1% 的差异不作提速宣传。预览由均匀取点改为头尾与视觉变化候选代表图，是明确的功能取舍；全分辨率变化检测未降精度，需要指定时刻时使用批量探针。

## Work and memory / 工作量与内存

- Group index reads and validations of the same selected part: 64 → 1. Request-long file leases preserve content checks; content changes, gaps and uncommitted ranges still fail.
- Tail RGB conversion: analysis 600 → 16 frames, editing 600 → 15 frames. Both still decode/composite the GIF prefix; this is not random seeking or a 97% end-to-end speed claim.
- Preview: UI image traversals 600+600 → 600, Bilibili 30+30 → 30. The collector retains at most 32 thumbnails; thumbnail pixel storage is capped at 4,147,200 bytes (3.96 MiB). Contact-sheet canvas, decoder/comparison buffers and allocator overhead are separate.
- The JSON records sampled analysis-worker RSS every 5 ms, not unique physical memory or total host-plus-worker memory. Bounded thumbnails may increase working set in some scenes; the performance result does not imply every operation uses less memory.
- Compared with v0.4.0, the EXE grows by 38,912 bytes (38 KiB, about 1.06%). The three algorithm changes add **zero runtime dependencies**. The existing 23 MCP tools and recording/timeline formats remain current.

## Reproduce / 复现

Run from a source checkout with the two native executables available:

```powershell
python -m pip install -r scripts/requirements-test.txt
python scripts/benchmark_review.py --before D:/Tools/framelet/v0.4.0/framelet.exe --after D:/Tools/framelet/v0.4.1/framelet.exe --output work/benchmark-fresh --runs 10
```

Use a fresh output directory. The script generates the synthetic corpus, copies the existing public demo, starts isolated MCP clients, and writes `benchmark.json`. For correctness regressions rather than timing:

```powershell
python scripts/verify_review_algorithms.py target/release/framelet.exe --output work/review-correctness-fresh
```

Correctness tests independently check disposal/transparency, clipped pixels, variable frame delays, window predecessor scores, chronological distinct preview tiles, transient recall, content tampering, active index snapshots and output collisions. Performance is not obtained by removing sync, digest checks, input identities or recovery logic.

Presentation references: the concise scenario tables and detailed benchmark links in [ripgrep](https://github.com/BurntSushi/ripgrep), and warmup/repetition/result-export guidance in [hyperfine](https://github.com/sharkdp/hyperfine). Framelet's measurements use its existing test dependencies; neither tool is a new runtime dependency.
