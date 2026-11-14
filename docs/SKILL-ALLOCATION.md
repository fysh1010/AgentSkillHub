# 技能分配与外部变更刷新

## 使用方式

- 侧栏仅保留「技能库」统一入口，提供列表、卡片和按 Agent 视图。技能详情、按 Agent 的部署预览与回滚均保留；旧 `agents` 页面跳转兼容到技能库按 Agent 视图。
- 按 Agent 列表右侧的数字统计当前技能库实际启用的技能，与该 Agent 下全部开关保持一致；不使用历史配置数量，不计入库外自有技能，也不随关键词筛选缩小。
- 图标亮起表示磁盘上实际存在指向技能库的链接；点击可立即创建或移除当前技能的 Junction。其他待部署配置不会跟着应用。源技能不变。
- 每个技能旁支持全选、全不选、反选 Agent；表头支持选择当前筛选中的多个技能，指定 Agent、启用、停用或反转状态，先预览再确认。
- 分配图标、批量选项和 MCP 编辑器只列出已开启管理的 Agent。未开启或未确认安装的工具留在「设置 → Agent 工具」中；关闭工具不会删除已有链接或改写原 MCP 配置。已识别品牌使用随应用分发的图标，自定义工具使用首字占位。
- 旧的整目录链接须先在「按 Agent」导入当前配置并部署迁移；技能库本身不能作为即时开关的写入目录。多个 Agent 共用目录时会反映同一实际状态，矛盾的批次请求会被拒绝。

## Agent 自有技能

技能库通过顶部横向标签切换库中技能与「Agent 自有技能」，同一时间只显示所选列表。已开启工具中，其他软件安装的普通目录、库外链接复用标准 `SKILL.md` 扫描，不以符号链接作为发现前提。私有位置在 Agent 工具中设置用户 Skills 目录。

「设置 → 工具技能」和总览的工具技能入口只显示已开启 Agent 的用户目录，不显示系统技能、插件包或内置来源。用户安装、自定义和本工具分配的技能可以只读查看及复制入库；Agent 内置技能由其自身维护。已保存的历史来源配置保留，不自动迁移或删除。

「导入技能库」复制副本，保留原目录。重复名称拒绝覆盖；库中有同名的外部技能以琥珀色标识，普通分配操作会保留并跳过，不能冒充已部署的库中版本。导入后切回技能库标签，可将库中版本分配给其他 Agent。

需要统一管理同名实体技能时，可在「按 Agent」点击「外部同名 · 转为链接」，或在「Agent 自有技能」点击「备份并转为链接」。先检查完整来源及库中目标的内容指纹，再显示路径并要求确认；确认后将原实体目录重命名到同级 `.agentskillhub-backups/<唯一标识>`，在 Agent 用户技能根目录创建库链接。支持分类子目录中的用户技能，恢复时回到原来的分类位置。原字节完整保留，不进入扫描列表；创建链接或保存配置失败时恢复原目录。技能内部含链接、来源与库相互包含、预览后内容变化均拒绝操作。

「Agent 自有技能 → 原目录备份」可确认恢复；原位置只允许空缺或仍指向预期库目标的链接。备份被外部改动或原位置出现其他内容时，不覆盖。备份记录保存在原应用配置中，清理配置前应先恢复或自行保存这些目录。

`~/.agents/skills` 单独显示为「Agents 通用共享目录」，不归属 Codex。它是跨客户端约定，并不保证所有 Agent 都加载；见 [Agent Skills 客户端实现说明](https://agentskills.io/client-implementation/adding-skills-support)。

导入目标独占创建，复制失败清理本次新建的目标。技能内部的链接拒绝跟随，`.git` 与 `node_modules` 不复制，层级与条目数设上限，避免将技能引用的其他目录带入库中。

## 刷新与生效

窗口可见时每 2 秒读取目录和 MCP 配置文件的元数据指纹；变化后更新技能、实际分配、Agent 自有技能、来源目录及 MCP 列表。回到窗口立即检查；本机工具发现最长每 30 秒再检查一次。轮询串行执行，隐藏时暂停，不自动执行部署。正在编辑或确认 MCP 变更时保留当前审阅内容；应用时继续执行已有外部修改检查。

指纹复用扫描模块，包含文件名、修改时间及大小；标准技能中的链接会观察其内容。技能库的多层包装目录均参与刷新检测，技能无论放在多少层分类下，其新增、移除及 `SKILL.md` 修改都在覆盖范围内。技能自身内部超过三层的资源文件仍没有完整递归监听，其变更可能延迟感知；MCP 绑定文件保持原有检测方式。

这里的即时启停指目录链接已落盘。目标 Agent 的活跃会话是否重载技能，由其自身实现决定。

## MCP 图标分配

「我的 MCP」每行复用技能的 Agent 图标组件，只展示已开启 Agent、悬停名称、实际启停状态和全选/全不选/反选。保留「按 Agent」详细管理及分配弹窗，弹窗同样提供批量选择。图标亮起只依据实际读取的用户配置，不以历史管理记录推断启用。

独立的「配置指南」页和 MCP 内的路径绑定弹窗已移除。Skills 目录、MCP 配置路径/格式、产品说明与路径编辑集中在「设置 → Agent 工具」，开关保存后重新读取适配结果。已支持产品复用已知路径规则，未知或便携工具可显式填写自定义位置；工具开启本身不安装软件、不部署技能，也不自动修改原 MCP 文件。

点击 MCP 图标或批量选择后，复用现有 IPC 生成变更预览，确认后写入；取消不改变 Agent 配置。单个图标只生成该 Agent 的计划，不顺带更新其他 Agent。原有配置暂停和恢复不强制收编到服务库，向新 Agent 分配时才按需保存定义。同名不同配置标出并阻止接管，已托管版本的更新仍经预览和外部修改检查。

技能与 MCP 共用 `AgentAssignmentList.vue`；来源横向标签共用 `ResourceTabs.vue`。分配入口复用现有格式转换、备份、外部修改检测与权限判断，没有增加依赖。

## Trae MCP 与市场源

Trae / Trae CN 用户级 MCP 默认绑定 `%APPDATA%/Trae[ CN]/User/mcp.json`，格式为 `mcpServers`。优先已有文件，兼容已有 `TRAE SOLO[ CN]/User/mcp.json`；自定义用户档使用设置里的实际路径绑定。项目 `.trae/mcp.json` 不作为全局分配目标。本机 Trae CN 已安装程序的 `resources/app/out/vs/workbench/workbench.desktop.main.js` 中，MCP 服务将 `userSettingsResource/../mcp.json` 作为用户文件；其 package.json 基线版本为 1.107.1。格式与信任操作依据 [Trae 官方 MCP 文档](https://docs.trae.cn/ide_add-mcp-servers)，实际连接在 Trae 内确认。

重复的「发现与导入」页已移除；自动发现的用户服务合并进「我的 MCP」，「按 Agent」保留编辑、暂停、另存到服务库等操作。读取错误直接在相应列表显示。

MCP 市场支持官方 Registry、自定义兼容 Registry API 和 HTTP(S) JSON 清单。来源保存在现有 `mcp-library.json` 中；Registry 根地址自动补 `/v0.1/servers`，特殊版本可填写完整端点。JSON 接受 `servers`（官方包装或平铺条目）以及 `mcpServers` 对象；配置只进入既有编辑与预览流程，不在查询时执行。图标优先 `icons[].src`，其次 GitHub 发布者头像，最后为本地语义占位；图片失败自动降级。

服务字段依据 [MCP Registry server.json 文档](https://github.com/modelcontextprotocol/registry/blob/main/docs/reference/server-json/generic-server-json.md)。简单清单示例：

```json
{"servers":[{"name":"team-search","title":"团队搜索","description":"内部资料检索","icons":[{"src":"https://example.com/icon.png"}],"config":{"type":"http","url":"https://example.com/mcp"}}]}
```

## 实现与验证

- `assignments.rs`：批次预检查、只操作选中技能、失败撤销；审计复用现有部署记录与回滚入口。
- `SkillAssignments.vue` / `SkillAllocation.vue`：共享图标开关与批量选择；`AssignmentPreview.vue` 统一显示预览。
- `useLiveRefresh.ts` / `workspace_stamp`：只读变更检测，后台刷新不重置筛选与选择。
- 保持前后端模型一致；复用技能扫描、Agent 权限、Junction、审计和 MCP 解析，不增加依赖。

```powershell
npx vue-tsc --noEmit
npm run build
cargo test --manifest-path "src-tauri/Cargo.toml" --lib --locked
node "scripts/verify-skills.mjs" --build
node "scripts/verify-mcp.mjs" --build --offline
```

桌面验证使用独立 `com.agentskillhub.skillvalidation` 标识、临时目录和 9241 调试端口，覆盖真实 Junction 启停、全选/全不选/反选、预览不落盘、实体技能导入与链接转换/恢复、外部增改移出、MCP 自动刷新及图标加载；新加入的 HTTP 夹具验证自定义市场源的搜索、图标和安装入口，Trae CN 使用临时用户文件验证默认适配。截图保存在 `_tmp/` 中。验证完成后从正常 Tauri 开发模式启动预览，无需发布打包。

原有 MCP 回归使用独立 `com.agentskillhub.validation` 标识；`--offline` 以本地夹具替代外部 Registry 搜索，其余配置编辑、分配、恢复、本地握手和 UI 检查仍运行。该选项不代表市场联网功能已经验证。

## 参考实现

- [Magpie 技能管理](https://github.com/yetone/magpie/blob/main/internal/library/skills.go)：区分库中技能与 Agent 自有技能，避免按名称盲目覆盖。
- [Magpie 资源库设计](https://github.com/yetone/magpie/blob/main/docs/subsystems/library.md)：外部文件保留、目录内容指纹和批量操作。这里只参考相关交互与安全边界，未移植其同步引擎。
- 图标来源及许可见 `public/agent-icons/README.md`。
