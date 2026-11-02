# Fast AI recording and review / 高效 AI 录制与分析

The current catalog is **3.0**, MCP protocol **2025-11-25**, with **23 tools**. Use the installed executable's catalog as the contract. The portable tool still needs no extra runtime. This document describes implemented services; the separate fidelity-preset proposal is not implied to be shipped.

当前版本提供 23 个工具。运行无需 Python、Node、FFmpeg 或模型运行时；这些仍只是开发测试工具。以实际安装 EXE 的目录为准，本文不代表历史 Release 已被替换，也不代表清晰度四预设已实施。

## Recording with fewer model turns / 减少录制过程的模型往返

1. Cache `framelet_context` and the catalog for the connection; use windows/monitors only when the region is unknown.
2. `framelet_start_recording` → `framelet_wait_recording(until:"ready")` → operate the target app using the client's UI tools.
3. `framelet_stop_recording` returns promptly. `framelet_wait_recording(until:"terminal")` awaits the saved result without blocking controls. A client can perform this wait without repeatedly asking the model to poll.
4. Analyze once, review the candidate interval, batch probe uncertain boundaries, then edit and verify.

连接时缓存 context/catalog；明确区域后 start，再 wait 等待 ready。操作结束 stop，wait 等待 terminal。等待工具最多 30 秒，超时只返回当前状态，不会自动停止或取消录制；客户端可继续等待。取消 wait 也不改变录制状态。CLI/MCP 定时单 GIF 的默认时长为 **60 秒**，上限 300 秒；更长任务使用分组轮转。暂停仍受墙钟期限限制。

```json
{"name":"framelet_wait_recording","arguments":{"session_id":"<start result>","until":"ready","timeout_ms":10000}}
```

Status includes `ready`, `next_poll_ms`, `wall_remaining_ms`, sample count and media elapsed time. Start/stop/cancel are controls; wait remains an active asynchronous request while other controls execute. Clients must correlate responses by JSON-RPC ID, not line order.

状态给出 ready、建议轮询间隔、剩余墙钟预算、采样数和媒体时间。MCP 响应允许乱序；不要按输出行顺序匹配请求。

## Event discovery / 事件发现

The full trace keeps every retained observation. Query output defaults to compact action events: mouse movement is omitted unless requested. `at` orders the nearby matches by distance to the requested time; the first 100 early movements cannot push out the central click. `event_id` selects an exact observation even if it is a move.

默认查询展示动作事件，原始鼠标轨迹并未丢弃。按需设置 `include_moves:true`；用 `device`、`event_kinds`、`time_range` 筛选。默认 limit=8、detail=summary；需要完整派生 UID、坐标和输入字段时传 detail=full。唯一身份由根部 session_id/asset_id 与事件 id/sample_id/gif_frame 组合，不必在每条摘要中重复会话字符串。

```json
{"name":"framelet_timeline","arguments":{"input":"demo.gif","at":12.3}}
{"name":"framelet_timeline","arguments":{"input":"demo.gif","time_range":[12,13],"include_moves":true,"limit":30}}
```

Return fields: `matched_total`, `returned_count`, `truncated`, `next_cursor`, `order`. Repeat the same filter and pass next_cursor to fetch the next page. Cursors are bound to the trace content and query; changed content/filter/limit is refused. Group-level `at`/UTC/event-ID lookup is supported; choose the returned part path for paginated/range queries.

游标绑定内容摘要与查询条件，数据或条件变化后明确拒绝。组级支持全局时间/UTC/事件 ID 定位；分页和范围查询使用选中的具体分段。

CLI examples:

```powershell
framelet timeline demo.gif --at 12.3
framelet timeline demo.gif --time-range 12 13 --include-moves --limit 30
framelet timeline demo.gif --event-kind 513 --detail full
```

## Batch probes / 批量探针

`framelet_probe_frames` / `framelet probe` accepts 1–64 timestamps. One forward traversal per selected segment preserves caller order and duplicate timestamps, returning exact half-open frame intervals. Only selected frames are converted to full RGB. Numeric mode creates no PNG/image block. Optional reference/compare_region/difference_threshold uses composited RGB and the same coordinate rules as frame_at.

多点一次遍历，保持请求顺序与重复时间点。返回 `decode_passes` 和 `work.decoded_frames/converted_frames/composited_pixels`。这些是实际算法工作计数，不是采集帧率。GIF disposal/透明色语义不变。

```json
{"name":"framelet_probe_frames","arguments":{"input":"demo.gif","times":[0,2,4.5,7.6]}}
{"name":"framelet_probe_frames","arguments":{"input":"demo.gif","times":[0,2,4.5],"output":"review","export":"sheet","include_image":true}}
```

Optional output is an atomically published directory. `export:sheet` is the default: manifest plus contact-sheet.png, without full-size per-frame PNGs. `export:frames` adds PNG references. Images are sent only with include_image:true; output is then required. Group probes are numeric; choose a part for image export. The existing group overview also batches selected times by segment, builds its sheet directly from decoded pixels and exposes decode-pass counts. Failed exports do not publish a misleading finished directory or replace an existing output.

默认输出只写联系表与 manifest；frames 模式另写全尺寸 PNG。组级探针按分段分别遍历，数字结果仍可全局定位；图像导出选择具体分段。frame_at 继续作为单点操作，经过内容校验的重复点和参考图可复用有界缓存。

```powershell
framelet probe demo.gif --times 0 2 4.5 --output review --export sheet
```

## Action intervals / 动作区间

`framelet_analyze` / `framelet analyze` combines 16-pixel spatial change with mapped input actions, producing padded merged playback intervals. It detects a recorded small transient even when a 32×18 nearest thumbnail would miss it. Static long frame holds do not automatically become long motion intervals. Repeated/reversed actions need no monotonic predicate. A visual change is not a semantic highlight, and a click time is not its UI response time.

空间变化与输入动作共同产生候选区间，最多返回 32 个区间和有界候选列表，并显式标记截断。默认不送图；指定 output 后可生成联系表。start/end 选择分析窗口。组级窗口最多覆盖 16 个已提交分段，累计合成预算为 2 gigapixels；图像导出选择具体分段。

```json
{"name":"framelet_analyze","arguments":{"input":"demo.gif"}}
{"name":"framelet_analyze","arguments":{"input":"demo.gif","output":"action-review","count":6,"padding":0.25,"include_image":true}}
```

Review the intervals and limited images, then call framelet_edit. Keep original event/sample IDs and interpret offsets using timeline.md. For a known single monotonic boundary, local numerical bisection remains useful; it is no longer necessary to make the model discover every action through repeated image calls.

v0.4.1 collects a bounded visual-candidate contact sheet during this same analysis pass: up to count distinct opening/closing/change/time-coverage frames, rather than uniform timestamps. preview_selection states the policy; preview_work reports zero additional decodes and at most 4,147,200 thumbnail pixel bytes. The existing comparison buffer supplies the closing frame. These thumbnails do not reduce full-resolution change detection. Use batch probes for exact timestamp selections. Windowed analysis converts only intersecting frames and one predecessor; editing converts only retained frames. GIF prefix decoding/compositing remains sequential. Group probes/overviews read one index snapshot and validate each unique selected segment once under request-long input leases; work.index_reads/segment_validations expose that work.

v0.4.1 的候选联系表与分析同遍生成，最多 count 张不同的头尾、视觉变化和时间覆盖代表图；缩略图像素内存上限约 3.96 MiB，画布等其他内存另计。需要精确时间点时用 batch probe。局部窗口省掉无用 RGB 转换，保留必要的 GIF 前缀合成和前驱比较；分组调用按请求校验唯一分段，保留完整内容校验。发布基准与原始数据见仓库 docs/benchmarks-v0.4.1.md。

先选候选，再少量复核边界，最后 edit。单调边界仍可做局部二分；不要要求 AI 逐帧阅读，也不要将动作候选误称语义理解。

## Isolation, cancellation and caches / 隔离、取消与缓存

- A persistent supervised analysis child handles decode/query/edit work; recording controls stay in the host. At most one active analysis plus two queued requests; busy requests fail explicitly. Each analysis has a 120-second host deadline, and decoder pixel/time limits remain active. Cancel using standard `notifications/cancelled` with requestId.
- Analysis pipe writes are asynchronous too: a suspended child cannot trap the supervisor in a large pipe write. Cancellation/death closes the child job, and a later request starts a fresh child. Published files stay published; inspect paths before retrying a write.
- EOF cancels pending analysis/waits. Single GIF cancellation and group stop/save follow long-recording.md; an already accepted stop is preserved. Keep stdin open until replies when making normal tool calls.
- LRU cache: 8 entries, 64 MiB estimated retained data, 30-second freshness. Every lookup hashes leased input bytes. Changed content invalidates cache even with identical length/mtime. Read leases end after lookup so another app can edit a file; cached data is an immutable snapshot. Large/expired entries are not blindly trusted.
- The analysis child is released after idle time. There is no resident daemon, database or additional runtime.

分析有独立进程与有界队列；stop/cancel/status 不排在重分析后面。取消后下一次请求可重新运行。缓存不是跳过校验：每次都校验受租约保护的内容摘要，且对内存、条目、时间有界。EOF 是关闭信号，不是批量命令的“等待所有请求完成”指令。

## Capture/encoding metrics / 采集编码指标

Recording results/receipts expose `capture_metrics`: source_samples, target_fps, samples_per_media_second, capture_us, transform_us, encode_pipeline_us, quantize_us, lzw_write_us, finalize_us, missed_full_periods, exact_palette_frames, quantized_frames, merged_samples and checkpoint_snapshots. Quantization/LZW timings are nested parts of encode_pipeline_us and must not be summed twice. Samples per media second is not physical display refresh or browser GIF playback FPS.

RGB/crop/index buffers are reused. Exact palettes up to 256 colors can reuse indexed mapping; complex color uses the current quantizer. This is not the separate configurable color/dither/preset proposal. Checkpoints clone an incremental SHA-256 state after flush/sync instead of rereading the growing GIF prefix; recovery still independently hashes/decodes the saved prefix.

阶段计时用于定位瓶颈，不能把目标 FPS 当实测采样率。颜色快捷路径只在精确映射成立时使用，不偷偷降低色数。检查点提速不删除持久化、摘要绑定或恢复验证。

## Verification / 验证

```powershell
python scripts/verify_performance.py target/release/framelet.exe --output work/performance-fresh
python scripts/run_checks.py target/release/framelet.exe --desktop --output work/all-fresh
```

Synthetic dense traces, independent Pillow pixels, small transient/reverse actions, content-changing caches/cursors, blocked IPC writes, live recording controls and EOF are tested. CI uses work bounds and generous regression thresholds; local benchmark results remain scenario-specific. Real 650-second CLI and MCP acceptance stays separate and SHA-bound. See performance-delivery.md for stage mapping and verification reports for measured results.
