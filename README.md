# Framelet

**一个小窗口，把屏幕变成 GIF。也让 agent 直接调用录屏。**

受 [GifCam](https://www.bahraniapps.com/apps/gifcam/gifcam.php) 启发的独立实现：Rust、原生 Win32、CLI First、MCP stdio。无 Electron，无浏览器运行时，无账号，无遥测。MIT 开源。

## 快速开始

Windows 10 2004+ / Windows 11，x64，交互式桌面。

```powershell
# 双击或无参数运行，打开透明取景框
.\framelet.exe

# 录制物理像素区域 x,y,width,height，3 秒，16 fps
.\framelet.exe record --region 100,100,640,360 --duration 3 --fps 16 --cursor --output demo.gif

# 截一张 PNG
.\framelet.exe shot --region 100,100,640,360 --output shot.png

# 裁剪第 0.5 到第 2 秒，并以 2 倍速导出
.\framelet.exe edit demo.gif --start 0.5 --end 2 --speed 2 --output trimmed.gif
```

输出路径不能已经存在；父文件夹需提前创建。坐标包含负数时可用 `--region=-1920,0,1280,720`。不传 region 则捕获整个虚拟桌面，受 16 MP 上限约束。屏幕像素来自当前可见桌面；遮挡和受保护内容遵循 Windows 的捕获行为。

## 小窗口

拖动、缩放透明取景框，框内区域可以直接点击。按 **Record / F8** 开始，再按一次完成保存；**Pause / Resume** 排除暂停时段；**Frame** 可启动暂停的单帧会话并逐张追加。停止后 **Save as...** 复制导出，**Open folder** 查看自动保存文件。

自动保存位置：`%LOCALAPPDATA%\Framelet\Captures`。取景框录制时固定位置和尺寸；关闭正在录制的窗口会先保存，再次关闭才退出。可选 10 / 16 / 20 / 25 fps 和鼠标光标。F8 注册失败时仍可用按钮控制。窗口自身默认排除在录屏外；开发时设置 `FRAMELET_CAPTURE_UI=1` 可以对窗口做截图 QA。

## CLI First

```powershell
.\framelet.exe --help
.\framelet.exe doctor
.\framelet.exe monitors
.\framelet.exe windows
.\framelet.exe inspect demo.gif
.\framelet.exe schema

# JSON Lines 进度在 stderr，最终 JSON 在 stdout
.\framelet.exe record --duration 60 --session demo --events --output long.gif
# 在另一个终端或 agent 进程中提前结束并保存
.\framelet.exe stop --session demo
```

Ctrl+C 同样会完成 GIF。CLI fps 范围 1–50，duration 范围 (0,3600] 秒，quality 范围 1–30（1 最精细，30 最快）。GIF 延时单位为 10 ms，采样按实际经过时间记账。相同画面合并延时，变化画面只编码变化包围框，录制内存不会随时长增长。

成功输出：`{"ok":true,"result":{...}}`；操作失败在 stderr 返回 `{"ok":false,"error":"..."}` 并退出 1；命令行语法错误由 clap 报告并退出 2。标准帮助和版本信息是文本。

## AI Native

`framelet schema` 提供版本化工具声明。`framelet mcp` 是本地 JSON-RPC MCP stdio server，支持 initialize、ping、tools/list、tools/call。提供 7 个工具：doctor、monitors、windows、record、shot、inspect、edit。录制与 CLI 共用相同核心，不需要 LLM SDK、API Key 或网络。

客户端配置示例（把路径替换为你的实际位置）：

```json
{
  "mcpServers": {
    "framelet": {
      "command": "D:\\Projects\\framelet\\dist\\framelet.exe",
      "args": ["mcp"]
    }
  }
}
```

工具调用示例：

```json
{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"framelet_record","arguments":{"output":"D:/captures/demo.gif","region":{"x":100,"y":100,"width":640,"height":360},"duration":3,"fps":16,"cursor":true}}}
```

MCP 录制是阻塞调用，单次最多 300 秒；调用前让客户端的工具超时大于录制时长。进程启动不开始录屏，只有调用 record/shot 才捕获；不包含自动上传、自动理解画面或自然语言模型。

## 构建与验证

Rust stable + MSVC C++ Build Tools + Windows SDK。

```powershell
cargo build --release --locked
cargo fmt --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
python -m pip install pillow
python scripts/verify.py target/release/framelet.exe
# 需要交互式桌面；实际录屏、单帧、命名会话停止
python scripts/verify.py target/release/framelet.exe --desktop
powershell -ExecutionPolicy Bypass -File scripts/package.ps1
```

发布包使用静态 CRT；用户无需安装 Rust、.NET、FFmpeg、Python。开发和验证依赖不进入发布包。

## 范围

v0.1 实现区域 GIF、PNG、暂停/单帧、鼠标、变化区域编码、CLI 裁剪变速与 MCP。编辑器导出为不透明 GIF，透明像素落在黑色背景上。暂未实现 GifCam 的可视时间轴、逐帧删除/绘字、调色模式、AVI、音频及窗口后台捕获；这不是完整的逐项等价复刻。

逆向记录见 [docs/research.md](docs/research.md)，设计见 [docs/design.md](docs/design.md)。参考软件不随源码或发行包分发；分析得到的 API/行为用于制定接口，项目代码独立编写。项目与 GifCam 作者无隶属关系。
