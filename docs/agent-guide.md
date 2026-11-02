# Efficient GIF editing for agents · Agent 高效 GIF 剪辑指引

For recordings longer than one GIF, read [long-recording.md](long-recording.md): `framelet_start_series` returns immediately and rotates into a named group, each part at most 300 seconds. Query the group by global media time before selecting a part for scene ranking or editing. Normal EOF saves, forced termination requires recovery. / 长录制先读该指引：分段按组保存，先全局定位再局部抽帧剪辑，不要每 300 秒重新开始一个任务。

**Use playback time, a stable image region and the returned frame interval. Send images only when a visual decision is needed.**

**按播放时间二分，比较稳定区域，利用帧覆盖区间跳跃。只有需要视觉判断时才发送图片。**

This workflow is advertised in MCP initialization instructions and `framelet_context.editing_recipe`. It uses Framelet's native tools; no Python, FFmpeg or image-analysis runtime needs to be installed with the recorder.

MCP 初始化说明和 `framelet_context.editing_recipe` 会告知 Agent 这套流程。下面的差分由 Framelet 原生完成，运行录屏工具不需要额外安装 Python、FFmpeg 或图像分析运行时。

## Default interaction timeline / 默认交互时间线

Schema-2 recordings default to keyboard + mouse + frame synchronization. Call `framelet_timeline` with `event_id` to retrieve one event and its unique sample/frame identity and offset. Read [timeline.md](timeline.md) before interpreting links. Recordings run in a kill-on-close supervised worker; failed sessions include output/pending paths. Use `framelet_recover` after an interrupted worker has exited. Never repeat a click or keystroke just because metadata publication failed; inspect the GIF/manifest and failure first.

新版默认记录键鼠帧。先查事件 ID，再查看对应及相邻画面；不要因元数据错误就重复点击或输入。读 `warnings`、`manifest` 和偏差字段，确认画面是否真正覆盖了动作。记录不包含剪贴板或输入法最终文本；按键内容仍可能敏感。

Every new recording saves an adjacent `.timeline.json`. Query `framelet_timeline` with the GIF path, optionally `at` (GIF seconds) OR `utc_us` (original UTC microseconds). Read [timeline.md](timeline.md) for event codes, coordinates, pause and edit semantics. Use event media times with frame_at to inspect the picture before/after an action. Old recordings have no recoverable interaction history. [Development and validation](development.md) and [MCP setup](mcp.md) live outside README.

每次新录制默认保存相邻时间线。先查 `framelet_timeline`，按点击缩小范围，再取图复核；低帧率可记录点击，但不能补回漏采的画面。运行时先调用 context 获取允许的路径和文档位置。

## 0. Choose capture FPS / 先选择采集帧率

### Choose FPS by intent (CLI / MCP)

Supported range: **1–50 FPS**. These are starting points; choose the evidence you need to preserve:

| Intent | Suggested FPS | Why |
| --- | --- | --- |
| Stable UI acceptance, long-running progress | **1** | Stable states only; use `shot` for just the final result |
| Development workflow, forms and state changes | **3–5** | Before/after states, rather than brief glitches or animation |
| Compact README GIF, step-by-step tutorial | **10–15** | Favor size when smooth motion is secondary |
| Polished software demo, scrolling, dragging, transitions | **30** | Default for audience-facing demos; preserve motion continuity |
| Fast motion, brief visual defects | **40–50** | Check performance with a short trial; shorter transients may still be missed |

**Development testing does not automatically mean 1 FPS is enough.** At 1 FPS, sub-second dialogs, flicker or errors may be missed. Default to 30 FPS for polished demos; deliberately use 10–15 FPS when compact output matters more.

Start with a short trial at `max_width:960` or `1280`. Estimate actual sampling speed from `recording_status` samples and recorded elapsed time, not encoded GIF frame count: static frames may be merged. If capture is slow, reduce the region or output size first; explain the compromise if lowering demo FPS is necessary. Encoded width × height × FPS must be ≤80,000,000. Requested FPS is a ceiling; GIF centisecond delay resolution also means 30 FPS does not guarantee uniform 33.33 ms intervals. MCP exposes the same advice in `framelet_context.capture_guidance`.

### 按场景选择帧率（CLI / MCP）

支持 **1–50 FPS**。以下是录制起点，AI 应根据需要保留的证据选择：

| 场景 | 建议 FPS | 选择理由 |
| --- | --- | --- |
| 静态界面验收、长任务进度 | **1** | 只需稳定状态；只看最终结果可直接用 `shot` |
| 开发流程检查、表单与状态切换 | **3–5** | 看清操作前后；不用于判断短暂闪烁或动画 |
| 体积优先的 README GIF、分步教程 | **10–15** | 动作流畅性非重点时，控制文件体积 |
| 正式软件演示、滚动、拖拽、转场 | **30** | 默认面向观众的演示档位，优先保留动作连续性 |
| 快速动作、短暂视觉缺陷排查 | **40–50** | 先做短录验证性能，仍可能漏掉更短的瞬态 |

**“开发测试”不等于 1 FPS 足够**：1 FPS 可能漏掉不足一秒的弹窗、闪烁或错误状态。正式演示默认 30 FPS；若明确优先考虑体积，可主动选择 10–15 FPS。

AI 先用 `max_width:960` 或 `1280` 短录，再根据 `recording_status` 的采样数和录制时长估计实际采样速度。不要用编码后的 GIF 帧数判断采样帧率，静态帧可能被合并。性能不足时，优先缩小区域或输出尺寸；若必须降低演示帧率，应说明取舍。输出宽 × 高 × FPS 不能超过 80,000,000；指定 FPS 是采样上限，GIF 的百分之一秒延时精度也不保证 30 FPS 对应均匀的 33.33 ms 帧间隔。MCP 的 `framelet_context.capture_guidance` 提供同一套建议。

## 1. Establish the task / 先明确剪辑目标

- Call `framelet_context` for paths and `framelet_inspect` for duration. Use unique output names; existing files are protected.
- For an unfamiliar/multiple-action GIF, request only 4–6 `uniform` or `scene` overview frames first. Choose a useful action and a local time bracket. Scene scores are visual-change candidates, not semantic importance.
- For one known scroll, save opening/closing anchor PNGs with `framelet_frame_at`. Use their actual PNG paths as references, not resized image-block data.

- 用 context 确认路径，inspect 确认总时长；输出用新文件名，不覆盖已有文件。
- 对内容未知或多动作 GIF，先要 4–6 张 uniform / scene 概览图，选定有用动作及局部时间区间。scene 评分仅表示视觉变化，不代表语义重要性。
- 对已知单次滚动，先取开头和结尾两个 PNG 锚点。reference 传实际保存的 PNG 路径，不要传缩小后的图片块。

```json
{"name":"framelet_frame_at","arguments":{"input":"raw.gif","at":0,"output":"opening.png"}}
{"name":"framelet_frame_at","arguments":{"input":"raw.gif","at":29.87,"output":"closing.png"}}
```

The example times/regions below belong to the [real Bilibili recording](demo.md). Re-establish them for other input. For its 29.88-second duration, 29.87 is inside the final frame; the exact end timestamp is out of range.

以下时间和区域来自 [Bilibili 实录](demo.md)，换一段输入需要重新确认。原片长 29.88 秒，29.87 位于末帧内；恰好等于总时长的时间点越界。

## 2. Choose a stable region / 选择稳定区域

Use a navigation row or text/card layout that moves with the page. Keep animated banners, playing thumbnails, cursors and tooltips outside the region. The opening reference detects departure; the closing reference detects arrival. The predicate must be consistent across the bracket. If scrolling reverses, or another action changes the region, split the bracket and review a small overview again.

选择随页面移动的导航行、文字或卡片布局，避开轮播、播放中的缩略图、鼠标和提示浮层。开头锚点检测“离开初始位置”，结尾锚点检测“进入最终位置”。区间内的判定应保持单调；反向滚动或其他动作改变区域时，先拆分区间，再取少量概览图确认。

`compare_region` uses **decoded GIF image pixels**, unlike recording masks, which use source capture-region pixels. Anchor metadata includes the original `width`/`height` and `preview` dimensions. If a preview was resized, map coordinates to the original dimensions; references must match the GIF's full dimensions. Recording with `max_width:960` simplifies this mapping.

`compare_region` 使用**解码后 GIF 图像的像素坐标**；录制遮挡则使用源捕获区域坐标，二者不同。锚点结果包含原图 `width`/`height` 和 `preview` 尺寸；预览缩小后，需要把坐标映射回原尺寸。reference 必须与 GIF 完整尺寸一致。录制时设置 `max_width:960` 可以简化映射。

## 3. Bisect numerically / 用数值二分

```json
{
  "name":"framelet_frame_at",
  "arguments":{
    "input":"raw.gif",
    "at":14.94,
    "reference":"opening.png",
    "compare_region":{"x":40,"y":125,"width":885,"height":65},
    "difference_threshold":24,
    "include_image":false
  }
}
```

This sends **zero images** and, with `output` omitted, writes **zero PNGs**. The result includes `frame_index`, `timestamp_ms`, `duration_ms`, and:

这样发送 **0 张图片**；省略 `output` 后写入 **0 张 PNG**。结果包含帧号、起点、持续时间，以及：

```json
{"comparison":{"metric":"max_absolute_rgb_channel","difference_threshold":24,"changed_fraction":0.28,"mean_absolute_error":12.3}}
```

Those numbers illustrate the fields, not measured values. A pixel is changed when **any RGB channel differs by more than the threshold**. `changed_fraction` is the changed-pixel fraction in the selected region; `mean_absolute_error` averages absolute differences over all three channels. Comparison uses composited RGB, not GIF palette indexes. A noise threshold helps avoid counting small quantization differences.

上面的数值仅演示字段，不是测量结果。**任一 RGB 通道的绝对差超过阈值**，该像素才计为变化；`changed_fraction` 是区域内变化像素占比，`mean_absolute_error` 是三通道平均绝对差。比较的是合成后的 RGB，不是 GIF 调色板索引；阈值可过滤较小的量化差异。

For the Bilibili case, departure used `changed_fraction > 0.03`, arrival used `< 0.025`, with pixel threshold 24. These are tested example parameters, not universal defaults for motion. Inspect visual anchors and tune for the content.

Bilibili 示例中，离开初始位置用 `changed_fraction > 0.03`，进入最终位置用 `< 0.025`，像素阈值为 24。这是本例验证过的参数，不是适用于所有动作的判定阈值；应先看锚点，再按内容调整。

## 4. Jump over known frame intervals / 跳过已经知道的帧区间

Maintain `lo` with predicate false and `hi` with predicate true, in **integer playback milliseconds**. Cache each answer over `[timestamp_ms, timestamp_ms + duration_ms)`, keyed by input, reference, region and threshold. Reuse cached answers before another tool call.

维护判定为假的 `lo` 与为真的 `hi`，使用**整数播放毫秒**。按输入、reference、区域和阈值缓存每个回答，覆盖 `[timestamp_ms, timestamp_ms + duration_ms)`；再次调用前，先检查已有区间。

```text
while hi - lo > 250 ms:
    mid = floor((lo + hi) / 2)
    result = cached answer or frame_at(mid / 1000, include_image=false)
    if predicate(result):
        hi = min(hi, result.timestamp_ms)
    else:
        lo = max(lo, result.timestamp_ms + result.duration_ms - 1)
```

Verify that bounds stay ordered and the predicate remains consistent; otherwise stop bisection and inspect a local overview. The `-1` keeps `lo` inside a false frame's half-open interval. GIF timing is in centiseconds; sub-frame searches add no useful precision. A stationary image can occupy seconds in one frame, so **do not bisect frame numbers**.

检查边界仍有序、判定仍一致；否则停止二分，查看局部概览。`-1` 让 `lo` 留在判定为假的帧覆盖区间内。GIF 延时单位为厘秒，小于单帧时间的搜索没有额外信息。静止画面可能用一帧承载数秒，**不要按帧号二分**。

## 5. Review, trim, verify / 复核、裁剪、验证

Request image output only at uncertain boundaries. Visually check that the beginning and end of the action are included; use roughly 0.15–0.4 seconds of context as appropriate. Call `framelet_edit`, inspect its duration, then extract only 3–4 final frames to verify ordering and content. Report retained times, assumptions and uncertainty. Do not claim semantic highlight detection from a pixel difference.

只在边界不确定时请求图片，检查动作的开始和结束都有保留；按情况留约 0.15–0.4 秒上下文。调用 edit 后核对时长，最后取 3–4 张成片图检查顺序和内容。向用户报告保留区间、适用假设和定位精度，不把像素差分说成语义精彩片段识别。

## Verified on the same recording / 同一原片的复测

| Method / 方法 | Midpoint calls / 中点请求 | Boundary interval / 定位区间 | Search images / 搜索图片 |
| --- | --- | --- | --- |
| Plain bisection + independent PNG comparison / 普通二分 | 12 | <234 ms each / 首尾各 <234 ms | 15 point PNGs including anchors / 含锚点 15 张点位图 |
| Native region comparison + interval jumps / 原生区域比较＋区间跳跃 | 8 | 171 ms opening, 211 ms closing / 开头 171 ms、结尾 211 ms | 2 anchors; numerical midpoints / 2 张锚点，中间仅数值 |

The native replay made 12 point calls in total: 2 image anchors, 2 numeric bracket validations and 8 numeric midpoint probes. Midpoints wrote no PNGs and sent no image blocks. Every native numerical result matched independent Pillow pixel comparisons. The published 7.63-second GIF keeps the original conservative context around these boundaries. [Replay evidence](demos/bilibili-native-bisection.json)

原生复测总计 12 次点位调用：2 次图片锚点、2 次数值边界确认、8 次数值中点。中点不写 PNG、不发送图片块；所有原生差分数值均与独立 Pillow 像素核对一致。公开的 7.63 秒 GIF 仍保留原先较稳妥的首尾上下文。[复测依据](demos/bilibili-native-bisection.json)

GIF prefix decoding remains sequential and bounded; these savings concern tool calls, disk exports and model images, not guaranteed logarithmic decode cost or measured token percentages.

GIF 底层仍是受限的顺序前缀解码。这里节省的是工具请求、磁盘导出和模型图片，不保证解码成本为对数级，也没有宣称测量过 token 百分比。
