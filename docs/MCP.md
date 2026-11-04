# MCP 集成管理（v0.6.0）

MCP 管理以用户自己配置的服务为中心。配置只保存在本机，默认页不展示内置连接器，也不把插件缓存条目算进“我的服务”。

## 工作方式

- **我的 MCP**：合并服务库与本机用户配置，查看连接方式、配置来源和已分配 Agent；同名不同配置分别展示。
- **按 Agent**：查看这个 Agent 的用户服务，直接编辑、启用、暂停、移除；配置位置、文档依据和恢复入口收在详情中。
- **发现与导入**：默认仅发现明确的用户配置，不再递归扫描插件和缓存。单项导入、批量导入只复制定义。
- **MCP 市场**：实时查询官方 MCP Registry，支持关键词、翻页、参数填写与分配安装。
- **配置指南**：对内置 Agent 清单逐项核查，展示用户级位置、格式、文档链接、版本注意事项与适配状态。

完整核查表见 [MCP-AGENTS.md](MCP-AGENTS.md)。程序使用同一份 [内置配置目录](../src-tauri/resources/mcp-agents.json)，测试确保清单覆盖所有已知 Agent；“配置位置已确认”与“格式已适配”分别处理。

## 添加、分配、启用

1. 点击“添加 MCP”，选择命令 / HTTP / SSE。完整命令与参数写在同一行，含空格的路径加引号；环境变量或请求头用键值行填写。也可切换到 JSON，一次粘贴完整的 mcpServers 配置导入多项。
2. 在同一弹窗中选择一个或多个 Agent；不选择时仅保存到服务库。按 Agent 页发起新增时，默认勾选当前可配置的 Agent。取消已选项会预览暂停对应 Agent 中的服务。
3. 预览中逐个列出 Agent、实际文件和新增/更新/暂停等动作；确认后分别写入各自的原生格式。
4. 编辑库中已有服务后再次分配，可以把更新同步到当前已分配的 Agent。修改原 Agent 中的服务则只影响该来源，不静默同步其他 Agent。
5. 默认保存后自动检测连接、MCP 握手与工具列表；主列表直接显示结果。目标 Agent 仍可能需要重新加载，以及它独立要求的信任或 OAuth。

配置管理开关沿用 Agent 安装证据与“允许配置”规则。工具尚未确认安装、扫描尚未完成或配置管理关闭时，不允许写入；按 Agent 页可为已安装工具开启配置管理。非默认配置可手动绑定已适配的格式。

## 配置后的自动检测

编辑器支持保存前“立即检测”，并默认勾选“保存后自动检测连接和工具列表”。配置应用或重新启用后也会检测；可以关闭保存后的检测。主列表提供单项重新检测、停止和“检测全部”，最多同时运行两个任务。打开页面或扫描配置本身不会自动连接服务。

检测会启动 STDIO 服务，或访问 HTTP / SSE 端点，依次执行 `initialize`、`notifications/initialized`、分页 `tools/list`，并响应服务器 ping。HTTP 支持 JSON 和事件流响应、协议版本与会话头，旧版 SSE 支持 endpoint/message 事件。成功后显示服务版本、协议版本、工具数量及工具名称与描述；空工具列表单独提示。找不到命令、环境变量缺失、认证失败、非 MCP 网页、协议不兼容、启动退出、超时和取消均有独立结果。

检测不会执行 `tools/call`，不替用户完成 OAuth，也不验证每个业务操作的权限。绿色结果表示当前配置已成功连接并发现工具，不保证原 Agent 已刷新配置或实际业务调用全部成功。结果仅保留在本次应用会话中，并附检测时间；配置变化后使用独立结果。

启动超时允许 1–300 秒，留空默认 30 秒，覆盖连接、握手和工具发现全过程。服务库保存此检测设置，不向 Agent 的原生配置注入额外字段，也不覆盖其原有超时。直接编辑原 Agent 配置时，该值用于本次检测。支持检测时解析本机 `${NAME}` / `${env:NAME}`，以及 Codex 的 `bearer_token_env_var`、`env_http_headers`；插件交互变量需要在原宿主中处理。

Windows 检测进程使用 Job 管理。结束、超时或停止时关闭本次启动的进程树；远程检测结束会尝试释放本次 HTTP 会话。HTTP 使用系统 curl，不全局安装运行环境。检测结果遮蔽配置中的令牌、环境变量值和请求头值；不记录业务响应。命令模式可能由 npx / uvx 下载并启动服务，与客户端启动 MCP 的行为一致。

## 暂停与恢复

不同 Agent 对禁用字段的支持不一致，因此“暂停”会将该服务从对应 Agent 文件移出，并在本应用中保留其完整原始定义及归属。服务库和其他 Agent 不受影响。“重新启用”恢复该定义；目标已有同名条目时会拒绝覆盖。原配置中已设置 enabled=false 或 disabled=true 的服务可通过明确的启用操作开启。

暂停记录会持久化，重开应用仍可恢复。编辑暂停中的定义不启动服务。“移除”针对当前所选来源，服务库删除与原 Agent 删除各有明确操作。

## 格式适配

已适配 JSON/JSONC 与 TOML，针对客户端处理差异，例如：

- Codex：mcp_servers、http_headers，拒绝不能直接使用的 SSE。
- Gemini / Qwen / iFlow：远程 HTTP 使用 httpUrl，SSE 使用 url。
- OpenCode / 新版 Kilo：顶层 mcp，local 的 command 是数组，环境变量为 environment。
- Posit Assistant：同样使用 command 数组，但位于 mcpServers。
- Cline CLI：transport 为对象；IDE 扩展是独立文件，按实际来源区分。
- Antigravity：新版 ~/.gemini/config/mcp_config.json，远程使用 serverUrl。
- Copilot CLI：本地 type=local，并补充 tools；不把 VS Code 格式混进 CLI 文件。
- Kimi：同时识别新旧目录，远程传输使用其 transport 字段。
- Zcode：嵌套 mcp.servers，严格检查未知字段。
- Claude Desktop：本地文件只支持 STDIO；远程连接器留在原应用管理。

YAML、数组型服务列表、数据库或未核实 schema 的产品提供配置指南，不猜测成通用 JSON。Trae 官方提供“原始配置（JSON）”入口；可在原应用核实路径后绑定兼容文件。Windsurf / Cascade 与新版 Devin Local 配置不同，指南会说明版本边界。

环境变量覆盖只读取、不改动系统设置。路径从当前用户与 Agent 环境解析，不写死开发者用户名。编辑器扩展与 profiles 单独发现，不依赖 skills 目录是否存在。

## 配置与凭证

主列表不显示 URL、参数、环境变量值或请求头值。表单中的环境变量和请求头值默认遮蔽；JSON 模式是显式编辑入口，可能包含凭证。导入保留原始禁用状态；不能可靠移植的插件变量或客户端专有字段会给出错误，不静默丢弃。

应用数据、暂停定义与原始字节备份可能包含本地凭证，应与原 Agent 配置同样保管。

## 写入保护

- JSONC 只替换 MCP 节点，保留节点外注释与设置；TOML 保留无关节点。被替换 MCP 节点内的格式与注释可能重新生成。
- 不覆盖内容不同的同名外部服务。不移除其他程序添加或修改的服务。
- 每份预览各自持有目标原始字节，应用时重新校验目标、服务库、绑定、暂停及管理记录。
- 多 Agent 预览独立保存，避免后一次预览使前一次失效；同一物理配置文件不重复应用。各 Agent 分别提交；部分失败时保留成功项并明确显示失败，要求重新预览。
- 修改前保留原始字节备份；“恢复上次修改”核对完整文件，存在后续外部修改时停止。
- Windows 优先 ReplaceFile 保留目标属性。加密过滤驱动拒绝替换时，先同步落盘同目录 .agentskillhub-recovery-<文件名>.json，再兼容原位写入。兼容方式不保证断电原子性；写入异常会尝试恢复，进程中断可使用副本中的 before 字节恢复。
- 内置、插件、缓存、市场目录始终只读，不能绑定为部署目标。

旧版 mcp-library.json 的服务、分配和恢复记录继续读取；新增暂停记录使用默认空值，不清空已有配置。

## 市场边界

官方 Registry 是发布者注册目录，不代表官方审核。支持 HTTPS 远程 HTTP/SSE 和标准 npm Registry 的固定版本 STDIO 包。标准 -y / --yes 运行参数可识别，复杂运行参数、重复参数或其他包格式仍需手动配置。npm 服务需要 Node.js；首次检测或在目标 Agent 中启动时，由 npx 下载相应包。本应用不会全局安装运行环境。

## 实现参考与验证

交互和管理原则参考 [Magpie Library](https://github.com/yetone/magpie/blob/main/internal/library/library.go) 与其 [Agent 适配](https://github.com/yetone/magpie/blob/main/internal/library/targets.go)。界面及实现独立编写，服务保存、客户端转换、预览、应用、恢复复用同一条配置链路。

验证命令：

- npx vue-tsc --noEmit
- npm run build
- cargo test --manifest-path src-tauri/Cargo.toml --lib
- cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
- node scripts/verify-mcp.mjs --build
- node scripts/test-mcp-command.mjs

桌面验证使用独立应用标识与临时 Agent 文件，覆盖用户/内置隔离、原配置编辑、暂停与恢复、多 Agent 分配、库更新同步、冲突保护、完整 JSON 导入、真实市场搜索与配置写入，以及窄窗口/深色主题。真实可用性检测仅启动隔离的本地测试服务，覆盖 STDIO、HTTP、SSE、分页、ping、会话、认证失败、超时、主动停止及进程树清理；不启动真实 Agent 配置中的服务，不下载市场中的第三方测试包，不调用业务工具。

检测协议依据：[MCP 生命周期](https://modelcontextprotocol.io/specification/2025-06-18/basic/lifecycle)、[STDIO / Streamable HTTP](https://modelcontextprotocol.io/specification/2025-06-18/basic/transports)、[旧版 SSE](https://modelcontextprotocol.io/specification/2024-11-05/basic/transports)。

## 配置生效与自定义工具

自定义 Agent 可同时填写 Skills 目录和 MCP 配置文件、格式。确认应用后立即保存磁盘配置，按 Agent 页面显示具体的重新连接 / 重启提示；配置写入与运行中的 MCP 连接状态分开。Qoder CLI 官方要求重启，不能承诺所有 Agent 热重载。暂停配置不擅自终止原工具的进程。详见 [目录核查与共享说明](SHARING-AND-AGENTS.md)。
