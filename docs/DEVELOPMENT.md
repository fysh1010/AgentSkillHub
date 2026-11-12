# 开发指南

面向想参与开发或自行构建的开发者。

## 1. 技术栈

| 层 | 技术 | 说明 |
|---|---|---|
| 桌面壳 | Tauri 2（Rust） | 产物 ~10MB 级，WebView2 渲染 |
| 后端 | Rust（无 async 运行时依赖，tauri 内置 tokio） | 全部业务命令在 `src-tauri/src/commands/` |
| 前端 | Vue 3 `<script setup>` + TypeScript + Vite 7 | 无 UI 组件库，纯手写样式（`src/styles/global.css`） |
| Markdown | marked | 仅用于 SKILL.md 全文渲染 |
| 数据 | JSON 配置文件 + 文件系统 | 无数据库 |

## 2. 环境准备（Windows）

1. **Node.js 22.12+** 与 npm
2. **Rust 1.77+**（`rustup` 安装，需 MSVC 工具链 + VS Build Tools）
3. **WebView2 运行时**（Win11 自带）
4. 可选：Git（技能库备份功能用）

```bash
npm install          # 前端依赖
npm run tauri dev    # 开发模式（前端热更新 + Rust 增量编译，改 Rust 自动重启）
npm run tauri build  # 发布构建 → NSIS 安装包 + 独立 exe
```

> 首次 `tauri dev` / `build` 会全量编译 Rust 依赖（几分钟），之后增量秒级。
> `cargo clean` 可释放 2GB+ 构建缓存，代价是下次全量重编。

## 3. 架构与数据流

```
Vue 组件
  │ invoke("command_name", { args })
  ▼
Tauri IPC
  │
  ▼
commands::*(mod.rs 路由) ──► scan.rs / deploy.rs / market.rs / delete.rs
  │                                    │
  ▼                                    ▼
AppState { config: Mutex<AppConfig> }  文件系统（技能库 / Agent 目录 / Junction）
```

- **同步 vs 异步命令**：涉及网络（市场、翻译）或重 I/O（扫描）的命令**必须**声明为
  `pub async fn` + `State<'_, AppState>`，网络/磁盘操作放
  `tauri::async_runtime::spawn_blocking`；同步命令跑在主线程上会**卡死窗口**。
- **配置锁纪律**：`state.config.lock()` 只允许在「取参数 / 改配置」的极短临界区内持有，
  绝不跨越网络调用。

## 4. 核心模块说明

### scan.rs — 技能扫描
- `parse_frontmatter`：解析 SKILL.md frontmatter，返回 `(name, display_name, description, version)`
  四元组；支持**YAML 块标量**（`description: >` 多行折叠）与引号包裹；
- `scan_skills`：扫描技能库根目录，`_`/`.` 开头条目跳过（`_archive`、`.git` 不参与）；
- `find_icon_b64`：探测技能目录官方图标（`_icon.*` → `icon.*` → `logo.*` → `assets/`），
  单文件上限 3MB，base64 data URL 返回；
- `dir_size_cached`：目录大小缓存，键是**目录内容指纹**（自身 + 两层内条目 mtime 之和 + 条目数），
  指纹未变直接命中，全量重扫近乎零 I/O。注意：三层以上的就地改文件仍会延迟失效（折中）；
- `read_skill_detail`：SKILL.md 全文（200KB 截断）+ 两层文件清单 + 风险识别。

### deploy.rs — 部署引擎
- Junction 操作统一走 `create_junction / remove_junction / is_junction / junction_target`；
- `compute_diff`：配置期望 vs 目录实际的差异（to_create / to_remove / keep / expand_parent_link）；
- 部署 = 执行 diff；审计条目记录每次动作明细，回滚 = 逆向执行上一次动作。

### market.rs — 市场与图标
- 三个源：`skillhub_search/skillhub_install`（腾讯）、`clawhub_search/clawhub_browse/clawhub_install`、
  `github_list/github_install`；
- HTTP 用系统 `curl.exe`、解压用系统 `tar.exe`（零额外依赖）；子进程一律经 `hidden_command`
  创建（抑制控制台黑框）；
- ClawHub 注意事项：`icon` 字段恒为 null（卡片图是发布者头像）；真实版本号在顶层 `version`
  字段（`stats.versions` 是版本总数）；默认推荐走 `/api/v1/skills?sort=downloads|stars` 的
  cursor 分页；
- 腾讯源安装后会把 `@ns/slug` 嵌套结构 `fs::rename` 展平回技能库根目录；
- `ensure_skill_icon`：把市场图标按内容判格式（PNG/GIF/JPG/SVG/WEBP 魔数）落盘为 `_icon.*`。

### delete.rs — 应用内回收站
- 删除 = `move_to_trash`（同盘 rename 到 `_archive/trash`，重名加时间戳后缀）；
- `list_trash / restore_from_trash / purge_trash`；purge 才进 Windows 回收站
  （PowerShell：VisualBasic FileIO → Shell COM 双策略，带 25s 超时防卡死）；
- `force_clear_for_update`：市场更新的旧版本移入回收站，可回滚。

### mod.rs — 命令路由与横切
- 首次启动不预注册工具，默认技能库位于当前用户的 `AgentSkillHub/skills`；旧默认路径通过 `portable_agent_path` 迁移，自定义位置优先。
- `discovery.rs` 综合程序侧证据与产品路径规则扫描本机，扫描只读；`agent_policy.rs` 统一判断已安装、允许配置和写入权限。
- `load_config`：配置加载 + 内置源迁移（自动补腾讯源、剔除下线源、腾讯源置顶）；
- `translate_text`：MyMemory 免费翻译（带 `de` 参数提额），磁盘缓存 `translations.json`，
  错误文本（MYMEMORY WARNING）识别；
- `git_backup`：`git add -A + commit`（无仓库自动 init），porcelain 判「无变更」。

### known_agents.rs — 各家产品的技能目录约定清单
- `KNOWN_AGENTS`：63 条国内外产品的「全局/用户级」skills 目录模板（`Root` 锚点 + 相对段），
  来源是各产品官方文档 + vercel-labs/skills 跨 Agent 路径表 + 本机实测；项目内目录（`.claude/skills`
  等）不收录，那是随仓库走的；
- `rel` 支持 `*` 通配段（可多段、逐层展开，单模板上限 128 个路径），用于按账号分目录的产品
  （如 `~/.meituan-catpaw/*/skills`），不进入插件缓存与安装包，
  展开后 id 追加实例名（`meituan-catpaw-anon`）以免撞注册表主键；
- `KNOWN_SOURCES`：明确的系统技能目录（WorkBuddy / Codex 的 `.system`、WPS 灵犀的 `official_skills`）。这些**不**作为可部署 Agent 注册，
  由 `refresh_discovery` 列出来源，`builtin_source_skills` 浏览，`adopt_builtin_skill` 收编副本；
  命令只认 source_id（后端按清单解析路径），前端传不进任意路径；`detect_sources` 还要求目录非空；
- `detect_existing` 只报本机存在的目录；`template_path` 用于推导路径（通配项返回 None），目录存在不直接等同于程序已安装；
- 清单未收录的新产品由 `refresh_discovery` 的通用 `~\.xxx\skills` 扫描兜底，显示名走 `name_for` 查表。

### cache.rs — 按指纹失效的缓存
- `cached(cache, key, fingerprint, compute)`：命中返回缓存值，否则算一次并写入，超过 500 条清空；
- 大小统计 / 安全报告 / token 估算三处共用，指纹由各模块自己决定
  （文件 mtime 或 `scan::dir_fingerprint` 目录内容指纹）。

## 5. 前端约定

- **类型对齐**：`src/types.ts` 必须与 `src-tauri/src/models.rs` 保持一致（serde 默认蛇形命名）；
- **全局状态**：`store.ts` —— `store`（技能/Agent/配置）、`ui.page`（跨页导航，
  App.vue watch 它）、`transMap`（翻译缓存）；
- **页面切换**：`App.vue` 用 `shallowRef` + `<Transition name="page">`，除总览外按需加载；
- **样式**：主题变量在 `src/styles/global.css`（`--accent`、`--panel`、`--radius-sm` 等），
  页面级样式 scoped；默认黑白、可切换蓝白主题，交互强调色由共享变量控制；
- **技能描述翻译**：按需请求，同一原文在途去重，前端缓存最多保留 256 条。

## 6. 测试

```bash
cd src-tauri
cargo test --lib     # 后端单元测试：frontmatter/块标量/技能库结构/base64/图标探测/目录指纹与缓存/
                     # junction 增删/回滚/导入链接/回收站 roundtrip/市场收编/清单/URL 编码等
npx vue-tsc --noEmit # 前端类型检查
```

测试一律在 `commands::test_scratch(tag)`（临时目录下带 pid 的专属目录）里造夹具，
不依赖开发者真实的技能库目录，可并发跑。

真实网络端到端测试（ignored，按需跑）：

```bash
cargo test --lib clawhub_real_install -- --ignored
```

## 7. 打包与发布

```powershell
& "node_modules/.bin/tauri.cmd" build --no-bundle --ci -- --locked
& "scripts/package-portable.ps1" -ExePath "src-tauri/target/release/agentskillhub.exe"
```

打包脚本核对项目版本、EXE 文件版本和 ZIP 内程序的 SHA-256，只生成一个 `AgentSkillHub-v<版本>-portable.zip`；包内为 `AgentSkillHub.exe`，不包含个人配置或技能文件。

发布流程：

1. 同步 `package.json`、`package-lock.json`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml` 与 Cargo 锁文件中的应用版本。
2. 添加 `docs/releases/v<版本>.md`，完成类型检查、后端测试和本地构建。
3. 提交并推送代码，再推送对应的 `v<版本>` 标签。
4. GitHub Actions 检查代码、构建并核验 ZIP，在 Releases 发布唯一的 Windows x64 免安装包；手动触发只生成 Artifact。

历史 Release 保留，新版本不再上传 NSIS 安装器。构建缓存和 `release/` 本地产物不提交到源码仓库。

## 8. 贡献注意

- 提交前跑 `cargo test --lib` 与 `npx vue-tsc --noEmit`；
- 涉及文件系统破坏性操作的命令必须有安全护栏（root 内校验 / SKILL.md 校验 / 拒绝路径穿越），
  参照 `delete.rs`；
- 不要在命令里持有配置锁做网络请求；不要新增同步的重 I/O 命令；
- UI 文案使用简体中文；代码注释密度参照现有文件（中文注释，说明「为什么」）。

## 技能共享、版本通知与目录边界

`skill_sync.rs` 实现独立技能库 Git 同步，`updates.rs` 实现正式 Release 检查与官方链接打开。前端通过 `src/api/maintenance.ts` 调用，复用既有配置写入与 IPC。自定义 Agent 的 MCP 格式选项与 MCP 页面共享，避免重复适配表。

目录核查和使用边界见 [SHARING-AND-AGENTS.md](SHARING-AND-AGENTS.md)。桌面回归运行 `node --experimental-strip-types "scripts/verify-mcp.mjs" --build`；仅验证新增入口可加 `--maintenance-only`。验证使用独立应用标识、临时配置与本地夹具，不执行真实 GitHub 推送。
