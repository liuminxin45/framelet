# MCP · Local recording for AI / AI 本地录屏

Local development now exposes 21 tools, including `framelet_start_series`, `framelet_recording_list` and `framelet_recover`. `framelet_timeline` accepts `event_id` and returns keyboard/mouse events, unique observation/sample/frame links and timing offsets. Each explicit recording runs in a supervised child process and writes GIF + timeline + content-hash manifest. Keyboard observation is active only during recording, never on MCP initialization. See [timeline.md](timeline.md) and [reliable long recording](long-recording.md) for scope and semantics. Ordinary single-GIF calls retain their 300-second MCP bound; series return immediately and rotate at most every 300 active seconds, with a separate task lifetime. Normal EOF saves a series; a killed process requires recovery.

The portable EXE **is** the MCP server: your AI client launches `framelet.exe mcp` and exchanges JSON-RPC over stdin/stdout. It runs on the Windows computer whose visible desktop you want to capture. There is no daemon to start, model API key or server runtime to install. Initialization alone never starts capture.

便携 EXE **本身就是 MCP 服务**。AI 客户端按配置自动启动 `framelet.exe mcp`，通过标准输入输出通信。它必须运行在被录制的 Windows 电脑上，并有交互式桌面。无需单独启动常驻服务、安装服务运行时或填写模型 API Key；连接 MCP 不会自动录屏。

## Discover local guidance / 默认发现文档

Call `framelet_context` first. `documentation` returns relative document paths, a local document root and a repository fallback URL. The local development version records timestamps/mouse events by default and adds `framelet_timeline`; see [timeline.md](timeline.md). Older releases may lack this tool.

先调用 context 获取路径、文档与帧率建议。本地开发版默认记录时间线；先查点击，再抽取操作前后画面。文档随便携包提供，源码目录也有 AGENTS.md 入口。

## Install / 安装

Download and extract the portable ZIP from [Releases](https://github.com/liuminxin45/framelet/releases), verify its SHA256, then use its actual **absolute EXE path** in your client's MCP configuration. The configuration format depends on the client; this is the common JSON form:

从 [Releases](https://github.com/liuminxin45/framelet/releases) 下载并解压便携包，核对 SHA256，然后把 EXE 的真实**绝对路径**填入客户端配置。不同客户端配置入口不同，常见 JSON 格式如下：

```json
{
  "mcpServers": {
    "framelet": {
      "command": "D:\\Tools\\Framelet\\framelet.exe",
      "args": ["mcp"],
      "env": {
        "FRAMELET_MCP_ROOT": "D:\\Captures",
        "FRAMELET_MCP_READ_ROOTS": "[]"
      }
    }
  }
}
```

`FRAMELET_MCP_ROOT` is created on startup; extra read directories must already exist. Both environment settings are optional. The default root is `%LOCALAPPDATA%\Framelet\Captures`, and reads are limited to that root unless extra directories are configured. Restart the MCP connection after updating the executable/configuration.

`FRAMELET_MCP_ROOT` 启动时自动创建；额外读取目录必须已存在。两个环境变量均可省略，默认根目录为 `%LOCALAPPDATA%\Framelet\Captures`；不额外配置时，只能读取此目录内的文件。升级 EXE 或修改配置后重新连接 MCP。

MCP is a protocol, not a universal installer. Each AI host registers its local command; a remote/cloud AI needs an authorized local connector/executor to reach this Windows process. A local EXE path on another machine is insufficient.

MCP 是协议，不是统一安装器。每个 AI 客户端需要注册本机命令；云端 AI 还需要经过授权的本地连接或执行端，不能直接运行另一台电脑上的 EXE 路径。

## Record while operating / 边操作边录制（v0.2.0-rc.1）

1. Call `framelet_context` to discover the executable, output root and limits. Call `framelet_windows` / `framelet_monitors` to choose **physical-pixel** coordinates.
2. Call `framelet_start_recording`, then poll `framelet_recording_status` until it reports `recording` with at least one sample. Operate your browser or app with your AI client's UI tools. Framelet records; it does not automate the page.
3. Call `framelet_stop_recording`. If `terminal:false`, poll status until completion. Inspect the saved result, extract frames and review images before trimming a highlight.

1. 先调用 `framelet_context` 获取安装和输出位置，再用 windows / monitors 确认**物理像素**区域。
2. 调用 start，查询 status 直到 `recording` 且至少有一次采样，再用 AI 客户端的浏览器/桌面工具操作页面。Framelet 负责录像，页面操作由客户端完成。
3. 调用 stop 保存；若 `terminal:false`，继续查询至完成。检查结果后抽帧看图，再裁出需要的片段。

```json
{
  "name": "framelet_start_recording",
  "arguments": {
    "output": "website.gif",
    "region": {"x": 100, "y": 100, "width": 1280, "height": 720},
    "duration": 60,
    "fps": 12,
    "max_width": 960,
    "delay": 2,
    "max_mib": 64,
    "masks": [{"x": 1000, "y": 0, "width": 280, "height": 70}]
  }
}
```

The start result contains a `session_id`; use it for status, pause, resume, stop and cancel. Pause is a request: poll until `paused` before relying on it. Paused time is excluded from playback; the wall-clock deadline still applies. Only one recording is active per server. Disconnect/EOF cancels an unfinished background recording; stop saves it. A late cancel never deletes a completed file. At most 16 terminal results are retained.

start 返回 `session_id`，后续控制工具都传此 ID。暂停是异步请求，需要查询到 `paused` 后再认定已经暂停。暂停时段不进入 GIF，整段会话的最长时间仍持续计算。每个 MCP 服务只允许一个录制会话；断连/EOF 取消尚未完成的后台录像，stop 则保存；取消已完成会话不会删除成品。最多保留 16 条完成结果。

## Tools / 工具

The v0.2 preview has 17 tools; v0.1.0 has the original 7. Install the preview for the new workflow.

v0.2 预览版共 17 个工具；v0.1.0 保留原来的 7 个。以下新增能力需要安装预览版。

| Tools | Purpose / 用途 |
| --- | --- |
| `framelet_context`, `framelet_doctor` | Paths and capture readiness / 路径与录屏诊断 |
| `framelet_monitors`, `framelet_windows` | Choose visible capture bounds / 选择可见录制区域 |
| `framelet_start_recording`, `framelet_recording_status` | Background recording / 后台录制与状态 |
| `framelet_pause_recording`, `framelet_resume_recording` | Exclude waiting time / 排除等待时段 |
| `framelet_stop_recording`, `framelet_cancel_recording` | Save or discard unfinished output / 保存或取消未完成录像 |
| `framelet_record`, `framelet_shot` | Blocking GIF or PNG / 阻塞式 GIF 或 PNG |
| `framelet_inspect`, `framelet_edit` | Inspect, trim and speed / 检查、时间裁剪与变速 |
| `framelet_frame_at` | One timestamp image for midpoint/bisection review / 按时间点取一帧，辅助二分定位 |
| `framelet_extract_frames`, `framelet_read_image` | Contact sheet and individual images / 抽帧预览与单图 |

## Guardrails / 防护边界

- MCP capture requires an explicit `region` or `full_screen:true`. Outputs resolve under the configured root; existing files are never overwritten. Parent directories must exist. Inputs are limited to configured read roots. Traversal, UNC/device paths and alternate data streams are rejected. Canonical parent paths prevent ordinary junction escapes. These are mistake guards for trusted local clients, not an OS security sandbox against another local process racing directory changes.
- Privacy masks are fixed solid rectangles **relative to the source region**, applied before scaling/encoding. They do not detect secrets, track moving elements or stop unrelated windows from covering the region. Review the recording before sharing.
- Ordinary single-GIF background sessions have a maximum 300-second wall-clock duration (plus an optional 0–30-second countdown). The tool waits up to 10 seconds on stop; the supervisor separately enforces worker heartbeat/stop deadlines and terminates a stalled child. A finalized file is published without replacement; ordinary cancellation discards unfinished single-GIF output. Series use separate task budgets, save on normal EOF and preserve committed parts on cancel; see [long-recording.md](long-recording.md).
- Capture: 16 MP, 8192 pixels per side, 1–50 fps, 80 MP/s encoded-pixel budget; output defaults to 256 MiB, configurable 1–1024 MiB. GIF processing: 512 MiB input, 100000 frames, 2 billion composited canvas pixels, 60 seconds per decode pass. Oversized/invalid JSON-RPC requests return an error while preserving the next request.
- No network upload, telemetry, shell execution or audio capture is built in. GIF/PNG output currently; MP4, semantic highlight detection, a visual timeline and background-window capture are not implemented.

- MCP 必须明确区域或传 `full_screen:true`；输出限定在配置根目录内，已有文件不可覆盖，父目录需已存在；读取也受目录限制。拒绝路径穿越、UNC/设备路径、备用数据流，并核对父目录的真实路径。这是防误操作机制，不能代替操作系统沙箱防范其他本地进程竞态替换目录。
- 遮挡是相对源区域左上角的固定纯色矩形，先遮挡再缩放编码；不自动识别秘密、不跟随元素移动，也不能阻止其他窗口遮住录制区域。分享前仍需检查录像。
- 普通单 GIF 后台会话最长 300 秒，另可设置 0–30 秒倒计时，暂停仍受墙钟上限约束。停止工具最多等待 10 秒；监督层另有心跳和停止期限，失效子进程会被结束。分段任务采用独立总预算，正常断连停止保存，取消保留已提交段；详见 [长录制指引](long-recording.md)。
- 像素、帧率、文件大小、GIF 解码工作量均有上限；格式错误或过大的 JSON-RPC 请求不会破坏后续请求的处理。
- 不内置上传、遥测、shell 执行或音频采集。当前输出 GIF/PNG；尚无 MP4、语义选片、可视时间轴或后台窗口录制。

## Timestamp probes / 时间点探针

`framelet_frame_at` takes GIF `input` and `at` in seconds (0..3600). With the default `include_image:true`, a new PNG `output` is required. With `include_image:false`, `output` may be omitted: no image block is sent and no PNG is written. Optional `reference` is a full-size PNG; `compare_region` selects image-relative pixels, and `difference_threshold` is an integer 0..255 (default 24). The result reports changed-pixel fraction and mean absolute RGB error. A pixel changes when any channel's absolute difference exceeds the threshold. References must match source dimensions; the read-root policy applies.

The returned frame index and half-open playback interval support interval caching. Exactly at GIF duration is out of range. Invalid timestamps, references or regions publish no output. Image responses include original/preview dimensions for coordinate mapping. See the [efficient agent recipe](agent-guide.md) and [real Bilibili example](demo.md).

`framelet_frame_at` 接收 GIF 输入与秒数 `at`（0..3600）。默认 `include_image:true` 时必须提供新的 PNG `output`；传 `include_image:false` 后可以省略 `output`，不发送图片、不写 PNG。可选 `reference` 为原尺寸 PNG，`compare_region` 指定图像内的相对像素区域，`difference_threshold` 为 0..255 的整数（默认 24）。返回变化像素占比与 RGB 平均绝对差；任一通道绝对差超过阈值才计作变化像素。参考图尺寸必须匹配输入，且受读取根目录限制。

帧号与左闭右开的播放区间可用于缓存和跳跃，恰好等于 GIF 总时长的时间点越界。无效时间点、参考图或区域不会发布输出。图片响应还包含原图与预览尺寸，用于坐标映射。见 [Agent 高效剪辑指引](agent-guide.md) 和 [Bilibili 真实示例](demo.md)。
