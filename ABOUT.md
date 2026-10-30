# About Framelet · 关于 Framelet

**Tiny. Self-contained. Callable by AI.**<br>
**小体积，无需额外运行时，AI 可通过 MCP 调用。**

Framelet is a native Windows GIF/PNG screen recorder built with Rust and Win32. The v0.2 preview EXE is about **1.12 MiB**. Unzip and run; no Python, Node.js, .NET, FFmpeg, browser runtime, account or model API key is required. Windows system APIs and a static CRT provide the local runtime foundation. Open-source Rust libraries are compiled in; their license notices are included in the portable package.

Framelet 是使用 Rust + Win32 编写的原生 Windows GIF/PNG 录屏工具。v0.2 预览版单个 EXE 约 **1.12 MiB**，解压即用，无需额外安装 Python、Node.js、.NET、FFmpeg 或浏览器运行时，也无需账号和模型 API Key。程序使用 Windows 系统 API 和静态 CRT；编译进程序的开源 Rust 库及其许可证声明随便携包提供。

Three entrances share the same local capture/encoding foundation: a small native window, a JSON-oriented CLI, and a stdio MCP server with 17 tools. An AI host launches the EXE, discovers its paths with `framelet_context`, then records, probes timestamps, reviews images and trims clips. No model or cloud upload service is embedded.

同一套本地捕获与编码基础提供三种入口：原生小窗口、面向 JSON 的 CLI、包含 17 个工具的 MCP stdio 服务。AI 客户端启动 EXE 后，用 `framelet_context` 发现路径，即可录制、按时间点取帧、看图和裁片。程序不内嵌大模型或云端上传服务。

[Real Bilibili walkthrough / Bilibili 真实案例](docs/demo.md) · [MCP guide / MCP 配置](docs/mcp.md) · [Changelog / 变更记录](CHANGELOG.md)

Built by [Neo / 刘民心](https://liuminxin.cn) · [GitHub](https://github.com/liuminxin45) · MIT. Independently inspired by GifCam; no affiliation or redistribution of its executable, icons or decompiled code. Framelet targets Windows 10 2004+ / Windows 11 x64 with an interactive desktop. Current media formats are GIF/PNG; MP4 and audio are not implemented.

作者：[Neo / 刘民心](https://liuminxin.cn)，[GitHub](https://github.com/liuminxin45)。MIT 开源。受 GifCam 启发的独立实现，不附带原程序、图标或反编译代码。面向 Windows 10 2004+ / Windows 11 x64 交互式桌面；当前支持 GIF/PNG，尚无 MP4 和音频。
