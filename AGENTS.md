# Repository Guidelines

## 项目结构与模块职责

AgentSkillHub 是基于 Vue 3、TypeScript、Vite 7 与 Tauri 2 的 Windows 桌面应用。

- `src/views/` 存放页面，`src/components/` 存放共享组件，`src/composables/` 与 `src/utils/` 存放组合函数及工具函数。
- `src/api/` 封装 Tauri IPC；`src/store.ts` 管理共享状态；`src/types.ts` 对齐 Rust 数据模型。
- `src-tauri/src/commands/` 实现扫描、部署、发现、市场及 MCP 管理；`models.rs` 定义模型，`lib.rs` 注册命令。
- Rust 单元测试位于各模块内部；`scripts/` 包含 MCP 验证、图标生成与便携版打包脚本。
- `public/app-icon.svg` 是图标源；`src-tauri/icons/` 存放生成资源；`docs/` 记录架构、设计与配置约定。

## 构建、测试与本地开发

准备 Node.js 22.12+、Rust MSVC 工具链、VS Build Tools 和 WebView2。以下命令均从仓库根目录执行：

```powershell
npm ci                         # 按锁文件安装前端依赖
npm run tauri -- dev            # 启动完整桌面开发环境
npm run dev                    # 仅启动 Vite，无法提供原生 IPC
npm run build                  # 构建前端资源
npx vue-tsc --noEmit            # 独立检查 Vue/TypeScript 类型
cargo test --manifest-path "src-tauri/Cargo.toml" --lib --locked
npm run tauri -- build --no-bundle --ci -- --locked
npm run icons                  # 从统一图标源生成资源
```

`npm run build` 不执行类型检查，提交前应单独运行 `vue-tsc`。

## 编码风格与命名

Vue 使用 `<script setup lang="ts">`，缩进两空格；Rust 缩进四空格。组件使用 PascalCase（如 `McpEditor.vue`），组合函数使用 `useXxx`，Rust 模块与函数使用 snake_case。沿用邻近代码风格，避免无关格式化；目前未配置 ESLint 或 Prettier。

IPC 字段与 serde 命名保持一致，修改模型时同步更新前后端类型。共享主题变量放在 `src/styles/global.css`，页面样式优先使用 scoped。遵循 KISS、DRY、YAGNI，抽象应服务于已有需求。

## 测试要求

后端使用 Rust 内置 `#[test]`，测试名以 snake_case 描述行为。文件系统夹具使用 `commands::test_scratch(tag)`，不得依赖真实技能库或 Agent 配置。修复缺陷时补充相应回归测试；当前没有数值覆盖率门槛。

MCP 命令解析可运行 `node --experimental-strip-types "scripts/test-mcp-command.mjs"`。桌面集成脚本 `scripts/verify-mcp.mjs` 需要独立 validation 应用标识及 WebView2 调试端口，执行前阅读脚本说明。真实网络测试默认忽略，按需显式运行。

## 提交与 Pull Request

历史提交混用中文描述及 `feat:`、`workflow:` 前缀；新增提交宜采用简短的“类型: 具体变更”。PR 应说明问题、行为变化、验证命令与结果，并关联适用 issue；界面调整附截图，配置格式变化说明兼容与恢复方式。

未经用户主动要求，不创建分支、不提交或推送；保留已有未提交改动。

## 配置与安全边界

扫描与部署不得改写技能源内容或修改不属于本工具的目录。用户主动确认的 Git 技能同步可更新独立技能库；不得强推或自动解冲突。内置技能来源只读，部署保持差异预览、干跑、回滚与审计能力。MCP 配置更新保留无关字段并检查外部修改，探测仅完成握手和 `tools/list`，不调用业务工具。

配置位于 `%APPDATA%/com.agentskillhub.app`；不要提交本机配置、凭证或审计数据。重 I/O 放入 `spawn_blocking`，配置锁只用于短临界区，不跨网络调用。

GitHub 共享、版本通知与最新目录核查见 `docs/SHARING-AND-AGENTS.md`；维护新功能时复用现有 IPC、配置写入与格式适配，不新增重复的目录扫描器。
