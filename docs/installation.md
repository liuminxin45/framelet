# Install Framelet for AI / 安装与接入

Framelet is a Windows x64 native executable and local stdio MCP server. It does not install a network daemon or capture during connection. The AI host starts `framelet.exe mcp`; the host's UI tools operate the target application. Cloud hosts need an authorized executor on the recorded Windows desktop.

Framelet 是 Windows x64 原生 EXE。AI 客户端按配置启动本机 MCP，不需要常驻网络服务；连接不自动开始录屏。不要把另一台电脑上的 EXE 路径当成云端可访问的地址。

## Portable + native integration / 便携包与原生安装

Download the versioned Windows portable ZIP from Releases, check its SHA-256, and extract to a stable local directory. Run from that directory:

```powershell
.\framelet.exe integrate install --client codex
.\framelet.exe integrate check --client codex
```

Choose one client per invocation: `codex`, `claude-code`, `cursor`, or `claude-desktop`. Reconnect/restart that client's MCP connection after install or upgrade. No admin rights, Rust, Python, Node, .NET or FFmpeg runtime is required.

每次选择一个客户端；可分别安装多个客户端。安装器注册当前 EXE 的绝对路径，不移动或删除其他版本。先解压到长期保留的位置，再运行 install；升级时从新版本目录重跑。check 会验证配置、已管理的 Skill 文件、实际 MCP 握手、工具目录与 context；它不会录屏，也不会把 SDK 握手当成目标 GUI 客户端已验收。

| Client | Default user config | Skill discovery |
| --- | --- | --- |
| Codex | `$CODEX_HOME/config.toml`, otherwise `%USERPROFILE%/.codex/config.toml` | `%USERPROFILE%/.agents/skills/framelet` |
| Claude Code | `$CLAUDE_CONFIG_DIR/.claude.json` when set, otherwise `%USERPROFILE%/.claude.json` | `$CLAUDE_CONFIG_DIR/skills/framelet`, otherwise `%USERPROFILE%/.claude/skills/framelet` |
| Cursor | `%USERPROFILE%/.cursor/mcp.json` | `%USERPROFILE%/.cursor/skills/framelet` |
| Claude Desktop | `%APPDATA%/Claude/claude_desktop_config.json` | Import the separate skill ZIP through the client's supported skill UI; no filesystem auto-registration is claimed |

Custom profiles use explicit `--config <file>` and, where supported, `--skill-dir <framelet-directory>` on install/check/uninstall. Use `--output-root <directory>` on install to choose the recording root. The default root is `%LOCALAPPDATA%/Framelet/Captures`. Extra read roots remain opt-in through client settings; the installer grants no extra read directories.

现有同名服务默认拒绝覆盖。要明确接管它，使用 `--adopt-existing`：安装器保存原来的 Framelet 条目，卸载时还原；其他 MCP、模型设置与项目数据保留。JSON 配置要求标准 JSON；含注释的 JSONC 会明确拒绝，不尝试猜测或吞掉内容。TOML 使用保留格式的解析器，保留无关配置及注释。

## Check, retry and uninstall / 校验、重试与卸载

```powershell
.\framelet.exe integrate uninstall --client codex
```

Uninstall removes the managed integration/skill files, or restores an explicitly adopted entry. It preserves captures, unrelated config, user-added files, portable binaries and running clients. A user-modified managed entry/skill is preserved and reported as a conflict instead of silently removed.

配置和 Skill 的改动先写入持久化事务日志。中途失败后重跑 install/uninstall，先完成可验证的 before/after 状态；未知改动被拒绝。配置备份与收据保存在配置目录下 `.framelet-integrations`，可能包含被接管条目的私有环境设置，不要公开上传它们。备份有数量上限，达到上限时明确拒绝继续写入。安装器协调自己的并发调用并在发布前检查外部改动；它不是阻止其他本地程序竞态编辑的 OS 沙箱。

## Distribution choices / 分发渠道

- **Portable ZIP:** full native tool, local docs, notices and Skill source. Use the native integration commands above.
- **MCPB:** a binary bundle for clients implementing MCPB installation. It declares Windows support and lets the user choose a capture directory. It does not require Node/Python. Its embedded Skill files are not automatically activated by every MCPB host; the separate skill ZIP is available.
- **Skill ZIP:** one `framelet` folder with SKILL.md, three focused recipes and UI metadata. Use a client's supported skill import or native installation. A skill's discovery does not guarantee every model will select it.
- **Codex plugin ZIP:** native EXE, root plugin.json/mcp.json and Skill. Extract/install with the client's supported local-plugin flow. OpenAI public local-MCP directory submissions require approved-partner eligibility; a generated ZIP is not a public approval.
- **Claude Code plugin ZIP:** Skill plus local MCPB. The repository marketplace points at the immutable Release MCPB, so a Git install does not need Rust or a committed EXE.

For the repository marketplace, after the corresponding Release exists:

```text
/plugin marketplace add liuminxin45/framelet
/plugin install framelet@framelet
```

Choose either the native registration or the plugin for a client to avoid duplicate servers. Directory publication is separate from package creation. The MCP Registry lists server metadata; it does not cause every AI host to install or connect automatically. Registry metadata binds the specific Release MCPB URL and SHA-256.

## Agent entry points / Agent 入口

The Skill description makes recording/testing/demo/editing goals discoverable. Its short entry point routes to testing, demos or editing recipes. Once connected, read `framelet_context` and the current catalog; detailed documents are exposed through local docs and MCP resources. See [performance.md](performance.md) for the current wait/analyze/batch workflow.

源码、便携包、Skill 与运行时文档保持一份现行接口。不要照抄历史录制工具名，也不要把 README 当完整 API 手册。

## Verified surfaces / 验证范围

The native regression checks isolated configs for all four clients, repeat install, executable-path upgrades, adoption, private config rejection, modified skills, no-delete uninstall and real interrupted publication/retry. Host acceptance separately exercises real Codex app-server tool/Skill discovery and Claude Code connection. Bundle validation and config tests are not a claim of interactive Cursor or Claude Desktop GUI acceptance. Consult the SHA-bound delivery report for actual versions/results.

Sources: [Codex MCP](https://developers.openai.com/codex/mcp), [OpenAI local plugin packaging](https://developers.openai.com/plugins/deploy/desktop-only), [Claude Code MCP](https://code.claude.com/docs/en/mcp), [Claude plugin manifest](https://code.claude.com/docs/en/plugins-reference), [Cursor MCP](https://cursor.com/docs/mcp), [Cursor skills](https://cursor.com/docs/skills), [MCPB](https://github.com/modelcontextprotocol/mcpb), [Registry package types](https://github.com/modelcontextprotocol/registry/blob/main/docs/modelcontextprotocol-io/package-types.mdx).
