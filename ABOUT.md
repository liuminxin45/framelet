# About Framelet · 关于 Framelet

**CLI First · AI Native · Keyboard–Mouse–Frame synchronization**<br>
**CLI 优先 · 本地 MCP · 键鼠帧同步的 Windows 录制工具。**

Built for AI-driven software automation tests, bug reproduction, product showcases and walkthroughs. Record screen and input together, locate actions by unique identity or time, then extract frames and trim GIFs. / 面向基于 AI 的软件自动化测试、问题复现、产品展示与操作演示；同步录制键鼠与画面，按唯一标识或时间定位操作，再抽帧、复核与剪辑。

Framelet is a native Windows GIF/PNG screen recorder built with Rust and Win32. It ships as one small native EXE. Unzip and run; no Python, Node.js, .NET, FFmpeg, browser runtime, account or model API key is required. Windows system APIs and a static CRT provide the local runtime foundation. Open-source Rust libraries are compiled in; their license notices are included in the portable package.

Framelet 是使用 Rust + Win32 编写的原生 Windows GIF/PNG 录屏工具。使用一个小巧原生 EXE，解压即用，无需额外安装 Python、Node.js、.NET、FFmpeg 或浏览器运行时，也无需账号和模型 API Key。程序使用 Windows 系统 API 和静态 CRT；编译进程序的开源 Rust 库及其许可证声明随便携包提供。

CLI and the local MCP server are the core interfaces; a native capture frame is also available. Version 0.4.1 provides 23 MCP tools, default input/frame timelines, unique event/sample/frame links, supervised recording workers and hash-bound recovery manifests. Reliable long recordings rotate into a named group of GIFs, each at most 300 seconds, with durable checkpoints. Capture supports 1–60 FPS; high-rate GIF browser playback has [timing limits](docs/60fps.md). No model or cloud upload service is embedded.

CLI 与本地 MCP 是核心入口，同时保留原生捕获框。v0.4.1 提供 23 个 MCP 工具、默认键鼠帧时间线、唯一事件/采样/画面关联、受监督录制子进程与内容校验恢复清单。可靠长录制按组命名，每段最长 300 秒自动轮转，并保留持久检查点。支持 1–60 FPS 采集，高帧率 GIF 的浏览器播放存在[时序限制](docs/60fps.md)。不内嵌大模型或云端上传服务。

Input events retain original IDs and computer time through native editing; derived GIFs have new asset IDs. Sampling differences are explicit offsets. Keyboard recording is key-level input, not clipboard or IME text extraction. 按键、鼠标事件经剪辑后保留原始身份和电脑时间，派生 GIF 使用新资产标识，并明确返回采样偏差；键盘记录是按键级事件，不读取剪贴板或输入法提交文本。[Timeline / 时间线](docs/timeline.md)。

[Real Bilibili walkthrough / Bilibili 真实案例](docs/demo.md) · [MCP guide / MCP 配置](docs/mcp.md) · [Changelog / 变更记录](CHANGELOG.md)

Built by [Neo / 刘民心](https://liuminxin.cn) · [GitHub](https://github.com/liuminxin45) · MIT. Independently inspired by GifCam; no affiliation or redistribution of its executable, icons or decompiled code. Framelet targets Windows 10 2004+ / Windows 11 x64 with an interactive desktop. Current media formats are GIF/PNG; MP4 and audio are not implemented.

作者：[Neo / 刘民心](https://liuminxin.cn)，[GitHub](https://github.com/liuminxin45)。MIT 开源。受 GifCam 启发的独立实现，不附带原程序、图标或反编译代码。面向 Windows 10 2004+ / Windows 11 x64 交互式桌面；当前支持 GIF/PNG，尚无 MP4 和音频。

Current engineering policy: [engineering-standard.md](docs/engineering-standard.md). Interrupted recovery can be repeated; mismatched output is preserved and refused.

[Efficient AI workflow / 高效 AI 工作流](docs/performance.md) — nonblocking waits and controls, filtered/paginated input events, one-pass batch probes, transient-aware action intervals and bounded content-verified caches. / 无阻塞等待与控制、事件筛选分页、单遍批量探针、短暂状态候选和有界内容缓存。

Native client integration installs MCP plus workflow skills; binary MCPB and client-plugin bundles preserve the no-extra-runtime model. / 原生客户端接入提供 MCP 注册、Skill 安装与校验；二进制 MCPB/插件包仍无需额外运行时。[Installation / 接入](docs/installation.md)。

[Development demo workflow / 开发版演示工作流](docs/demo-workflow.md): 30 FPS defaults, acknowledged non-invasive anchors, RGB/block difference details, action-window sampling checks and separate trimmed candidates. Raw media stays intact; saved is not visually approved. / 默认 30 FPS、已确认的非侵入式锚点、RGB/分块差分、动作区间采样检查与独立剪辑候选；保留原片，保存不等于演示验收通过。
