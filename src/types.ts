// 与 Rust 后端 models.rs 对齐的类型定义

export interface SkillInfo {
  name: string;
  category: string;
  key: string;
  path: string;
  description: string;
  has_scripts: boolean;
  size_bytes: number;
  modified_ts: number;
  healthy: boolean;
  /** 技能自带图标（data URL），为空时自动生成彩色分类图标 */
  icon_b64: string;
  /** 版本号（frontmatter version / metadata.version），空表示未知 */
  version: string;
}

/** 用户对某个技能的个性化标注（收藏 / 标签） */
export interface SkillUserMeta {
  starred: boolean;
  tags: string[];
}

/** 技能目录里的一个文件条目 */
export interface FileEntry {
  name: string;
  size_bytes: number;
  is_dir: boolean;
  risky: boolean;
}

/** 技能详情（SKILL.md 全文 + 文件清单） */
export interface SkillDetail {
  key: string;
  content: string;
  files: FileEntry[];
  risks: string[];
}

/** 应用内回收站里的一条记录 */
export interface TrashItem {
  name: string;
  key: string;
  size_bytes: number;
  deleted_ts: number;
  path: string;
}

export interface AgentDef {
  id: string;
  name: string;
  skills_dir: string;
  builtin: boolean;
}

export interface DetectedAgent {
  id: string;
  name: string;
  skills_dir: string;
  /** 内置技能来源用：所属产品（同一产品的多处实例归一组）；普通 Agent 为空 */
  group?: string;
}

export interface MachineAgent {
  id: string;
  name: string;
  skills_dir: string;
  resolved_dir: string;
  path_links: { path: string; target: string; scope: "directory" | "skill" }[];
  dir_exists: boolean;
  registered: boolean;
  known: boolean;
  shared: boolean;
  confidence: "confirmed" | "configured" | "possible" | "missing";
  install_dir: string;
  evidence: { kind: string; path: string }[];
  installed: boolean;
  management_enabled: boolean;
  executable_path: string;
  custom: boolean;
}

export interface CustomBuiltinSource {
  id: string;
  tool_name: string;
  name: string;
  skills_dir: string;
}

export interface BuiltinSource {
  id: string;
  name: string;
  group: string;
  skills_dir: string;
  resolved_dir: string;
  skill_count: number;
  skill_dirs: string[];
  cache_only: boolean;
  custom?: boolean;
}

export interface MachineInventory {
  home_dir: string;
  agents: MachineAgent[];
  sources: BuiltinSource[];
  warnings: string[];
  scanned_at: number;
  catalog_count: number;
}

export interface AgentPreference {
  enabled: boolean;
  executable_path: string;
  manual_confirmed: boolean;
}

export interface MarketSource {
  id: string;
  kind: string;
  name: string;
  repo: string;
  subpath: string;
}

export interface MarketSkill {
  name: string;
  display_name: string;
  description: string;
  downloads: number;
  updated_ts: number;
  installed: boolean;
  reference: string;
  owner: string;
  icon: string;
  version: string;
  topics: string[];
  featured: boolean;
  owner_image: string;
  source_kind: string;
  security: string;
  icon_url: string;
  category: string;
  stars: number;
}

export interface MarketSkillDetail {
  description: string;
  version: string;
  changelog: string;
  owner_name: string;
  publisher_source: string;
  category: string;
  downloads: number;
  stars: number;
  installs: number;
  versions: number;
  created_ts: number;
  updated_ts: number;
  requires_api_key: boolean | null;
  pricing_type: string;
  files: { path: string; size_bytes: number }[];
  skill_md: string;
  preview_error: string;
}

export interface ExternalSkill {
  dir_name: string;
  name: string;
  description: string;
  size_bytes: number;
  healthy: boolean;
  path: string;
  /** 点开头目录（.system 等）——工具自带系统技能 */
  system: boolean;
}

export interface AgentStatus {
  agent: AgentDef;
  dir_ok: boolean;
  is_parent_link: boolean;
  enabled_count: number;
  linked_count: number;
  parent_visible: number;
  external_count: number;
  drift: boolean;
  message: string;
}

export interface DiffItem {
  kind: string;
  skill_key: string;
  link_path: string;
  current_target: string | null;
  expected_target: string | null;
}

export interface DeployAction {
  kind: string;
  skill_key: string;
  link_path: string;
  target: string;
  ok: boolean;
  message: string;
}

export interface AuditEntry {
  id: string;
  ts: number;
  action: string;
  agent_id: string;
  detail: string;
  actions: DeployAction[];
}

export interface AgentConfig {
  enabled_skills: string[];
}

export interface AppConfig {
  builtin_sources?: CustomBuiltinSource[];
  skills_root: string;
  agents: AgentDef[];
  agent_preferences: Record<string, AgentPreference>;
  agent_configs: Record<string, AgentConfig>;
  audit: AuditEntry[];
  market_sources?: MarketSource[];
  /** 用户对技能的个性化标注（收藏 / 标签），key 为技能 key */
  skill_meta?: Record<string, SkillUserMeta>;
  /** 每个 Agent 启用技能的常驻上下文预算（token 数） */
  token_budget?: number;
}

export interface SyncResult {
  skills: SkillInfo[];
  agents: AgentStatus[];
  config: AppConfig;
}

export interface DeployResult {
  ok: boolean;
  actions: DeployAction[];
  summary: string;
}

export interface DeleteResult {
  skill_key: string;
  path: string;
  recycled: boolean;
  removed_links: DeployAction[];
  unassigned_agents: string[];
  warnings: string[];
  message: string;
}

// ================= 安全扫描 =================

/** 一条内容级安全命中 */
export interface SecurityFinding {
  file: string;
  /** 行号；文件级命中为 0 */
  line: number;
  category: string;
  level: string;
  pattern: string;
  snippet: string;
}

/** 单技能安全扫描报告 */
export interface SecurityReport {
  skill_key: string;
  level: string;
  findings: SecurityFinding[];
  scanned_files: number;
}

/** 技能列表 badge 用的轻量汇总 */
export interface SecuritySummaryItem {
  skill_key: string;
  level: string;
  high: number;
  medium: number;
  low: number;
}

// ================= Token 上下文预算 =================

/** 单个技能的 token 估算 */
export interface SkillTokens {
  skill_key: string;
  name: string;
  resident_tokens: number;
  body_tokens: number;
}

/** 单个 Agent 的 token 统计 */
export interface AgentTokenStats {
  agent_id: string;
  agent_name: string;
  enabled_count: number;
  resident_tokens: number;
  body_tokens: number;
  budget: number;
  over_budget: boolean;
  skills: SkillTokens[];
}

// ================= 环境清单导出/导入 =================

/** 清单导入结果 */
export interface ManifestImportResult {
  ok: boolean;
  message: string;
  missing_skills: number;
}
