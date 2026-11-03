# Interaction timeline / 交互时间线

## Schema 3: keyboard–mouse–frame synchronization / 键鼠帧同步

Only the current schema 3 is supported; missing identity/coverage fields and unsupported schemas are rejected, never silently migrated.

Recordings default to a shared keyboard/mouse/frame observer. Keyboard transitions use the Windows low-level input hook in the same message thread and bounded queue as mouse events. Input is collected only during an explicit recording, and keyboard scope is a foreground window intersecting the capture region. A fully masked region suppresses keyboard collection; a visual mask is not a general password filter.

Schema 3 uses a random `session_id` and `asset_id`, plus one shared observation counter: samples and events have disjoint `id` values. Query results expand these into `event_uid`, `sample_uid` and `frame_uid`. Several events may reference one sample/frame. Native edits preserve original event/sample identities and UTC, generate a new asset identity, and remap playback links.

Keyboard events have `device:keyboard`, `vk`, `scan`, `flags`, `key_name`, down/up message kind, and the same timing/link fields as mouse events. Message kinds: key down/up 256/257; system-key down/up 260/261. Key records are not clipboard contents or IME-committed text. Unicode-packet scan units are exposed separately when present.

`sample_id` is the corresponding original sample; `offset_us` is the signed difference between input observation time and its capture start. Association normally uses the sample whose visible hold covers the event. It is not a claim that a click's visual response was captured simultaneously. Events in pause gaps remain explicitly unmapped. Use adjacent frames to review results.

```json
{"name":"framelet_timeline","arguments":{"input":"demo.gif","event_id":12}}
```

The event-ID query returns the selected event, its sample/frame link and playback position. `at`, `utc_us` and `event_id` are mutually exclusive. Unknown IDs have no match.

New captures save `demo.gif`, `demo.timeline.json` and `demo.capture.json`. SHA-256 binds the GIF and timeline. A pending marker survives an interrupted worker; `framelet recover demo.gif` / `framelet_recover` verifies a complete capture or finalizes a valid interrupted pair after its owner exits. Missing input events are never reconstructed. The commit manifest makes completion observable; it does not turn two independent files into one atomic filesystem operation.

新版默认同步记录键盘、鼠标与采样；使用随机会话/资产标识和跨事件、采样共享的观测 ID。按事件 ID 查询可找到原始采样、GIF 帧及时间偏差。多次操作允许对应同一帧；暂停期间没有画面的事件不会被强行关联。剪辑保留原始身份和电脑时间，派生 GIF 使用新资产标识。新文件带内容校验与完成清单，中断后可验证恢复。

New local development feature, not yet published. 新增本地开发功能，尚未发布。

Every GUI, CLI and MCP recording enables frame timestamps and keyboard/mouse monitoring by default. No extra runtime or background service is required. Normal GIF viewing is unchanged. A successful capture returns `timeline`, `manifest` and `warnings`; inspect them before claiming a complete trace. Key-level events are collected; clipboard, IME-committed text and UI-element content are not read.

GUI、CLI、MCP 默认开启帧时间与鼠标交互记录，不需额外运行时或常驻服务。输出 `demo.gif` 和相邻 `demo.timeline.json`；返回值包含时间线路径与警告。先检查警告再判断记录是否完整。不会采集键盘文本或 UI 元素内容。

## Agent workflow / Agent 默认路径

Initialize MCP → `framelet_context` → read its documentation paths and FPS guidance → start recording → poll readiness → operate UI → stop → `framelet_timeline` → frame_at around selected event → edit → verify output timeline.

The MCP initialization instructions advertise this workflow. `framelet_context.documentation` returns a local document root when present, relative file names and a repository URL fallback. The portable package includes these docs and AGENTS.md. Source checkouts expose AGENTS.md to coding agents. [Development checks](development.md) · [MCP setup](mcp.md) · [Editing guide](agent-guide.md).

```json
{"name":"framelet_timeline","arguments":{"input":"demo.gif","at":2.5}}
{"name":"framelet_timeline","arguments":{"input":"demo.gif","utc_us":1793595000450000}}
```

Use either `at` (GIF seconds) or `utc_us` (Unix UTC microseconds); omit both for overview. Queries return at most 100 events and 100 dwell candidates, plus total counts. Use `matched_media_us / 1000000` with `framelet_frame_at` to inspect a picture; review surrounding pictures to judge an action's result. Out-of-range queries return no matched sample. Historical GIFs without sidecars return an explicit error, never invented timestamps.

CLI: `framelet timeline demo.gif --at 2.5`, or `framelet timeline demo.gif --utc-us <microseconds>`.

## Data model / 数据含义

- `session_id` + stream (`samples` or `events`) + `seq` identifies original observations. UTC is for external log alignment, not a globally unique key; clock adjustments can repeat or jump. Edited samples can have several mapped segments with the same original identity.
- `utc_us`: computer UTC, microseconds since Unix epoch. Capture timestamps use Windows GetSystemTimePreciseAsFileTime. `capture_us`/`mono_us`: monotonic elapsed microseconds from the recording timeline origin. Event UTC is observed in the hook callback; it is not the exact hardware click time.
- A sample records `capture_us` and `capture_end_us` around GDI capture, `coverage_start_us`/`hold_end_us` for the mapped visible hold, `media_start_us`/`media_end_us` (half-open GIF playback interval), and zero-based `gif_frame`. Several samples may map to one merged GIF frame. `cursor` is observed after capture and is optional.
- An event records a numeric Windows mouse-message `kind`: move 512, left down/up 513/514, right 516/517, middle 519/520, vertical wheel 522, X buttons 523/524, horizontal wheel 526. `injected` is the Windows flag, not proof of which agent generated input. Wheel delta is signed. Events outside a sampled active hold, including pause gaps, retain null media/frame mapping.
- Query results additionally provide readable `action`, `desktop_point` and `gif_point` fields for up to 100 events. Raw sidecars retain source coordinates.

Points use **source recording-region-relative physical pixels**, not desktop or resized preview coordinates. Desktop = region origin + point. GIF x = point.x × width / region.width, similarly y. This supports negative monitor origins and downscaling. Region and dimensions remain in the sidecar.

每个采样保留真实采集起止时间和播放覆盖区间；合帧后多个采样仍对应一个 GIF 帧。暂停时鼠标监听继续，但未被录进画面的事件没有播放位置。裁剪/变速保留原始事件 UTC、采集时间与身份，只更新覆盖区间和 GIF 播放映射。精度受 GDI 采集耗时、事件投递延迟、采样率及 GIF 的 10 ms 延时单位限制，不能承诺硬件级同步。

Native edit remaps against the **actual rounded per-frame output timings**, including long-frame splits. It does not assume output time is a perfect `(input-start)/speed`. GIFs edited by external programs do not automatically update metadata; dimensions, file length, duration and frame counts are checked for structural mismatch, not cryptographic tamper detection. Copy/rename the GIF and sidecar together. GUI Save As copies both.

## Dwell and privacy / 停留与隐私

Dwell candidates derive from available cursor samples and mouse events: within a 4-source-pixel radius for at least 500 ms of original monotonic time, split across detected pause gaps. They are approximate positional observations, not DOM hover or semantic target detection. Movement is coalesced to at most about 60 updates/sec; clicks are retained independently. Even at 1 FPS click timing can be retained, but unsampled visuals cannot be recovered.

Outside-region and masked button/wheel events are discarded; movement can retain a null point to break a dwell without exposing the location. Sample cursors inside masks/outside the region are null. Metadata stays local, with no upload built in. Moving masks and semantic redaction are not provided.

## Limits and failure behavior / 上限与异常

100,000 shared keyboard/mouse events, 200,000 sample mapping segments, 64 MiB sidecar; query output is bounded. Queue pressure/event limits increase `dropped_events`; inspect it. Hook installation failure and metadata failures are surfaced as warnings while preserving GIF capture. The hook uses its own message thread and queues data without file writes; Windows can remove a stalled hook without notification, so capture is best-effort, not audit-grade input logging.

Files publish without overwrite through handle-based rename after staging and sync. Directory leases pin ancestors against rename/delete; input leases reject reparse files and concurrent writes. Publication no longer requires hard-link support. GIF and JSON are not an atomic pair: the hash-bound capture manifest is the completion marker. Cancellation before publication cleans normal unfinished output; a killed process leaves a pending marker for explicit recovery. Size, mapping and content checks run before editing. These guards do not provide an OS sandbox; protected/elevated surfaces and power-loss durability still depend on Windows and storage.

Metadata collection remains bounded in memory; compare CPU and package size on the target workload before claiming negligible overhead.

Recovery discovery is included in framelet_context (bounded root scan, no automatic deletion). The final manifest is a content-completion marker; recording warnings and dropped-event counts still indicate trace quality.

## Intent markers / 意图锚点

Timeline 3 adds optional original capture_fps and typed marker observations (device=marker, kind=0, marker={request_id,kind,label}). They share the event/sample observation namespace, UTC and monotonic clock. They never inject OS input. Numeric event kinds apply to keyboard/mouse; use the marker payload for begin/end/point intent. Native recorded acknowledgements describe observation in worker memory; only the saved validated timeline/manifest proves persistence. A failed capture may lose acknowledged in-memory observations. Unmapped or unpaired markers are explicit review findings. Native editing preserves retained original marker IDs/timestamps and remaps media/sample/frame links. See [demo workflow](demo-workflow.md).
