# 技能共享、目录识别与 MCP 管理

核查日期：2026-11-04。文件路径中的 `~` 指当前用户主目录。以下是本机文件管理，不代表云端、手机端或所有历史版本都使用相同位置。

## 本次整改

- 默认只查明确的用户技能根目录与少量明确的系统技能根目录；不再遍历插件缓存、插件市场、程序安装包或连接器内部目录。
- 来源必须直接包含 `技能名/SKILL.md`，且能解析出非空 `name` 和 `description`。普通 Markdown、references、README 不单独算技能。
- 用户在工具内安装和手动创建的技能，往往落在同一个用户技能根目录，不能仅按文件名区分二者。界面统一标为“用户安装 / 自定义技能”；无法核实的私有安装目录允许手动指定。
- 文件存在只证明目录内容存在，不证明对应技能已在原 Agent 启用。系统技能来源只读，复制收编后才进入统一技能库。
- 自定义 Agent 支持名称、唯一 ID、Skills 目录、程序文件及 MCP 配置文件与格式。路径先保存，MCP 绑定失败时明确报告部分保存情况，允许修正重试。

## 常用 Agent 目录核查

| Agent | 用户安装 / 自建技能 | 用户级 MCP | 官方依据与限制 |
| --- | --- | --- | --- |
| WorkBuddy | 当前适配 `~/.workbuddy/skills`；系统只读 `skills/.system` | `~/.workbuddy/mcp.json`，`mcpServers` | [技能管理](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/Skills-Market)、[MCP](https://www.codebuddy.cn/docs/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/MCP-Guide)。技能页未承诺所有安装来源的统一磁盘路径；用户目录基于现有适配与本机目录，不能冒充官方保证。 |
| Codex | 官方 `~/.agents/skills`；兼容已有 `~/.codex/skills` 或 `CODEX_HOME/skills` | `$CODEX_HOME/config.toml`，`mcp_servers` | [Skills](https://learn.chatgpt.com/docs/build-skills)、[MCP](https://learn.chatgpt.com/docs/extend/mcp?surface=cli)。已有私有目录不自动迁移；插件目录不纳入。 |
| Claude Code | `~/.claude/skills` | `~/.claude.json`，`mcpServers` | [Skills](https://code.claude.com/docs/en/skills)、[MCP](https://code.claude.com/docs/en/mcp)。项目级 `.claude/skills`、`.mcp.json` 与用户级分开。 |
| Qoder IDE / CLI | `~/.qoder/skills`，可通过 `QODER_CONFIG_DIR` 改根目录 | `~/.qoder/settings.json`，`mcpServers` | [IDE Skills](https://docs.qoder.com/extensions/skills)、[CLI Skills](https://docs.qoder.com/cli/Skills)、[配置](https://docs.qoder.com/cli/settings-reference)。CLI MCP 需重启；CLI 不是 `~/.qoder-cli/skills`。 |
| Qoder CN | 保留现有 `~/.qoder-cn/skills` 适配 | 保留该产品配置适配，可自定义 | 国际版资料不能证明中国版各版本路径；不从国际版路径直接强迁移。 |
| QoderWork | 官方 `~/.qoderwork/skills` | 未确认统一外部可写文件 | [Skills](https://docs.qoder.com/qoderwork/skills)。与 Qoder IDE/CLI 区分；当前可通过自定义 Agent 接入，不虚构 MCP 配置。 |
| ZCode Agent | `~/.zcode/skills` | `~/.zcode/cli/config.json`，`mcp.servers` | [Skills](https://zcode.z.ai/en/docs/skill)、[MCP](https://zcode.z.ai/en/docs/mcp-services)。项目级 MCP 在 `.zcode/config.json`，层级不同。 |
| Cursor | `~/.cursor/skills`，也支持 `~/.agents/skills` | 既有 `~/.cursor/mcp.json` 适配，可自定义 | [Skills](https://cursor.com/help/customization/skills)。兼容目录可能让技能跨产品可见；本工具无法替原工具关闭其兼容扫描。 |
| Cline | `~/.cline/skills` | CLI 与 IDE 入口不同，保留格式识别及自定义绑定 | [Skills](https://docs.cline.bot/customization/skills)。本次修正原来的通用目录推断。 |
| Gemini CLI | `~/.gemini/skills`，支持 `~/.agents/skills` 别名 | 既有 `~/.gemini/settings.json` 适配 | [Skills](https://geminicli.com/docs/cli/skills/)。别名存在优先级，不代表独立分配。 |
| Qwen Code | `~/.qwen/skills`，也支持共享目录 | `~/.qwen/settings.json` | [Skills](https://qwenlm.github.io/qwen-code-docs/en/users/features/skills/)、[配置](https://qwenlm.github.io/qwen-code-docs/en/users/configuration/settings/)。扩展提供的技能属于扩展。 |
| OpenCode | `~/.config/opencode/skills`，遵循 XDG 配置目录 | 既有 `opencode.json/jsonc` 的 `mcp` 适配 | [Skills](https://opencode.ai/docs/skills)。新版还有单文件和额外来源形式；本工具只管理标准技能目录。 |
| Kilo | `~/.kilo/skills` | 新版 `~/.config/kilo/kilo.json` 的 `mcp` | [Skills](https://kilo.ai/docs/customize/skills)、[市场安装位置](https://kilo.ai/docs/customize/marketplace)。历史路径需保留兼容或手动绑定。 |
| Kiro | `~/.kiro/skills` | `~/.kiro/settings/mcp.json` | [配置作用域](https://kiro.dev/docs/cli/chat/configuration/)。不把云同步配置当成本机路径。 |
| Trae / 豆包等 | 保留已有明确用户目录及自定义路径 | 未确认可靠外部 schema 时使用原应用界面或用户显式绑定 | 本次不宣称完成其所有版本实机验证；停止插件与安装包深扫。 |

更多已有 MCP 适配见 [MCP-AGENTS.md](MCP-AGENTS.md)。原有规则数量不等于本次全部重新实测数量。

## GitHub 技能同步

入口：“设置 → 数据与备份 → GitHub 技能同步”。

1. 在 GitHub 创建专用仓库，建议使用私有仓库。在本机 Git 凭证管理器中登录，并配置 Git 提交身份；客户端不保存令牌。
2. 家里的电脑选独立技能库，填写 `owner/repo` 和分支（默认 `main`），点击“推送技能”。会在技能库自身初始化 Git、创建技能快照并推送；不操作 AgentSkillHub 的源码 Git 仓库。
3. 公司电脑先选择空技能库，填写同一仓库和分支，点击“拉取技能”。拉取后按需选择 Agent 并部署。
4. 日常遵循“先拉取 → 安装或修改 → 推送”。本地有改动、origin 不匹配、分支分叉或远端含非技能文件时停止；不自动改远程地址、不强推、不自动解冲突。

同步只暂存标准技能内的安全路径，保留技能脚本、文档、资源和 `_icon`。隐藏文件、回收站、依赖缓存、日志、常见凭据文件跳过；已跟踪的敏感路径则报错。隐藏文件对某些技能可能有用途，需要自行确认这种过滤是否适合该技能。MCP 凭证、本机绝对路径、应用设置和 Agent 分配不随技能同步。

已有 Git 历史会随推送传输。路径过滤不能发现正文、脚本或历史版本中的秘密；发送前仍需检查。已有本地快照若包含 `_archive` 等非技能内容，需使用干净专用技能库，软件不会重写历史。

## 版本通知

启动检查本项目 GitHub 最新正式 Release，成功结果缓存 24 小时；“设置 → 通用设置”可关闭自动检查或手动检查。发现较新版本时显示通知、“查看更新”和有对应资产时的“下载更新”。只接受本项目官方 Release 地址；不自动替换运行中的 exe、不自动发布版本。网络失败不会阻止启动。

## MCP 生效边界

启用、暂停、分配的配置修改在确认应用后立即写入目标文件，保留无关字段、原始备份与外部修改检查。暂停不会擅自终止目标 Agent 正在运行的进程。

目标 Agent 是否监视文件、是否重新连接 MCP、是否需要信任或 OAuth，取决于其自身。界面为每个 Agent 给出重载提示，无法观察目标运行状态时不显示“已运行”。Qoder CLI 官方要求重启；不能通过写文件承诺所有 Agent 的活跃会话即时生效。自定义路径只覆盖所选作用域，项目配置或启动参数仍可能覆盖用户配置。

## 清理范围

保留 `node_modules`、`dist`、`release`、`src-tauri/target`、运行环境和 `_tmp` 子目录的历史打包产物。`.mimosa` 状态与 `.claude/*.lock` 已加入忽略规则，不删除工具自己的配置或记忆。

`scripts/cleanup-nonessential.ps1` 默认预览，执行 `& "scripts/cleanup-nonessential.ps1" -Apply` 时仅删除 `_tmp` 顶层日志、截图、审计输出和旧清缓存脚本，不递归删除目录。本次已在用户确认后用该脚本清理 21 个候选文件，并复查构建与运行目录仍存在；保留可复用的验证脚本。
