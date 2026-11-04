# MCP 配置核查（2026-10-25）

对内置 Agent 清单逐项检索。官方文档优先，Magpie 仅作交叉参考；未核实 schema 的文件不默认写入。跨版本差异通过实际来源和手动绑定处理。此表区分配置位置已知与程序已适配，不把指南等同于自动管理支持。

参考：[Magpie 配置适配](https://github.com/yetone/magpie/blob/main/internal/library/targets.go)、[MCP 格式转换](https://github.com/yetone/magpie/blob/main/internal/library/mcp.go)。借鉴服务集中定义、按 Agent 分配、只移除自身管理配置的交互与原则，界面及实现独立编写。

| Agent | 用户级位置 | 已适配格式 | 依据 | 说明 |
| --- | --- | --- | --- | --- |
| codex | $CODEX_HOME/config.toml | codex | [来源](https://learn.chatgpt.com/docs/extend/mcp?surface=cli) | 用户级配置在 CLI、桌面及扩展间共享。修改后重新加载 MCP。 |
| claude | ~/.claude.json | json | [来源](https://code.claude.com/docs/en/mcp) | 用户级 mcpServers；项目配置 .mcp.json 与用户级配置分开。支持 CLAUDE_CONFIG_DIR。 |
| workbuddy | $WORKBUDDY_CONFIG_DIR/mcp.json | json | [来源](https://www.codebuddy.ai/docs/zh/workbuddy/From-Beginner-to-Expert-Guide/Function-Description/MCP-Guide) | 默认 ~/.workbuddy/mcp.json。新增后在 WorkBuddy 内信任服务；不修改信任或 OAuth 状态。 |
| zcode | ~/.zcode/cli/config.json | zcode | [来源](https://zcode.z.ai/en/docs/mcp-services) | 服务位于 mcp.servers；字段校验严格。 |
| qoder | ~/.qoder/settings.json | json | [来源](https://docs.qoder.com/qoder/connectors) | 用户级 mcpServers；IDE 与 CLI 共享目录。 |
| qoder-cn | ~/.qoder-cn/settings.json | json | [来源](https://docs.qoder.cn/cli/mcp-reference) | 国内版独立目录；不读取运行时 mcp-router.json。 |
| qoder-cli | ~/.qoder/settings.json | json | [来源](https://docs.qoder.com/cli/mcp-reference) | CLI 使用 ~/.qoder；同路径来源合并展示，写入前检查其他 Agent 的管理记录。 |
| cursor | ~/.cursor/mcp.json | json | [来源](https://cursor.com/docs/context/mcp) | 用户级和项目级 .cursor/mcp.json 合并，项目优先。 |
| windsurf | ~/.codeium/windsurf/mcp_config.json | json | [来源](https://docs.windsurf.com/windsurf/cascade/mcp) | 此路径适用于 Windsurf / Cascade。新版 Devin Local 使用 Devin CLI 的独立配置，请从应用中核对后绑定。 |
| gemini | ~/.gemini/settings.json | gemini | [来源](https://geminicli.com/docs/tools/mcp-server/) | mcpServers；HTTP 使用 httpUrl，SSE 使用 url。运行时停用另存于 mcp-server-enablement.json，由 Gemini 管理。 |
| qwen | ~/.qwen/settings.json | gemini | [来源](https://qwenlm.github.io/qwen-code-docs/en/users/features/mcp/) | mcpServers；HTTP 使用 httpUrl，SSE 使用 url。 |
| iflow | ~/.iflow/settings.json | gemini | [来源](https://platform.iflow.cn/en/cli/examples/mcp) | 用户级 mcpServers，与项目级配置分开。 |
| copilot | $COPILOT_HOME/mcp-config.json | copilot | [来源](https://docs.github.com/en/copilot/how-tos/copilot-cli/customize-copilot/add-mcp-servers) | 默认 ~/.copilot；本地 type=local，需要 tools。VS Code 的 mcp.json 不由 CLI 直接读取。 |
| cline | ~/.cline/data/settings/cline_mcp_settings.json | cline | [来源](https://docs.cline.bot/mcp/mcp-overview) | CLI 支持 CLINE_MCP_SETTINGS_PATH，transport 为对象；IDE 扩展另有自己的文件，优先使用实际检测到的来源。 |
| roo | $APPDATA/Code/User/globalStorage/rooveterinaryinc.roo-cline/settings/mcp_settings.json | json | [来源](https://roocodeinc.github.io/Roo-Code/features/mcp/using-mcp-in-roo/) | Roo MCP 面板可打开全局配置；同时扫描不同编辑器和 profiles。 |
| kiro | ~/.kiro/settings/mcp.json | json | [来源](https://kiro.dev/docs/mcp/configuration/) | 用户级 mcpServers；Agent 和项目配置可能覆盖全局。 |
| kimi-code | ~/.kimi-code/mcp.json | kimi | [来源](https://moonshotai.github.io/kimi-cli/en/customization/mcp) | 同时识别旧版 ~/.kimi/mcp.json；支持 KIMI_CODE_HOME / KIMI_SHARE_DIR，远程 transport 使用 http 或 sse。 |
| codebuddy | ~/.codebuddy/.mcp.json | json | [来源](https://www.codebuddy.ai/docs/cli/release-notes/v2.119.3) | 按实际存在的 .mcp.json、mcp.json、~/.codebuddy.json 选择。 |
| opencode | ~/.config/opencode/opencode.json | opencode | [来源](https://opencode.ai/docs/mcp-servers/) | 顶层 mcp；local 的 command 为数组，remote 使用 url；支持 JSONC 和 XDG_CONFIG_HOME。 |
| kilo | ~/.config/kilo/kilo.jsonc | opencode | [来源](https://kilo.ai/docs/automate/mcp/using-in-kilo-code) | 新版 Kilo 使用顶层 mcp；旧扩展配置只作为额外来源展示。 |
| antigravity | ~/.gemini/config/mcp_config.json | antigravity | [来源](https://www.antigravity.google/docs/mcp) | 新版 IDE / CLI 共用配置；远程端点字段为 serverUrl。旧目录另行只读发现。 |
| antigravity-cli | ~/.gemini/config/mcp_config.json | antigravity | [来源](https://www.antigravity.google/docs/mcp) | 与新版 Antigravity IDE 共用配置；不把旧 antigravity 目录当成新版默认。 |
| grok | $GROK_HOME/config.toml | grok | [来源](https://docs.x.ai/build/settings) | 默认 ~/.grok/config.toml；mcp_servers。 |
| junie | ~/.junie/mcp/mcp.json | json | [来源](https://junie.jetbrains.com/docs/junie-cli-mcp-configuration.html) | 用户级配置与项目级 .junie/mcp/mcp.json 分开。 |
| cortex | ~/.snowflake/cortex/mcp.json | json | [来源](https://docs.snowflake.com/en/user-guide/cortex-code/cortex-code-desktop/mcp-support) | 用户级 mcpServers，支持设置面板中的表单与 JSON 配置。 |
| deepagents | ~/.deepagents/.mcp.json | json | [来源](https://github.com/langchain-ai/docs/blob/main/src/oss/deepagents/code/mcp-tools.mdx) | 用户级配置；项目配置需要单独信任，不自动批准项目来源。 |
| factory | ~/.factory/mcp.json | json | [来源](https://docs.factory.com/cli/configuration/mcp) | Factory Droid 的用户级 MCP 配置。 |
| posit | ~/.posit/assistant/settings.json | posit | [来源](https://assistant.posit.co/docs/reference/config-file/) | 顶层 mcpServers；local 的 command 为数组，environment 保存环境变量，remote 使用 url。 |
| kimchi | ~/.config/kimchi/harness/mcp.json | json | [来源](https://docs.kimchi.dev/docs/coding-mcp-servers) | 通过 Kimchi MCP 面板或 CLI 导入，使用 mcpServers。 |
| claude-desktop | $APPDATA/Claude/claude_desktop_config.json | desktop | [来源](https://py.sdk.modelcontextprotocol.io/get-started/real-host/) | 本地文件只配置 STDIO；远程连接器在 Claude Desktop 内管理。 |
| hermes-agent | ~/.hermes/config.yaml | 仅指南 | [来源](https://hermes-agent.nousresearch.com/docs/user-guide/features/mcp/) | YAML mcp_servers；当前提供配置指南，不自动改写 YAML。 |
| goose | ~/.config/goose/config.yaml | 仅指南 | [来源](https://block.github.io/goose/docs/guides/config-files/) | YAML extensions；通过 Goose 的 Extensions 或 goose configure 管理。 |
| continue | ~/.continue/config.yaml | 仅指南 | [来源](https://docs.continue.dev/reference) | YAML mcpServers 为列表，当前使用 Continue 面板配置，不套用 JSON 对象。 |
| crush | ~/.config/crush/crush.json | crush | [来源](https://github.com/charmbracelet/crush/blob/main/internal/skills/builtin/crush-config/SKILL.md) | JSON 顶层 mcp；crushrc 也可定义配置，本应用不执行或改写 Bash 配置。 |
| autohand-code | ~/.autohand/config.json | 仅指南 | [来源](https://docs.autohand.ai/working-with-autohand-code/mcp-servers) | mcp.servers 为数组，当前提供指南，不套用 Zcode 的对象格式。 |
| pochi | ~/.pochi/config.jsonc | pochi | [来源](https://docs.getpochi.com/mcp/) | 顶层 mcp，保留其他配置。 |
| aider-desk | ~/.aider-desk/mcp-servers.json | 仅指南 | [来源](https://github.com/hotovo/aider-desk/discussions/901) | 已确认独立配置文件；版本字段结构未完成适配，请在 MCP 设置面板管理。 |
| rovodev | ~/.rovodev/mcp.json | 仅指南 | [来源](https://support.atlassian.com/rovo/docs/use-rovo-dev-in-the-ide/) | 已确认路径；请在 Rovo Dev 中管理，当前未验证完整 schema。 |
| vibe | ~/.vibe/config.toml | 仅指南 | [来源](https://docs.mistral.ai/vibe/code/cli/mcp-servers) | TOML mcp_servers 使用 Vibe 自身 schema，暂不自动转换。 |
| codeartsdoer | ~/.codeartsdoer/codearts_cli.jsonc | 仅指南 | [来源](https://support.huaweicloud.com/intl/zh-cn/usermanual-cli/codeartsagent_cli_0028.html) | 官方确认用户级文件位置；请使用 codearts mcp add 管理。未核实完整字段 schema，不自动套用 OpenCode 格式。 |
| jazz | ~/.agents/mcp.json | 仅指南 | [来源](https://jazz-cli.vercel.app/docs/configure/mcp) | 配置在通用 .agents 目录，信任与启用另存 ~/.jazz/config.json；当前通过 Jazz 管理。 |
| openclaw | ~/.openclaw/openclaw.json | 仅指南 | [来源](https://docs.openclaw.ai/slash-commands) | 使用 /mcp 或 OpenClaw 设置；嵌入 Agent 的配置不等同于网关 MCP。 |
| astrbot | 应用内 / 未确认 | 仅指南 | [来源](https://docs.astrbot.app/use/mcp.html) | WebUI → 插件 → MCP；数据库/部署目录由实例决定。 |
| lingxi | 应用内 / 未确认 | 仅指南 | [来源](https://www.lingxi.cn/docs) | 应用内「设置 → 连接器管理」，当前未确认稳定的外部文件格式。 |
| trae | 应用内 / 未确认 | 仅指南 | [来源](https://docs.trae.cn/ide_add-mcp-servers) | 设置 → MCP → 原始配置（JSON）可打开真实用户文件；绑定实际路径后管理，不猜测私有目录。 |
| trae-cn | 应用内 / 未确认 | 仅指南 | [来源](https://docs.trae.cn/ide_add-mcp-servers) | 设置 → MCP → 原始配置（JSON）；项目级 .trae/mcp.json 与用户级分开。 |
| lingma | 应用内 / 未确认 | 仅指南 | [来源](https://help.aliyun.com/zh/lingma/agent) | 产品已转向 Qoder CN；使用 Qoder CN MCP 面板，旧版本路径不混用。 |
| doubao | 应用内 / 未确认 | 仅指南 | [来源](https://www.doubao.com/) | 技能·连接器·伙伴 → 新建自定义连接器；未查到可核实的外部文件 schema。 |
| minimax | 应用内 / 未确认 | 仅指南 | [来源](https://agent.minimaxi.com/about) | 通过插件/连接器管理，未确认稳定的用户级独立 MCP 文件。 |
| meituan-catpaw | 应用内 / 未确认 | 仅指南 | [来源](https://catpaw.meituan.com/docs) | 通过妙手 MCP 连接器界面配置；当前未确认外部文件 schema。 |
| augment | 应用内 / 未确认 | 仅指南 | [来源](https://docs.augmentcode.com/) | 使用 Augment MCP 设置；当前未确认该已识别版本的用户文件 schema。 |
| tabnine | ~/.tabnine/mcp_servers.json | 仅指南 | [来源](https://docs.tabnine.com/main/getting-started/tabnine-agent/agent-settings) | Tools and MCPs → MCP servers 打开实际文件；CLI/IDE 版本位置可能不同，暂不自动绑定。 |
| zencoder | 应用内 / 未确认 | 仅指南 | [来源](https://docs.zencoder.ai/) | 使用 Zencoder MCP 设置；当前未确认用户级外部文件格式。 |
| devin | ~/.devin/mcp_config.json | 仅指南 | [来源](https://github.com/yetone/magpie/blob/main/internal/library/targets.go) | Magpie 源码提供位置线索；未取得足够的 Devin 官方文件 schema，不自动写入。 |
| pi | ~/.pi/agent/mcp.json | 仅指南 | [来源](https://github.com/badlogic/pi-mono) | Pi 不同发行版/扩展的 MCP 格式不同；请通过所装扩展或当前版本配置，不猜测。 |
| inference-sh | 应用内 / 未确认 | 仅指南 | [来源](https://inference.sh/docs/connectors/mcp-server) | 平台提供 MCP 代理/连接工具；没有核实通用用户级配置文件。 |
| codemaker | 应用内 / 未确认 | 仅指南 | 未查到可靠来源 | 已逐项检索，未找到可核实的官方用户级 MCP 配置文档。 |
| codestudio | 应用内 / 未确认 | 仅指南 | 未查到可靠来源 | 产品名存在歧义，未找到足以确认当前 Agent 的 MCP 格式的官方资料。 |
| firebender | 应用内 / 未确认 | 仅指南 | 未查到可靠来源 | 项目 firebender.json 存在线索，但未核实用户级配置，不自动绑定。 |
| forge | 应用内 / 未确认 | 仅指南 | [来源](https://forgecode.dev/) | 已检索官方资料，未确认用户级 MCP 配置 schema。 |
| mcpjam | 应用内 / 未确认 | 仅指南 | [来源](https://docs.mcpjam.com/inspector/workspaces) | 在 MCPJam 项目的 Servers 页管理；非普通 Agent 用户配置。 |
| mux | 应用内 / 未确认 | 仅指南 | [来源](https://github.com/coder/mux) | 已检索，未核实此版本的用户级 MCP 配置路径与 schema。 |
| neovate | 应用内 / 未确认 | 仅指南 | [来源](https://github.com/neovateai/neovate-code) | 已检索源码线索，未完成用户级 schema 验证。 |
| openhands | 应用内 / 未确认 | 仅指南 | [来源](https://docs.openhands.dev/api-reference/store-settings) | MCP 由 OpenHands 设置中的 mcp_config 管理；部署和 SDK 配置不同，当前不自动写入。 |
