# Engineering standard / 工程强制规范

This document is mandatory for every contributor and coding agent. Read it before changing code. A task is incomplete until the affected code, tests, interfaces and documentation agree.

本文是所有开发者与 AI Agent 的强制规范。修改代码前必须阅读；代码、测试、接口与文档未同步时，不得宣布完成。

## One current contract / 只维护现行合约

- Delete obsolete implementations, aliases, format readers, silent migrations and tests that tolerate multiple historical interfaces. Do not introduce compatibility flags or dual execution paths. Reject unsupported versions and unknown parameters explicitly. Historical Git commits and published assets remain historical evidence; they are not runtime branches.
- 删除过时实现、别名、旧格式读取器、静默迁移以及容忍多版接口的测试。禁止兼容开关、同功能双轨实现和自动猜测旧参数。格式版本不支持、参数未知时明确失败。
- Single GIF and rotating groups are distinct current products; both share recording supervision, timestamps, encoding and storage services. Do not confuse a current product mode with obsolete code. The server implements MCP protocol 2025-11-25; initialization returns its supported version per protocol negotiation, without separate historical implementations. MCP recording is always start → status → stop/cancel → terminal status. No blocking recording tool.
- CLI/MCP/GUI 的默认值、范围与实际行为必须一致；有意差异须写明原因并测试。产品默认值不是历史兼容。未来清晰度接口一次切换，禁止将 fps/max_width/quality 自动翻译成另一套新参数。
- Each persisted format has one supported current version of its own (timeline, index, checkpoint and manifest have separate version namespaces). Fields that prove identity, time ranges and integrity must be validated; never invent a missing ID or silently fill an observation interval.

## Failure and control / 失败与可控性

- Enumerate failure points for every operation: validation, resource acquisition, capture, encoding, metadata, publication, indexing, cleanup and response delivery. Define saved paths, state and recovery behavior for each point before implementation.
- 每条流程必须定义参数校验、资源获取、采集、编码、元数据、发布、索引、清理、响应发送失败后的状态和产物归属。预期失败使用 Result；任务边界捕获 Rust panic，失败操作不得原地继续。FFI 回调禁止 unwind 跨过 Windows 边界。
- Native capture runs in a child owned by a kill-on-close Windows job. Assign the job before sending the request. Heartbeat, wall-clock and shutdown waits have explicit bounds. Never join a pipe reader before terminating/reaping its producer. Never leave control blocked behind an entire recording.
- Start/stop/cancel return promptly and expose status; the first stop/cancel intent wins. Once finalization starts, pause/resume/another terminal action cannot reverse it. Terminal controls are idempotent. A failed task must release the active slot so another task can run.
- EOF or broken response pipe must unwind task ownership: single GIF cancels unless a stop was already accepted; groups save valid parts/tail unless already cancelled. Host death must close the job. Check recovery only after owner PID **and creation time** no longer match; inability to verify liveness is a refusal, not permission.
- 保存与取消请求不可反转，终态幂等；断连、宿主退出、worker 卡死必须有界结束。失败记录至少包含 code/stage/message/time/retryability。日志写不成功也要通过结果暴露，不得谎报日志已保存；不记录请求正文、键鼠流或屏幕内容到诊断日志。
- Never retry UI clicks or keystrokes automatically. Missing frames/events remain explicitly missing. No fallback to a broader capture region, weaker path checks or disabled privacy masks.

## Durable output / 可靠产物

- Stage → flush/sync → validate → publish without overwrite → bind metadata/digests → receipt/index. Keep each successful publication even when later steps fail. Never delete valid output to make a failed operation look atomic.
- 分段 GIF 最长 300 秒；同一任务从首段开始就使用统一前缀和 part 编号。目录、索引、临时数据、检查点和最终产物职责清楚，禁止随机散落在工作目录。
- Recovery is restartable at every publication boundary. Reuse an existing output only after proving exact expected bytes and metadata; otherwise refuse or use the defined recovered name. Never overwrite a collision. Validate full decode, mapping and SHA-256 before committing recovery; retries may not create duplicate segments.
- Checkpoints and receipts are recovery evidence, not obsolete compatibility layers. Index backup is for interrupted writes, not older schemas. Path leases and no-overwrite semantics apply to every derived path, sidecar and recovery output.
- Every queue, file read, decode, event/sample collection, history, directory scan and finalizer must have a resource bound. Slow/disconnected/full storage may still fail; failure must preserve available evidence and report the stage. Do not claim protection against power loss or filesystem damage without corresponding evidence.

## Architecture / 架构

- CLI/MCP/GUI adapt requests and display state; application services own workflows. Windows capture/hooks stay out of pure timeline models. Encoding does not parse CLI options. Storage owns publication; identity owns random IDs, digests and process creation identity.
- Defaults are legitimate current product configuration. Recovery of the same format is legitimate durability logic. Neither is an excuse to keep a retired API or format reader.
- 不新增吞异常的万能 catch、无界队列/重试、生产代码测试后门或未经验证的成功兜底。清理错误不得掩盖原始错误；已经发布的产物不得被清理器当作临时文件删除。

## Verification gate / 验收门槛

- Required: formatting, warning-free Clippy, unit tests, release build, independent GIF decoding/time checks, official MCP SDK, invalid-input checks, worker kill/suspend, host kill, transport disconnect, cancellation races, recovery interrupted at each publication boundary, no-overwrite collisions and next-task success. Regressions must assert observable behavior, not repeat implementation logic.
- Fixes need a regression that reproduces the failure class. Core tests are time-bounded and emit SHA-bound summaries. A timeout is a failure and triggers process-tree cleanup; never skip a failed case or lower an assertion simply to turn CI green.
- GitHub Actions runs the core suite on Windows 2022 and 2025 on push/PR. Weekly and explicitly requested runs execute real CLI and MCP 650-second captures (300 + 300 + 50); accelerated clocks do not replace long acceptance. Fixed-seed mutation regression and dependency queries run automatically. Required jobs must not use continue-on-error.
- Actions use pinned commit SHAs and least-privilege permissions. PR cancellation is allowed, so interrupted-job safety must not rely on a final cleanup step. Upload summaries on failure too; do not upload raw desktop/input traces in normal CI artifacts. Test dependencies are pinned development dependencies, never recorder runtime requirements.
- Extract the actual portable ZIP and verify its EXE hash, current tool catalog, agent entry point, engineering standard and notices. Source-tree tests do not prove package correctness.
- 证据必须说明 EXE SHA、场景、实际帧率/时长、通过及失败项、尚未实测的范围。小区域跑满不能宣称所有画面跑满；本地通过不能宣称 GitHub 已通过；旧二进制的长测不能冒充本次构建。

## Documentation and delivery / 文档与交付

- README/About show core value and real examples; operational/build/test details live in focused docs discoverable from AGENTS.md and framelet_context. Remove stale duplicated guides rather than maintaining contradictory copies.
- Distinguish implemented features, design proposals and historical reports. Keep original demo media and historical measurement evidence unless specifically replacing them. Do not edit a past report to claim a new binary passed.
- Every delivery names what changed, what was tested and remaining limits. Commit only explicit intended paths; push and verify the remote SHA when delivery requires GitHub. Published tags/assets are immutable historical versions; never replace release files with changed bytes under the same version.

## Performance requirements / 性能要求

Use buffered JSON I/O with explicit flush errors; preserve flush/sync/publication order. Heavy analysis must never own stop/cancel/status dispatch. Bound queues, IPC, cancellation and all waits, including writes to a suspended child. Batch timestamps in a single forward pass; generate RGB only for selected frames. Default machine responses contain compact structured data and short text, with images by request. Cache immutable validated snapshots within a byte/entry/time budget and verify content under leases on every lookup; identical path/length/mtime never proves identity. Event limits must expose truncation/cursors and cannot silently hide central clicks behind movement traffic. Performance CI checks dense valid traces, transient recall, independent compositing, cache invalidation, control latency under a frozen analysis worker, IPC cancellation and EOF cleanup. Report model-call counts/bytes/images separately from native time; do not invent token or whole-workflow speedups.

## Installation and distribution

Native integration edits only its managed entry/files. Refuse unmanaged collisions unless adoption is explicitly selected, and restore the adopted entry on uninstall. Preserve recordings, user-modified skills, other servers and running clients. Persist intent before config/skill edits and resume only exact before/after states after interruptions. Keep config backups private and bounded; never log malformed TOML source lines or execute an arbitrary configured command during diagnostics. Test isolated install/check/uninstall, upgrade, real publication failure/retry and clean host discovery. Validate exact native bytes/skills across portable/MCPB/plugin packages, bind registry metadata to immutable Release URLs and hashes, and distinguish schema/SDK checks from actual GUI/client acceptance and public directory approval.
