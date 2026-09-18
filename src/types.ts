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
  /** 技能自带官方图标（data URL），空表示无，用 emoji 兜底 */
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
  skills_root: string;
  agents: AgentDef[];
  agent_configs: Record<string, AgentConfig>;
  audit: AuditEntry[];
  market_sources?: MarketSource[];
  /** 用户对技能的个性化标注（收藏 / 标签），key 为技能 key */
  skill_meta?: Record<string, SkillUserMeta>;
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
