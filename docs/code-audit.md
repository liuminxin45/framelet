# 全代码审查与加固记录 / Local architecture and defense audit

范围：Framelet 当前工作目录完整产品源码、构建脚本、打包/许可脚本、CI、现有与新增验证脚本；不是只看 diff。既有未提交的时间线工作已保留，重构前源码另存于 work/audit-baseline-*。未推送、未发布。审查不覆盖 work 下的历史临时实验脚本、图片二进制的生成源或第三方依赖内部逐行审计。

## 结论与优先级

发现多处流程正确性和防御性问题，已实施以下修复。没有据此声称“所有漏洞已消除”或“任意故障都能恢复”。P1 为可能造成录制中断、错误成功/失败状态或不可信画面关联的高优先级问题；P2 为局部可靠性和维护性问题，不等同于已经证实可利用的安全漏洞。

| ID | 优先级 | 原问题与影响 | 本轮处理 |
| --- | --- | --- | --- |
| F01 | P1 | Release panic=abort，意外 Rust panic 会直接终止进程，不能在任务边界恢复 | 改为 unwind；CLI、MCP 工具、录制 worker 和 GUI 回调设置失败边界，失败任务终止，下一次独立请求可继续 |
| F02 | P1 | Win32 同步重入时可能再次访问 App 可变状态；FFI 回调不能让 unwind 穿过 Windows 边界 | GUI 重入保护、回调捕获；鼠标回调失败停止监听并在终结时报告 |
| F03 | P1 | GUI 使用不可报告线程创建失败的 spawn，且忽略接收端断开，可能一直显示录制中；成功保存的警告被隐藏 | fallible Builder::spawn、断开转终态、显示/记录错误与保存警告；状态 timer 失败停止初始化，F8 注册失败明确降级，录制结束恢复键盘焦点 |
| F04 | P1 | GDI 位图选择结果/像素指针未验证，CPU 读 DIB 前缺 GDI 同步 | 校验 SelectObject、空像素指针，在读像素前 GdiFlush；初始化失败释放已取得的资源 |
| F05 | P1 | 时间线只检查总帧数/时长，合法范围内的错误帧号、无真实时间跨度、坐标越界和重叠区间可能被接受 | 按实际 GIF 每帧播放区间校验样本/事件映射、成对空值、区域坐标、顺序/重叠、字符串与资源上限；查询不存在的时间返回空匹配，不再误返回概览事件 |
| F06 | P1 | 鼠标线程启动 recv/join 无超时，锁中毒处 unwrap 会 panic，样本上限警告可能被其他警告遮蔽 | 启动 2 秒、退出等待 500 ms；超时标记不完整并分离持有 Arc 的线程；队列读取失败降级；上限警告独立保留 |
| F07 | P1 | GIF 已发布后的临时文件删除失败仍返回整体失败，诱导调用方重试或误判文件丢失 | 将提交后清理错误转成警告并记录；时间线失败保留有效 GIF；禁止盲目重试已部分提交的操作 |
| F08 | P1 | GUI Save As 直接写最终路径，复制中断可能留下看似完整的文件 | 与 PNG/时间线共用 staged storage：临时文件写完同步后无覆盖发布；失败 Drop 清理，已有目标不变 |
| F09 | P2 | 错误大多只存在于临时返回值/stderr，无法事后排查；日志自身失败缺少反馈 | 统一 Failure 结构、限长信息、1 MiB 日志轮转/一份备份；日志写入失败在 log_error 返回，不制造第二次异常 |
| F10 | P2 | Clap 参数错误会直接退出，绕过统一失败记录；进度 stderr 写失败可能触发 panic | try_parse 后统一参数错误 JSON/日志；help/version 正常退出；进度输出 best effort |
| F11 | P2 | inspect 与完整解码对 GIF 零延时的处理不一致；inspect 未明确拒绝空动画 | 统一至少 10 ms 的规范化时间线，验证帧边界、空动画和解码预算；避免元数据与实际探帧错位 |
| F12 | P2 | mcp/timeline 文件职责混杂，核心模型依赖 CLI；失败处理/落盘逻辑重复 | 拆分协议/分发/目录、时间线五个职责模块；模型与应用服务归 recording；增加 runtime/storage 共享层 |
| F13 | P2 | 打包 metadata 原生命令失败未检查，压缩直接覆盖旧归档；许可脚本依赖系统默认编码 | 检查原生命令退出码与版本，先写独立 ZIP stage 再发布，明确失败阶段；许可元数据使用 UTF-8 |

内部常量 catalog 上仍有少量 unwrap：其值由固定 json! 字面量构造、并非用户输入，不把出现 unwrap 本身认定为漏洞。输入相关/资源相关 unwrap 已改为校验或 Result，重要任务边界另有 panic 隔离。

## 架构

见 [architecture.md](architecture.md)。CLI/MCP/GUI 是适配层；recording/sessions 是应用流程层；capture/encode/frames/timeline 承担平台与媒体职责；runtime/storage/safety 为共享基础。MCP 执行和后台会话不再依赖 CLI。GUI 的手动逐帧模式仍使用独立循环，不能简单删除这一差异。

## 依赖与原始依据

OSV 实时 querybatch 检查 Cargo.lock 的 **53 个 registry 包**，本次结果未返回已知漏洞记录。范围包括锁文件中其他目标平台的包；不是 cargo-audit 的替代认证，也不证明没有未知漏洞。机器可读证据：work/dependency-audit.json。[OSV API 主文档](https://github.com/google/osv.dev/blob/master/docs/api/post-v1-querybatch.md)。

- panic=abort 无法被 catch_unwind 恢复，FFI 边界必须单独处理：[Rust FFI/unwinding](https://doc.rust-lang.org/nomicon/ffi.html#ffi-and-unwinding)。
- GDI 与直接访问位图内存需要同步：[Microsoft GDI 说明](https://devblogs.microsoft.com/oldnewthing/20100923-00/?p=12773)。

## 验证设计

GUI 实测还发现重入保护会拦截系统关闭命令转发的嵌套 WM_CLOSE。已将 SC_CLOSE 转为异步 WM_CLOSE，避免阻断关闭。初次测试窗口已安全完成录制，随后仅终止本次创建的开发测试进程以解除构建文件占用；未终止其他程序。修复后已通过 F8 启停、停止后 Tab 焦点恢复、Save As 对话框、双文件落盘、Alt+F4 正常关闭；保存副本 62 次采样且无警告，见 work/gui-audit-proof.json。

- 保留原有录屏、编辑、抽帧、透明/差分合成、根目录/协议防护、暂停/取消/EOF 与官方 MCP SDK 验证。
- 新增 verify_defense.py：8 类恶意侧车文件、错误后 ping、旋转日志、日志目录不可写、CLI 参数错误、64 次 GIF 字节变异；断言错误不发布输出且无 panic 文本。
- 新增 panic 后下一任务可运行、staged write unwind 清理和目标冲突保护单元测试。
- 所有验证都要针对最终 release EXE 重跑；构建/检查日志与最终验收摘要保存在 work/architecture-audit-* 及交付输出中。

## 上轮剩余项（历史记录；当前处置见下表）

1. **P1 防护边界：** 路径 canonicalize/打开/发布仍有 TOCTOU 窗口。当前面向可信本地 MCP 客户端；如面向不可信并发进程，需要基于目录句柄和 reparse 属性的文件操作，不应宣称是沙箱。
2. **P1 可用性：** native API 卡死、OOM、栈溢出、访问冲突、进程被杀/断电不能由 Rust catch_unwind 保证恢复。更强隔离应把录制放独立进程，增加监督/心跳与退出原因记录。
3. **P2 完整性：** GIF/时间线不是原子文件对，结构匹配不防同尺寸/同字节数内容替换。可增加内容 hash、事务 manifest 和启动恢复扫描；需要衡量大文件读取成本。
4. **P2 可移植落盘：** hard-link 发布当前以 NTFS 为验证环境。exFAT/网络盘不能默认承诺相同发布语义，应明确检测和提供不覆盖的替代策略。
5. **P2 GUI 回归：** 原生 UI 仍有较多 Win32 调用，事件重入/窗口销毁/快捷键冲突需要长期 UI 自动化覆盖。不能把 CLI 成功当成所有 GUI 路径已验证。
6. **P2 性能与防恶意输入：** 当前是有界解码与小型变异语料，不是持续 coverage-guided fuzzing。应补充长期 GIF/PNG/JSON fuzz corpus、CPU/内存峰值与慢盘/断盘测试。
7. **P2 日志：** 日志会包含错误中的本地路径；本轮不写入录屏、按键或请求正文。多进程并发轮转是 best effort，必要时引入每进程日志/跨进程协调。
8. **P2 开发质量：** 后续可把 GUI 与 MCP 生命周期状态收敛成共享状态机；许可文件同 basename 的复制策略可改为保留子目录；CI Actions 可固定 commit SHA，并加入定期依赖审查。


## 上轮代码定位（重构后以模块/函数为准）

## 本轮闭环：键鼠帧同步与剩余工程项

| 上轮条目 | 当前实现与验收方式 |
| --- | --- |
| 路径 TOCTOU | 目录/输入句柄租约拒绝 reparse 与并发改写；单元测试验证操作期间目录不能交换、输入不能改写。仍不宣称是 OS 沙箱 |
| 原生录制故障隔离 | 所有录制入口统一使用子进程 + kill-on-close job + 心跳/停止超时；强杀与挂起 worker 后 MCP 报告失败且下一次录制成功 |
| GIF/时间线完整性 | SHA-256 绑定、pending 标记、完成清单与显式恢复；内容篡改被拒绝，中断后有效文件对可恢复；缺数据明确报告 |
| 硬链接依赖 | 改为持源文件句柄的无覆盖重命名；已在本机 NTFS 验证，不冒充实测过所有文件系统/网络驱动 |
| GUI 重复生命周期 | GUI/CLI/MCP 共用受监督录制流程，支持单帧、暂停、继续与停止；本轮另做原生 UI 回归 |
| 持续输入与性能验证 | 增加 GIF/PNG/JSON 变异回归、固定种子、资源监测、看门狗和静态遮挡录制基准；CI 每周计划运行。它不是 coverage-guided fuzzing，也不是不存在漏洞的证明 |
| 多进程日志 | 每进程文件、轮转与日志保留策略，消除不同进程共享同一轮转文件的问题 |
| 许可与 CI | 许可保留相对子目录；Actions 固定官方提交 SHA、最小只读权限；加入每周 OSV 依赖检查 |

CI 运行结果必须按提交 SHA 核验，不以本地测试冒充远端执行。断电、系统级崩溃与物理存储损坏仍不是软件可以消除的“bug”；本轮提供故障隔离、可观察失败和内容验证/恢复，不能恢复从未采集或尚未写出的事件。

新 schema 2 默认记录按键和鼠标事件，使用随机会话/资产 ID 与共享观测序号，返回事件—采样—画面的标识链及偏差。详见 [timeline.md](timeline.md)。

- runtime: `src/runtime.rs:83`
- storage: `src/storage.rs:50`
- capture: `src/capture.rs:179`
- gui: `src/gui.rs:568`
- store: `src/timeline/store.rs:118`
- recorder: `src/timeline/recorder.rs:329`
- protocol: `src/mcp/dispatch.rs:215`
- inspect: `src/encode.rs:395`

## 2026-11-02 当前接口与中断防护

删除 schema-1 时间线读取、缺失区间补齐、资产 ID 猜测、阻塞式 MCP 录制及 SDK 的多版接口容忍分支。现行 MCP 共 20 个工具。停止/取消立即响应，首个终止请求不可反转。单 GIF 恢复改用 PID + 创建时间，防 PID 复用或权限未知时误判。监督器先结束子进程再 join 管道读取线程。分段恢复在 GIF/时间线/清单/receipt 发布后中断可重复执行，对已有文件逐一比对，拒绝冲突且不覆盖。新增实际宿主强杀、挂起、断管道、取消竞态及各保存边界回归。强制要求见 [工程规范](engineering-standard.md)。
