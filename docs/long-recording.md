# Reliable rotating recording / 可靠长录制

Available in v0.3 previews; inspect the installed catalog before use. Earlier v0.2 downloads do not include it. No extra runtime or background service is installed.

v0.3 预览功能，较早的 v0.2 下载不包含。CLI 与 MCP 共用录制内核；普通单 GIF 和原生 GUI 保持原有入口。长录制使用独立、明确的 `record-series` / `framelet_start_series`，避免升级后旧命令意外持续录制。

## Record / 录制

```powershell
# Existing parent directory required. Capture real desktop pixels for 650 active seconds.
framelet record-series --output-dir D:\Captures --name demo --duration 650 --region 0,0,960,540 --fps 10

# Record until Ctrl+C or a named stop request, within wall-clock/disk budgets.
framelet record-series --output-dir D:\Captures --name demo --until-stop --session demo --region 0,0,960,540
framelet stop --session demo
```

MCP: call `framelet_context`, then use its output_root as output_dir. An explicit region or full_screen:true is required.

```json
{
  "name": "framelet_start_series",
  "arguments": {
    "output_dir": "D:\\Captures",
    "name": "demo",
    "request_id": "demo-001",
    "duration": 650,
    "region": {"x": 0, "y": 0, "width": 960, "height": 540},
    "fps": 10
  }
}
```

Start returns session_id, directory and index immediately. Wait for recording_status.state=recording before operating the UI. Query/pause/resume/stop/cancel use the existing session tools. Stop returns promptly; poll to terminal before claiming success. The program rotates automatically; the AI does not need to restart it at each boundary.

`duration` 为总媒体时长，`until_stop:true` 与之互斥；分段默认最多 300 秒，可用 segment_seconds 设置 1–300。650 秒正常得到 300/300/50 秒；恰好 600 秒只有两段。暂停不计媒体时长，墙钟上限仍计时。正常 EOF 停止并保存，直接杀死宿主/worker 则需要恢复；本阶段不提供脱离客户端持续运行。

`request_id` 在同一输出目录内持久发现：相同参数返回原任务，异参报冲突，不重复录制。最多扫描 4096 个根目录项，超出会明确失败，避免不完整扫描造成重复任务；不要把大量无关文件混在输出根目录。

另一个连接发现正在录制的相同 request_id 时，只返回 existing 和非终态状态，不会启动第二个 worker，也不接管原连接的控制权。正常完成后的 .work 目录应为空；边界处未写入内容的临时文件会自动清理。

## Outputs / 产物

```text
demo__20261102-153000__7c91a2bf.framelet/
  recording.json
  recording.previous.json
  demo__20261102-153000__7c91a2bf.part-000001.gif
  demo__20261102-153000__7c91a2bf.part-000001.timeline.json
  demo__20261102-153000__7c91a2bf.part-000001.capture.json
  demo__20261102-153000__7c91a2bf.part-000001.segment.json
  demo__20261102-153000__7c91a2bf.part-000002.gif
  ...
  .work/                 # unfinished media and durable checkpoint snapshots
```

每段可独立播放。名称、时间与随机短标识共同分组，part 编号不重置；整次录制共享完整 session ID 与观察 ID 分配器，各段有独立 asset/frame 身份。索引中的段路径为相对路径，可以移动整个目录。segment.json 是独立段提交凭据，用于重建索引；不要仅删除索引后认定录像已丢失。

普通单 GIF 仍是原来的 demo.gif，不采用分段命名。单独分享 GIF 可以播放，但完整追踪和恢复应保留整个 .framelet 目录。时间线包含输入事件，请按录制内容选择分享范围。

## Discover, review, recover / 发现、抽帧和恢复

```powershell
framelet recordings D:\Captures --offset 0 --limit 16
framelet inspect D:\Captures\demo__20261102-153000__7c91a2bf.framelet
framelet frame-at D:\Captures\demo__20261102-153000__7c91a2bf.framelet --at 420 --output D:\Captures\420.png
framelet timeline D:\Captures\demo__20261102-153000__7c91a2bf.framelet --at 420
framelet frames D:\Captures\demo__20261102-153000__7c91a2bf.framelet --output D:\Captures\review --count 6 --mode uniform
framelet recover D:\Captures\demo__20261102-153000__7c91a2bf.framelet
```

MCP 使用 recording_list 发现分组，inspect 支持 offset/limit 分页。frame_at 和 timeline 接受组目录或 recording.json，以全局媒体时间定位已提交段；返回 segment_start_ms，帧/样本内时间仍以所选 GIF 为准。事件 ID / UTC 查询通过段级观察范围缩小候选，再验证真实侧车。活动段尚未提交时不能假装可查询。

整组 extract_frames 提供有界的均匀时间概览、PNG 和 contact sheet；返回有效 mode=uniform 与 requested_mode，不宣称全组做了场景排名。需要 visual-change 候选时对选定段执行 scene 模式。组级概览总处理预算 120 秒；单段仍有独立解码预算。剪辑暂以选定段 GIF 为输入，不自动合并多段。

恢复前必须停止录制；工具核查 PID 加进程创建时间，防止 PID 复用误判。已提交段经过哈希校验；当前段从最后成功的约 5 秒检查点恢复，验证前缀摘要、完整解码和时间线映射。后续尾部可能缺失，结果保持 partial；不会自动重启采集或重新执行点击。源残片保留。重复恢复不产生重复段。

## Bounds and failure semantics / 边界与失败

- 每段最多 300 秒、默认媒体硬预算 256 MiB、时间线 64 MiB；接近字节/观察数/总像素处理预算时提前轮转。高分辨率、高运动场景可能显著早于 300 秒切段，确保后续能在既有读取防护内检查。
- 默认任务总磁盘预算 2048 MiB、墙钟上限 28800 秒；可配置 total_mib 与 max_wall_seconds，后者最多 86400 秒。另保留至少约 1 GiB 可用空间及写入/收尾预留。数值是工程上限，不是续航或硬实时保证。
- 总预算包含临时文件和检查点。先预留帧及元数据空间，达到预算停止保存；外部进程抢占磁盘或同步调用失败仍可能造成 incomplete tail，不会自动删除旧段。
- 最多 4096 段，索引最多 8 MiB。任务内只保留当前段的有界样本/事件和有界汇总，不把整次录制留在内存。
- 轮转使用一个连续观察器、可轮换 writer 和最多一个异步收尾任务。队列/收尾超时会受控失败，已提交段保留；不保证所有硬件下绝对无缝。
- 检查点刷新完整帧并同步文件，静止画面也会定期提交，换取可恢复性；5 秒是目标，不是坏盘、断电或磁盘卡死下的绝对损失上限。恢复采用有界完整快照，当前版本不是无限追加的事件数据库。
- 检查点只刷新已编码区间，不提前消费仍在等待正常采样点的源帧；低帧率下最后可恢复媒体位置可能落后检查点写入时间一个采样周期。正常轮转和停止仍补足准确的媒体边界。
- 终态区分 completed / partial / failed / cancelled，索引列出 reason 与 failure。媒体已存、元数据失败时会明确告知。cancel 保留已提交段，移除未提交尾部的检查点，避免取消后又把它恢复出来；整组删除需要用户显式操作。

## Development checks / 开发验证

```powershell
python scripts/verify_series.py target/release/framelet.exe --output work/series-short
python scripts/verify_series.py target/release/framelet.exe --long --output work/series-cli-650
python scripts/verify_series.py target/release/framelet.exe --mcp-long --output work/series-mcp-650
```

短测试覆盖轮转、身份、暂停、停止、幂等、组级探针/抽帧、worker 崩溃恢复、取消、预算停止及正常 EOF。长测试真实采集桌面 650 秒，使用 64×64 全遮挡区域、1 FPS，检验时序、轮转和产物可用性，不能作为复杂画面/高帧率性能结论。完整长测试证据保存在指定输出目录的 long-verification.json。
