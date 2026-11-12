import { reactive } from "vue";
import * as api from "./api/tauri";
import type {
  AppConfig,
  SkillInfo,
  AgentStatus,
  DeployResult,
  DeleteResult,
  AgentTokenStats,
  MachineInventory,
  AgentOwnedSkill,
  SkillAssignment,
} from "./types";

// ---------- 英文描述翻译（全局缓存，后端另有一份磁盘缓存） ----------

/** 原文 → 译文 的内存缓存（响应式，译完卡片即刷新） */
export const transMap = reactive<Record<string, string>>(Object.create(null));
const inFlight = new Set<string>();
const MAX_TRANSLATIONS = 256;

/** 中文占比低于 30% 视为需要翻译 */
export function needsTranslation(text: string): boolean {
  if (!text || text.length < 8) return false;
  const cjk = (text.match(/[一-鿿]/g) || []).length;
  return cjk < text.length * 0.3;
}

/** 取译文（已译返回译文；否则后台发起翻译，返回当前最佳展示文本） */
export function ensureTranslate(text: string): string {
  if (!needsTranslation(text)) return text;
  if (transMap[text]) return transMap[text];
  if (!inFlight.has(text)) {
    inFlight.add(text);
    void api.translateText(text).then((translation) => {
      if (!translation) return;
      const keys = Object.keys(transMap);
      if (keys.length >= MAX_TRANSLATIONS) delete transMap[keys[0]];
      transMap[text] = translation;
    }).catch(() => {}).finally(() => inFlight.delete(text));
  }
  return text; // 译好前先显示原文
}

export interface ToastItem {
  id: number;
  type: "success" | "error" | "info";
  text: string;
}

export const store = reactive({
  skills: [] as SkillInfo[],
  agents: [] as AgentStatus[],
  config: null as AppConfig | null,
  loading: false,
  ownedSkills: [] as AgentOwnedSkill[],
  assigning: false,
  revision: 0,
  toasts: [] as ToastItem[],
});

/** 全局 UI 状态（跨页面跳转用） */
export const ui = reactive({
  /** 当前页：dashboard / skills / market / agents / audit / settings */
  page: "dashboard",
  settingsTab: "agents",
  skillsView: "list" as "list" | "cards" | "agents",
});

export function openAgentSettings() {
  ui.settingsTab = "agents";
  ui.page = "settings";
}

export function openSkillAllocation() {
  ui.skillsView = "agents";
  ui.page = "skills";
}

export const discovery = reactive({
  inventory: null as MachineInventory | null,
  loading: false,
  error: "",
});

let discoveryRequest: Promise<void> | null = null;
export function discoverMachine(refreshSkills = true): Promise<void> {
  if (discoveryRequest) return discoveryRequest;
  discoveryRequest = runDiscovery(refreshSkills).finally(() => { discoveryRequest = null; });
  return discoveryRequest;
}

async function runDiscovery(refreshSkills: boolean): Promise<void> {
  discovery.loading = true;
  discovery.error = "";
  try {
    discovery.inventory = await api.refreshDiscovery();
    if (refreshSkills || !store.config) {
      await loadAll();
    } else {
      const request = syncRequest;
      const agents = await api.refreshStatus();
      if (request === syncRequest) store.agents = agents;
    }
  } catch (e) {
    discovery.error = String(e);
    toast(`本机发现失败：${e}`, "error");
  } finally {
    discovery.loading = false;
  }
}

/** 技能 key → 安全级别（high/medium/low/none），后台扫描汇总填充 */
export const secLevels = reactive<Record<string, string>>({});

export async function loadSecuritySummary(): Promise<void> {
  try {
    const items = await api.scanSecuritySummary();
    for (const k of Object.keys(secLevels)) delete secLevels[k];
    for (const it of items) secLevels[it.skill_key] = it.level;
  } catch (e) {
    console.warn("安全扫描汇总失败", e);
  }
}

/** agentId → token 统计 */
export const tokenStats = reactive<Record<string, AgentTokenStats>>({});

export async function refreshTokenStats(): Promise<void> {
  try {
    const list = await api.tokenStats();
    for (const k of Object.keys(tokenStats)) delete tokenStats[k];
    for (const s of list) tokenStats[s.agent_id] = s;
  } catch (e) {
    console.warn("token 统计失败", e);
  }
}

/** 某技能的用户标注（收藏/标签），无记录时返回默认值 */
export function skillMeta(key: string): { starred: boolean; tags: string[] } {
  const m = store.config?.skill_meta?.[key];
  return { starred: m?.starred ?? false, tags: m?.tags ?? [] };
}

/** 更新收藏/标签（写后端持久化） */
export async function updateSkillMeta(
  key: string,
  starred: boolean,
  tags: string[]
): Promise<void> {
  try {
    store.config = await api.setSkillMeta(key, starred, tags);
  } catch (e) {
    toast(String(e), "error");
  }
}

let toastId = 0;
export function toast(text: string, type: ToastItem["type"] = "info") {
  const id = ++toastId;
  store.toasts.push({ id, type, text });
  setTimeout(() => removeToast(id), 3600);
}

export function removeToast(id: number) {
  const i = store.toasts.findIndex((t) => t.id === id);
  if (i >= 0) store.toasts.splice(i, 1);
}

let syncRequest = 0;
export async function loadAll(silent = false): Promise<void> {
  const request = ++syncRequest;
  if (!silent) store.loading = true;
  try {
    const data = await api.getSyncData();
    if (request !== syncRequest) return;
    store.skills = data.skills;
    store.agents = data.agents;
    store.config = data.config;
    const managed = new Set(data.agents.map(a => a.agent.id));
    store.ownedSkills = (data.owned_skills || []).filter(s => managed.has(s.agent_id));
    store.revision++;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    if (request === syncRequest) store.loading = false;
  }
}

export function isEnabled(agentId: string, skillKey: string): boolean {
  return store.config?.agent_configs[agentId]?.enabled_skills.includes(skillKey) ?? false;
}

export function isAssigned(agentId: string, skillKey: string): boolean {
  return store.agents.find(a => a.agent.id === agentId)?.assigned_skills?.includes(skillKey) ?? false;
}

export function hasExternalSkill(agentId: string, skillKey: string): boolean {
  return store.ownedSkills.some(s => s.agent_id === agentId && s.library_key === skillKey);
}

/** 所有即时开关复用同一个批次入口，后端只落盘指定技能。 */
export async function assignSkills(changes: SkillAssignment[]): Promise<boolean> {
  if (store.assigning || !changes.length) return false;
  store.assigning = true;
  ++syncRequest; // 丢弃写入前启动的后台读取。
  try {
    const result = await api.applySkillAssignments(changes);
    toast(result.summary, "success");
    return true;
  } catch (e) {
    toast(String(e), "error");
    return false;
  } finally {
    await loadAll(true);
    store.assigning = false;
  }
}

export async function toggleSkill(agentId: string, skillKey: string, enabled: boolean) {
  try {
    store.config = await api.toggleSkill(agentId, skillKey, enabled);
  } catch (e) {
    toast(String(e), "error");
  }
}

export async function deploy(agentId: string, dryRun: boolean): Promise<DeployResult | null> {
  try {
    const r = await api.deployAgent(agentId, dryRun);
    await loadAll();
    return r;
  } catch (e) {
    toast(String(e), "error");
    return null;
  }
}

export async function rollback(agentId: string): Promise<DeployResult | null> {
  try {
    const r = await api.rollbackAgent(agentId);
    await loadAll();
    return r;
  } catch (e) {
    toast(String(e), "error");
    return null;
  }
}

/** 从技能库删除技能（移入回收站）；成功后自动清理链接与配置并刷新 */
export async function deleteSkill(key: string): Promise<DeleteResult | null> {
  try {
    const r = await api.deleteSkill(key);
    await loadAll();
    return r;
  } catch (e) {
    toast(String(e), "error");
    return null;
  }
}
