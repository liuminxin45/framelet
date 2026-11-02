# Long recording acceptance / 长录制验收 · 2026-11-02

Version: local v0.3.0-dev. Tested EXE SHA-256: `6195740543d35aa305bdadc6d22e114b901bbefb5bc7494eb36f7d7aad6717a5`.

| Real recording | Result | Artifacts |
| --- | --- | --- |
| CLI 600 active seconds | 300 + 300 seconds | Two independently readable GIFs, no empty third part, empty .work after completion |
| MCP 650 active seconds | 300 + 300 + 50 seconds | Immediate start/status; committed-part review while recording; global 420-second probe/timeline and contact-sheet extraction passed |

两组验收均为真实 Windows GDI 采集，64×64 全遮挡区域、1 FPS。它们验证时长、轮转、保存、身份和查询链路，不能代表复杂动态画面或高帧率性能。

16 个单元测试与 14 组最终检查通过：格式、Clippy、原有录制/剪辑/抽帧、时间线、防御与异常回归、worker 故障、轮转专项、官方 MCP SDK 互通、1000 次确定性变异、场景基准及依赖查询。依赖查询未返回已知问题记录，不是无漏洞证明。

轮转专项覆盖同连接及跨连接幂等、暂停、停止、取消保留前段、正常 EOF 保存、容量触发停止、worker 被杀后的检查点恢复、重复恢复、路径越界和前缀摘要损坏拒绝。按键身份跨段映射另有确定性队列模拟测试，不把它描述成真实全局键盘输入验收。

验收发现并修复：总时长边界因厘秒取整可能预创建空临时段；现已阻止多余分配并清理未写入内容的 stage，正常终态检查 .work 为空。另一连接命中正在运行的 request_id 时，不再报告为终态，也不会启动第二个 worker。另修复低帧率下检查点提前消费待编码源帧造成中间段略短的问题，新增 1 FPS、13 秒、6/6/1 秒轮转回归并重新执行长实录。磁盘用量统计改用目录项元数据，消除异步改名/删除临时文件的竞争，并有并发回归测试。

Memory figures in [machine-readable evidence](long-recording-2026-11-02.json) sum host/worker RSS and include shared pages. Checkpoints target approximately five seconds; disk failure, OS failure or power loss can exceed that recovery window. Partial results explicitly identify incomplete data. [Usage and boundaries](../long-recording.md).
