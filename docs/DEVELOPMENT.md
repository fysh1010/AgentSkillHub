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

1. **Node.js 18+** 与 npm
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
- `dir_size_cached`：目录大小 mtime 缓存（目录 mtime 未变直接命中），全量重扫近乎零 I/O；
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
- `load_config`：配置加载 + 内置源迁移（自动补腾讯源、剔除下线源、腾讯源置顶）；
- `translate_text`：MyMemory 免费翻译（带 `de` 参数提额），磁盘缓存 `translations.json`，
  错误文本（MYMEMORY WARNING）识别；
- `git_backup`：`git add -A + commit`（无仓库自动 init），porcelain 判「无变更」。

## 5. 前端约定

- **类型对齐**：`src/types.ts` 必须与 `src-tauri/src/models.rs` 保持一致（serde 默认蛇形命名）；
- **全局状态**：`store.ts` —— `store`（技能/Agent/配置）、`ui.page`（跨页导航，
  App.vue watch 它）、`transMap`（翻译缓存）；
- **页面切换**：`App.vue` 用 `shallowRef` + `<Transition name="page">`，无路由库；
- **样式**：主题变量在 `src/styles/global.css`（`--accent`、`--panel`、`--radius-sm` 等），
  页面级样式 scoped；「腾讯 WorkBuddy 风格」浅色主题、蓝紫渐变 accent；
- **翻译队列**：`translateMany` 逐条 180ms 限流，避免打爆免费接口。

## 6. 测试

```bash
cd src-tauri
cargo test --lib     # 21 个单元测试：frontmatter/块标量/base64/图标探测/junction 增删/
                     # 回滚/导入链接/回收站 roundtrip/市场收编/URL 编码等
npx vue-tsc --noEmit # 前端类型检查
```

真实网络端到端测试（ignored，按需跑）：

```bash
cargo test --lib clawhub_real_install -- --ignored
```

## 7. 打包与发布

```bash
npm run tauri build
# 产物：
#   src-tauri/target/release/bundle/nsis/AgentSkillHub_<ver>_x64-setup.exe   安装包
#   src-tauri/target/release/agentskillhub.exe                              便携版裸 exe
```

发布清单：

1. 版本号三处同步：`package.json` / `src-tauri/tauri.conf.json` / `src-tauri/Cargo.toml`；
2. 构建 → 组装 `release/`（安装包 + 便携 zip + 使用说明）；
3. 更新 README 与本文件的版本引用；
4. git tag `v0.2.0` 并推送，GitHub Release 上传两个产物。

## 8. 贡献注意

- 提交前跑 `cargo test --lib` 与 `npx vue-tsc --noEmit`；
- 涉及文件系统破坏性操作的命令必须有安全护栏（root 内校验 / SKILL.md 校验 / 拒绝路径穿越），
  参照 `delete.rs`；
- 不要在命令里持有配置锁做网络请求；不要新增同步的重 I/O 命令；
- UI 文案使用简体中文；代码注释密度参照现有文件（中文注释，说明「为什么」）。
