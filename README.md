<div align="center">

# 🧩 AgentSkillHub

**AI Agent 技能统一管理台**

一套技能库 · 多个 AI 工具共享 · 可视化配置 · 一键部署

`Tauri 2` · `Rust` · `Vue 3` · `Windows`

</div>

---

## 这是什么

如果你同时在用 **Claude Code、OpenAI Codex、Zcode、WorkBuddy、豆包、Qwen Code、Trae** 等多个 AI Agent 工具，你一定遇到过这些问题：

- 每个工具的技能（Skills）目录各自为政，装一份要复制 N 份
- 想给不同工具配不同技能，得手动建链接、删链接，路径记不住
- 技能装多了不知道哪些在哪用着、哪些已经没用了
- 社区市场（ClawHub / 腾讯 SkillHub）的技能想要一键安装、统一管理

**AgentSkillHub 用「唯一技能库 + 按需软链接」的思路一次性解决：**

```
技能库 E:\my-skills\<技能名>\        ← 唯一真实源，永不被改动
        │
        │ 逐技能 Junction 链接（mklink /J，无需管理员权限）
        ▼
C:\Users\xxx\.claude\skills\xxx      → E:\my-skills\xxx
C:\Users\xxx\.codex\skills\xxx      → E:\my-skills\xxx
C:\Users\xxx\.zcode\skills\xxx      → E:\my-skills\xxx
        （给哪个工具配了哪个技能，就只在哪个目录建链接）
```

源文件永远只有一份；删除链接不影响源；改一处所有工具即时生效。

## 功能总览

### 技能共享与目录整改
- 设置中可指定自己的 GitHub 技能仓库与分支，手动推送 / 拉取，多台电脑共享技能；不上传应用设置与 MCP 凭证，冲突或分叉时停止。
- 启动检查正式版本并通知，提供官方发布说明与下载入口；支持关闭自动检查。
- 自动发现仅枚举明确的用户技能和系统技能目录，不深搜插件缓存、安装包与市场内部资源。
- 自定义 Agent 可在同一处填写 Skills 目录及 MCP 文件和格式；配置立即保存，目标工具的重载 / 重启限制明确展示。
- [核查依据、使用方式与边界](docs/SHARING-AND-AGENTS.md)。

### MCP 集成管理（v0.6.0）
- 重新设计「我的 MCP / 按 Agent / 发现与导入 / MCP 市场 / 配置指南」，默认聚焦用户自己配置的服务，插件内部连接器不自动深搜。
- 同一弹窗填写完整命令、环境变量、HTTP / SSE 地址、检测超时并选择 Agent，支持完整 JSON 批量导入；一份服务分配多个 Agent，并按客户端原生格式写入。
- 保存后自动连接 MCP、完成握手并发现工具，显示工具列表和具体失败原因；支持保存前检测、重新检测、停止和批量检测。检测不执行业务工具，目标 Agent 的独立授权仍由其处理。
- 直接编辑已有用户服务，支持暂停、重新启用、移除和恢复原始配置；管理开关、同名冲突、外部修改保护继续生效。
- 逐项核查全部已知 Agent 的配置入口、文件位置和格式，附来源依据；对未验证 schema 的产品提供指南，避免错误写入。适配差异包含 Cline CLI、OpenCode、Antigravity、Copilot、Posit 等。
- [MCP 使用说明](docs/MCP.md) · [逐项配置核查](docs/MCP-AGENTS.md)。

### 本机工具识别与配置开关
- 启动自动发现已知 Agent，综合配置目录、PATH、Windows 安装登记与运行进程展示识别证据；支持尚未创建技能目录的工具。
- 路径跟随当前用户与应用环境变量解析，修复旧默认用户名和豆包失效默认路径，同时保留自定义位置。
- 「设置 → Agent 工具」只展示本机已安装工具，未安装规则另列；每个工具可独立开关配置。「设置 → 内置技能」展示 WorkBuddy、豆包等工具用户安装、自定义和明确系统技能的来源位置、实际路径和技能清单。
- 界面支持黑白 / 蓝白配色，统一浅灰导航、白色内容区与线性图标；[设计参考](docs/DESIGN.md)与[识别策略](docs/DISCOVERY.md)均有说明。

### 📚 技能库管理
- 自动扫描技能库目录，解析每个技能的 SKILL.md（名称 / 描述 / 版本 / 健康状态）
- **技能图标**：优先显示技能自带图标；从市场安装的技能会把官方图标落盘到技能目录。无图的自建技能自动显示稳定配色的分类图标，图片加载失败也会自动回退
- **中文优先显示**：识别 frontmatter `display_name`，中文技能名直接显示，slug 作副标题
- **英文描述自动翻译**：内置免费翻译（MyMemory，无需密钥），双层缓存（内存 + 磁盘），只翻一次
- **收藏与标签**：★ 一键收藏、自定义标签筛选
- **全文预览**：SKILL.md 渲染成 Markdown + 文件清单，含 `scripts/`、`.py/.sh/.exe` 等可执行文件的**安全高亮**
- **内容级安全扫描**：对 SKILL.md 与脚本做危险模式检测（进程执行 / 网络外联 / 凭证读取 / 编码混淆），
  分级风险报告 + 列表卡片风险徽标；市场安装后启用前自动扫描
- **应用内回收站**：删除先进回收站（同盘秒移），可一键还原；「彻底删除」才进 Windows 回收站

### 🤖 多 Agent 配置与部署
- 内置 63 条产品路径规则，以本机安装证据区分已安装与待确认工具；支持自定义 Agent 和路径。
- 每个工具独立控制「允许配置」，关闭后保留原链接和技能分配，扫描和重启不会自动重新开启。
- **工具技能**：只读浏览明确的用户技能目录与系统技能目录，支持手动添加来源，
  按需「收编」副本进技能库共享给其他 Agent —— 系统技能来源保持只读，用户技能目录可通过 Agent 配置部署受管链接
- 对每个 Agent 独立勾选启用哪些技能
- 部署前**差异预览**（新增 / 移除 / 迁移），支持**干跑**（dry-run）
- 一键**回滚**最近一次部署；全程审计日志
- 自动识别「整目录链接模式」并支持迁移为逐技能链接
- 总览红绿灯：就绪 / 待部署 / 异常 一目了然；「N 个技能未分配」提醒
- **上下文预算**：估算每个 Agent 已启用技能的常驻 token 开销，进度条展示，超预算预警（预算值可在设置中调整）

### 🛒 技能市场
- **腾讯 SkillHub**（官方接口，中英双语描述，每页 60 条精选）
- **点开看详情**：点击技能卡片可查看完整介绍、作者、版本、下载与收藏；腾讯源还可预览公开的 SKILL.md 和文件清单，再决定是否安装
- **ClawHub 社区市场**（关键词搜索 + 默认推荐 feed）
- **任意 GitHub 仓库**作为技能源（如 `anthropics/skills`）
- 推荐 feed **按分类分组**展示，组内热门优先
- **一键安装**即装即用：自动落盘到技能库根目录 + 下载官方图标 + 弹窗勾选 Agent 直接启用部署
- **更新检测**：市场有新版自动标「可更新」，一键更新（旧版本先进回收站，可回滚）
- 安装前风险提示：含可执行脚本的技能有黄色警告，安装后自动内容扫描

### 🛡 安全与备份
- 删除 / 部署全部有审计日志
- 技能库 **Git 快照备份**：一键 `add + commit`（无仓库自动 init），删错随时回滚
- **环境清单导出/导入**：把 Agent 分配、收藏标签导出为单个 JSON，换机 / 重装后一键恢复
  （导入按 Agent 覆盖分配关系，缺失技能显示为「已失效」，不含技能文件本体）
- 纯本地运行，不收集任何数据；网络用于市场浏览 / 下载、免费翻译、GitHub 技能同步及版本检查

## 安装

### 官方下载（Windows x64，免安装）

前往 [最新 Release](https://github.com/fysh1010/AgentSkillHub/releases/latest) 下载 `AgentSkillHub-v0.6.0-portable.zip`，解压后双击 `AgentSkillHub.exe` 即可运行。

从 v0.3.0 开始，下载区只发布一个免安装 ZIP；旧版本安装包保留在历史 Release 中。

> 需要 Microsoft Edge WebView2 运行时。Windows 11 通常已具备；Windows 10 若缺少，请先安装运行时。
>
> 配置保存在 `%APPDATA%/com.agentskillhub.app`，更新程序会沿用现有配置与技能库。
>
> **macOS / Linux 用户**：部署机制针对 Windows Junction 设计，暂不提供官方跨平台包，参见 [开发指南](docs/DEVELOPMENT.md)。

### 方式三：从源码构建

```bash
git clone https://github.com/fysh1010/AgentSkillHub.git
cd AgentSkillHub
npm install
npm run tauri -- build --no-bundle    # 产物为 src-tauri/target/release/agentskillhub.exe
npm run tauri dev      # 开发模式
```

依赖：Node.js 22.12+、Rust 1.77+、WebView2（Windows）。

## 快速上手

1. **首次启动**：进入「设置」确认技能库根目录（默认当前用户目录下的 `AgentSkillHub/skills`，可改）与 Agent 列表
2. **配技能**：到「Agent 配置」左侧选工具，右侧勾选技能 → 点「部署」→ 查看差异 → 确认
3. **装新技能**：到「技能市场」搜索或浏览分类推荐 → 安装 → 在弹窗里勾选要启用的 Agent → 完成
4. **日常维护**：总览看健康状态；技能库可收藏、打标签、看全文、删技能（可还原）

## 项目结构

```
AgentSkillHub/
├── src/                     # Vue 3 前端
│   ├── views/
│   │   ├── Dashboard.vue    # 总览：状态红绿灯、未分配提醒、外部技能收编
│   │   ├── Skills.vue       # 技能库：卡片/详情/全文预览/收藏标签/回收站
│   │   ├── AgentConfig.vue  # Agent 配置：勾选、差异预览、部署、回滚
│   │   ├── Market.vue       # 技能市场：多源、分组推荐、更新检测
│   │   ├── AuditView.vue    # 部署记录与审计
│   │   └── Settings.vue     # 设置：技能库路径、Agent 管理、Git 备份
│   ├── api/tauri.ts         # 后端命令封装
│   ├── store.ts             # 全局状态（含翻译缓存/导航）
│   └── types.ts             # 与 Rust models 对齐的类型
└── src-tauri/               # Rust 后端
    └── src/
        ├── lib.rs           # Tauri 入口与命令注册
        ├── models.rs        # 数据模型
        └── commands/
            ├── mod.rs       # 配置/命令路由/翻译/Git 备份
            ├── scan.rs      # 技能扫描（frontmatter/块标量/图标/详情）
            ├── deploy.rs    # Junction 部署/差异/回滚/审计
            ├── market.rs    # 市场（腾讯/ClawHub/GitHub）/图标落盘
            └── delete.rs    # 应用内回收站/还原/彻底删除
```

## 设计原则

- **源文件不可变**：工具对技能库只做「整目录增删」，从不改写技能内容
- **最小侵入**：Agent 目录里不是本工具创建的链接 / 目录一概不碰（`.system` 等点开头条目完全不感知）
- **可逆优先**：部署可干跑、可回滚；删除进应用回收站；更新前旧版自动备份
- **零服务依赖**：数据全部本地 JSON + 文件系统，卸载不残留

## 常见问题

<details>
<summary><b>Junction 链接和快捷方式有什么区别？为什么用它？</b></summary>

Junction 是文件系统层的目录别名（`mklink /J`），对程序完全透明——AI 工具读取 `skills/xxx` 就等于读取 `E:\my-skills\xxx`。快捷方式只是 Shell 层对象，程序无法跟随。且 Junction 不需要管理员权限（符号链接 symlink 需要）。
</details>

<details>
<summary><b>更新技能 / 删除技能会把源文件弄丢吗？</b></summary>

不会。更新时旧版本先移入应用内回收站（`_archive/trash`），新版本装好即用；删除也是先入回收站，可一键还原。两者都不会硬删除。
</details>

<details>
<summary><b>技能描述翻译用的是 gì 服务？会泄露内容吗？</b></summary>

使用 MyMemory 免费翻译接口（无需密钥），只发送技能的描述文本。结果缓存到本地 `translations.json`，同一描述只请求一次。不想联网可在防火墙屏蔽该域名，应用仅显示原文。
</details>

<details>
<summary><b>WorkBuddy / 豆包这类工具能用链接模式吗？</b></summary>

WorkBuddy 目录为真实副本（其加载器不完全遵循 Junction），豆包已实测链接可用。应用对实体副本目录的态度是「看见但不碰」，还提供外部技能一键收编进技能库。
</details>

<details>
<summary><b>支持 macOS / Linux 吗？</b></summary>

代码层面 Tauri 是跨平台的，但当前针对 Windows 设计（Junction 链接、回收站脚本、路径约定）。跨平台支持在路线图中；macOS 可用 symlink 替代 Junction。
</details>

## 路线图

- [x] Windows 官方 CI 构建（安装包 + 便携版，推 tag 自动发布）
- [x] 内容级安全扫描（进程执行 / 网络 / 凭证 / 混淆四类模式）
- [x] 环境清单导出 / 导入（换机恢复）
- [x] 上下文预算（每 Agent token 开销估算与预警）
- [ ] macOS / Linux 的 symlink 链接模式适配（当前部署逻辑针对 Windows Junction）
- [ ] 技能市场收藏与安装历史
- [ ] 定时自动 Git 备份
- [ ] 技能冲突智能合并
- [ ] 深色主题

## Star 曲线

<div align="center">

[![Star History Chart](https://api.star-history.com/svg?repos=fysh1010/AgentSkillHub&type=Date)](https://star-history.com/#fysh1010/AgentSkillHub&Date)

</div>

## 许可证

[MIT](LICENSE)

---

<div align="center">

**AgentSkillHub** · 让每个 AI 工具都用上同一套好技能

</div>
