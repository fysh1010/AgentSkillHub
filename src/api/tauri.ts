import { invoke } from "@tauri-apps/api/core";
import type {
  AgentDef,
  AppConfig,
  DeleteResult,
  DeployResult,
  DiffItem,
  SkillDetail,
  SyncResult,
  AgentStatus,
  MarketSource,
  MarketSkill,
  MarketSkillDetail,
  ExternalSkill,
  TrashItem,
  SecurityReport,
  SecuritySummaryItem,
  AgentTokenStats,
  ManifestImportResult,
  MachineInventory,
  CustomBuiltinSource,
  SkillAssignment,
} from "../types";

export function getSyncData(): Promise<SyncResult> {
  return invoke("get_sync_data");
}

export function workspaceStamp(): Promise<string> {
  return invoke("workspace_stamp");
}

export function applySkillAssignments(changes: SkillAssignment[], dryRun = false): Promise<DeployResult> {
  return invoke("apply_skill_assignments", { changes, dryRun });
}

export function adoptAgentSkill(agentId: string, skillKey: string): Promise<string> {
  return invoke("adopt_agent_skill", { agentId, skillKey });
}

export function setSkillsRoot(root: string): Promise<AppConfig> {
  return invoke("set_skills_root", { root });
}



export function setAgentManagement(agentId: string, enabled: boolean): Promise<AppConfig> {
  return invoke("set_agent_management", { agentId, enabled });
}

export function saveCustomAgent(agent: AgentDef, executablePath: string, enabled: boolean): Promise<AppConfig> {
  return invoke("save_custom_agent", { agent, executablePath, enabled });
}

export function toggleSkill(agentId: string, skillKey: string, enabled: boolean): Promise<AppConfig> {
  return invoke("toggle_skill", { agentId, skillKey, enabled });
}

export function computeDiff(agentId: string): Promise<DiffItem[]> {
  return invoke("compute_diff_cmd", { agentId });
}

export function deployAgent(agentId: string, dryRun: boolean): Promise<DeployResult> {
  return invoke("deploy_cmd", { agentId, dryRun });
}

export function rollbackAgent(agentId: string): Promise<DeployResult> {
  return invoke("rollback_cmd", { agentId });
}

export function clearAudit(): Promise<void> {
  return invoke("clear_audit");
}

/** 从技能库删除技能（移入应用内回收站） */
export function deleteSkill(key: string): Promise<DeleteResult> {
  return invoke("delete_skill", { key });
}

// ===== 详情 / 回收站 / 收藏标签 / Git 备份 =====

export function readSkillDetail(key: string): Promise<SkillDetail> {
  return invoke("read_skill_detail", { key });
}

export function listTrash(): Promise<TrashItem[]> {
  return invoke("list_trash");
}

export function restoreTrash(name: string): Promise<string> {
  return invoke("restore_trash", { name });
}

export function purgeTrash(name: string): Promise<void> {
  return invoke("purge_trash", { name });
}

export function setSkillMeta(key: string, starred: boolean, tags: string[]): Promise<AppConfig> {
  return invoke("set_skill_meta", { key, starred, tags });
}

export function gitBackup(message: string): Promise<string> {
  return invoke("git_backup", { message });
}

export function refreshStatus(): Promise<AgentStatus[]> {
  return invoke("refresh_status");
}

export function importExistingLinks(agentId: string): Promise<AppConfig> {
  return invoke("import_existing_links", { agentId });
}

export function refreshDiscovery(): Promise<MachineInventory> {
  return invoke("refresh_discovery");
}

// ===== 技能市场 =====

export function marketSources(): Promise<MarketSource[]> {
  return invoke("market_sources");
}

export function marketAddSource(name: string, repo: string, subpath: string): Promise<AppConfig> {
  return invoke("market_add_source", { name, repo, subpath });
}

export function marketRemoveSource(id: string): Promise<AppConfig> {
  return invoke("market_remove_source", { id });
}

export function marketList(sourceId: string, query: string, page = 1): Promise<MarketSkill[]> {
  return invoke("market_list_ex", { sourceId, query, page });
}

export function marketSkillDetail(slug: string): Promise<MarketSkillDetail> {
  return invoke("market_skill_detail", { slug });
}

export function marketInstall(
  sourceId: string,
  reference: string,
  owner: string,
  iconUrl = "",
  force = false
): Promise<string> {
  return invoke("market_install", { sourceId, reference, owner, iconUrl, force });
}

/** 给已安装技能补官方图标（下载落盘为 _icon.*） */
export function ensureSkillIcon(slug: string, iconUrl: string): Promise<string> {
  return invoke("ensure_skill_icon", { slug, iconUrl });
}

// ===== 工具内置技能来源（只读）=====

export function saveBuiltinSource(source: CustomBuiltinSource): Promise<AppConfig> {
  return invoke("save_builtin_source", { source });
}

export function removeBuiltinSource(id: string): Promise<AppConfig> {
  return invoke("remove_builtin_source", { id });
}

export function builtinSourceSkills(sourceId: string): Promise<ExternalSkill[]> {
  return invoke("builtin_source_skills", { sourceId });
}

export function adoptBuiltinSkill(sourceId: string, dirName: string): Promise<string> {
  return invoke("adopt_builtin_skill", { sourceId, dirName });
}

// ===== 安全扫描 =====

export function scanSkillSecurity(key: string): Promise<SecurityReport> {
  return invoke("scan_skill_security", { key });
}

export function scanSecuritySummary(): Promise<SecuritySummaryItem[]> {
  return invoke("scan_security_summary");
}

// ===== Token 上下文预算 =====

export function tokenStats(): Promise<AgentTokenStats[]> {
  return invoke("token_stats");
}

export function setTokenBudget(budget: number): Promise<AppConfig> {
  return invoke("set_token_budget", { budget });
}

// ===== 环境清单导出/导入 =====

export function exportManifest(): Promise<string> {
  return invoke("export_manifest");
}

export function importManifest(path: string): Promise<ManifestImportResult> {
  return invoke("import_manifest", { path });
}

export function latestManifest(): Promise<string | null> {
  return invoke("latest_manifest");
}

export function revealPath(path: string): Promise<void> {
  return invoke("reveal_path", { path });
}

// ===== 翻译 =====

export function translateText(text: string): Promise<string> {
  return invoke("translate_text", { text });
}

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

export function formatTs(ts: number): string {
  const d = new Date(ts * 1000);
  const p = (x: number) => String(x).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())} ${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}
