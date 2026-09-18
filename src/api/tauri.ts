import { invoke } from "@tauri-apps/api/core";
import type {
  AgentDef,
  AppConfig,
  AuditEntry,
  DeleteResult,
  DeployResult,
  DiffItem,
  SkillInfo,
  SkillDetail,
  SyncResult,
  AgentStatus,
  DetectedAgent,
  MarketSource,
  MarketSkill,
  ExternalSkill,
  TrashItem,
} from "../types";

export function getSyncData(): Promise<SyncResult> {
  return invoke("get_sync_data");
}

export function scanSkills(): Promise<SkillInfo[]> {
  return invoke("scan_skills_cmd");
}

export function setSkillsRoot(root: string): Promise<AppConfig> {
  return invoke("set_skills_root", { root });
}

export function saveAgents(agents: AgentDef[]): Promise<AppConfig> {
  return invoke("save_agents", { agents });
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

export function detectAgents(): Promise<DetectedAgent[]> {
  return invoke("detect_agents");
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

// ===== 外部技能 =====

export function listExternalSkills(agentId: string): Promise<ExternalSkill[]> {
  return invoke("list_external_skills", { agentId });
}

export function adoptSkill(agentId: string, dirName: string): Promise<string> {
  return invoke("adopt_skill", { agentId, dirName });
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
