import { reactive } from "vue";
import * as api from "./api/tauri";
import type { AppConfig, SkillInfo, AgentStatus, DeployResult, DeleteResult } from "./types";

// ---------- 英文描述翻译（全局缓存，后端另有一份磁盘缓存） ----------

/** 原文 → 译文 的内存缓存（响应式，译完卡片即刷新） */
export const transMap = reactive<Record<string, string>>({});
const inFlight = new Map<string, Promise<string>>();

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
    const p = api
      .translateText(text)
      .then((t) => {
        if (t) transMap[text] = t;
        return t;
      })
      .catch(() => "")
      .finally(() => inFlight.delete(text));
    inFlight.set(text, p);
  }
  return text; // 译好前先显示原文
}

/** 手动触发一条翻译（用于「译」按钮） */
export async function translateNow(text: string): Promise<void> {
  await ensureTranslate(text);
}

// ---- 批量翻译：限流队列，避免瞬时并发打爆免费翻译接口 ----

const pendingQueue: string[] = [];
let draining = false;

async function drainQueue() {
  if (draining) return;
  draining = true;
  while (pendingQueue.length) {
    const t = pendingQueue.shift()!;
    ensureTranslate(t);
    // 免费接口限流：每条之间留点间隔
    await new Promise((r) => setTimeout(r, 180));
  }
  draining = false;
}

/** 批量翻译（自动去重、跳过已译），逐条限流出队 */
export function translateMany(texts: string[]) {
  for (const t of texts) {
    if (!needsTranslation(t)) continue;
    if (transMap[t] || inFlight.has(t) || pendingQueue.includes(t)) continue;
    pendingQueue.push(t);
  }
  drainQueue();
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
  toasts: [] as ToastItem[],
});

/** 全局 UI 状态（跨页面跳转用） */
export const ui = reactive({
  /** 当前页：dashboard / skills / market / agents / audit / settings */
  page: "dashboard",
});

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
  setTimeout(() => {
    const i = store.toasts.findIndex((t) => t.id === id);
    if (i >= 0) store.toasts.splice(i, 1);
  }, 3600);
}

export async function loadAll(): Promise<void> {
  store.loading = true;
  try {
    const data = await api.getSyncData();
    store.skills = data.skills;
    store.agents = data.agents;
    store.config = data.config;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    store.loading = false;
  }
}

export function findAgent(id: string): AgentStatus | undefined {
  return store.agents.find((a) => a.agent.id === id);
}

export function agentConfig(id: string) {
  return store.config?.agent_configs[id];
}

export function isEnabled(agentId: string, skillKey: string): boolean {
  return store.config?.agent_configs[agentId]?.enabled_skills.includes(skillKey) ?? false;
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
