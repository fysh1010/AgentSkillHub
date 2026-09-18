<script setup lang="ts">
import { computed, ref, watch } from "vue";
import {
  store,
  isEnabled,
  toggleSkill,
  deploy,
  rollback,
  toast,
  loadAll,
  ensureTranslate,
  needsTranslation,
  transMap,
  translateMany,
} from "../store";
import { computeDiff, importExistingLinks } from "../api/tauri";
import type { DiffItem } from "../types";

const selectedId = ref("");
const diff = ref<DiffItem[]>([]);
const diffLoading = ref(false);
const busy = ref(false);
const showPreview = ref(false);
const lastResult = ref<{ ok: boolean; summary: string } | null>(null);

const agents = computed(() => store.agents);
const current = computed(() => agents.value.find((a) => a.agent.id === selectedId.value) || null);

// 技能按分类分组
const grouped = computed(() => {
  const map = new Map<string, typeof store.skills>();
  store.skills.forEach((s) => {
    if (!map.has(s.category)) map.set(s.category, []);
    map.get(s.category)!.push(s);
  });
  const arr = Array.from(map.entries());
  arr.sort((a, b) => {
    const ap = a[0].startsWith(".") || a[0].startsWith("_") ? 1 : 0;
    const bp = b[0].startsWith(".") || b[0].startsWith("_") ? 1 : 0;
    if (ap !== bp) return ap - bp;
    return a[0].localeCompare(b[0], "zh");
  });
  return arr;
});

const diffSummary = computed(() => {
  const create = diff.value.filter((d) => d.kind === "to_create" || d.kind === "to_replace").length;
  const remove = diff.value.filter((d) => d.kind === "to_remove").length;
  const expand = diff.value.filter((d) => d.kind === "expand_parent_link").length;
  const parts: string[] = [];
  if (create) parts.push(`新增 ${create}`);
  if (remove) parts.push(`移除 ${remove}`);
  if (expand) parts.push(`迁移整目录链接`);
  return parts.length ? parts.join(" · ") : "已同步";
});

// ---------- 英文描述翻译 ----------

function showDesc(d: string): string {
  if (!needsTranslation(d)) return d || "—";
  return transMap[d] || d;
}

watch(selectedId, async (id) => {
  if (!id) return;
  await refreshDiff(id);
  // 选中 Agent 后自动把英文描述排队翻译（限流逐条，译完即显示）
  translateMany(store.skills.map((s) => s.description));
});

function selectAgent(id: string) {
  selectedId.value = id;
}

async function refreshDiff(agentId: string = selectedId.value) {
  if (!agentId) return;
  diffLoading.value = true;
  try {
    diff.value = await computeDiff(agentId);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    diffLoading.value = false;
  }
}

async function doDeploy(dryRun: boolean) {
  if (!current.value) return;
  busy.value = true;
  lastResult.value = null;
  try {
    const r = await deploy(current.value.agent.id, dryRun);
    if (r) lastResult.value = { ok: r.ok, summary: r.summary };
    showPreview.value = false;
  } finally {
    busy.value = false;
    await refreshDiff();
  }
}

async function doRollback() {
  if (!current.value) return;
  busy.value = true;
  try {
    const r = await rollback(current.value.agent.id);
    if (r) lastResult.value = { ok: r.ok, summary: r.summary };
  } finally {
    busy.value = false;
    await refreshDiff();
  }
}

async function doImportExisting() {
  if (!current.value) return;
  busy.value = true;
  try {
    const cfg = await importExistingLinks(current.value.agent.id);
    store.config = cfg;
    await loadAll();
    const added = store.config?.agent_configs[current.value.agent.id]?.enabled_skills.length ?? 0;
    lastResult.value = {
      ok: true,
      summary: `已导入当前可见的 ${added} 个技能为配置，可在下方按需增删，然后部署迁移`,
    };
    toast("已导入当前链接为配置", "success");
  } catch (e) {
    toast(String(e), "error");
  } finally {
    busy.value = false;
    await refreshDiff();
  }
}

async function onToggle(skillKey: string, enabled: boolean) {
  if (!current.value) return;
  await toggleSkill(current.value.agent.id, skillKey, enabled);
  await refreshDiff();
}

function diffLabel(kind: string): string {
  const map: Record<string, string> = {
    to_create: "待创建",
    to_remove: "待移除",
    to_replace: "待替换",
    keep: "保持",
    external: "外部保留",
    conflict: "冲突",
    expand_parent_link: "迁移整目录",
    skip: "跳过",
  };
  return map[kind] || kind;
}

function diffTone(kind: string): string {
  if (kind === "to_create" || kind === "to_replace") return "blue";
  if (kind === "to_remove") return "red";
  if (kind === "external" || kind === "skip") return "gray";
  if (kind === "conflict") return "amber";
  if (kind === "expand_parent_link") return "amber";
  return "green";
}

function skillCount(key: string): number {
  return grouped.value.reduce((n, [_, list]) => n + list.filter((s) => s.key === key).length, 0) || 0;
}
</script>

<template>
  <div class="config-layout">
    <!-- 左：Agent 列表 -->
    <aside class="agent-list card">
      <div class="list-head">
        <b>Agent 工具</b>
        <button class="btn ghost" style="padding: 3px 8px" @click="loadAll()" title="刷新">↻</button>
      </div>
      <button
        v-for="a in agents"
        :key="a.agent.id"
        class="agent-item"
        :class="{ active: selectedId === a.agent.id }"
        @click="selectAgent(a.agent.id)"
      >
        <span class="pulse" :class="!a.dir_ok ? 'red' : a.drift ? 'amber' : 'green'"></span>
        <span class="agent-name">{{ a.agent.name }}</span>
        <span class="agent-count">{{ a.enabled_count }}</span>
      </button>
      <div v-if="!agents.length" class="empty" style="padding: 24px 8px">
        暂无 Agent，请先到「设置」注册。
      </div>
    </aside>

    <!-- 右：配置 -->
    <section class="config-main" v-if="current">
      <header class="agent-head">
        <div>
          <div class="head-line">
            <h2>{{ current.agent.name }}</h2>
            <span class="badge" :class="!current.dir_ok ? 'red' : current.drift ? 'amber' : 'green'">
              {{ current.message }}
            </span>
            <span class="badge gray" v-if="current.agent.builtin">内置</span>
          </div>
          <code class="dir">{{ current.agent.skills_dir }}</code>
        </div>
        <div class="head-actions">
          <button class="btn" :disabled="busy" @click="doImportExisting()" v-if="current.is_parent_link" title="把当前整目录链接内的技能导入为配置">
            导入当前链接
          </button>
          <button class="btn" :disabled="busy" @click="refreshDiff()">刷新差异</button>
          <button class="btn" :disabled="busy" @click="doRollback()">回滚</button>
          <button class="btn primary" :disabled="busy" @click="showPreview = true">部署</button>
        </div>
      </header>

      <!-- 整目录链接迁移提示 -->
      <div v-if="current.is_parent_link" class="card parent-banner">
        <b>当前是整目录链接模式</b>（skills 目录整体指向技能库，{{ current.parent_visible }} 个技能经父链接可见）。
        点「导入当前链接」把这批技能设为配置，再「部署」即可迁移为逐技能独立链接，之后就能按 Agent 分别配置了。
      </div>

      <!-- 结果提示 -->
      <div v-if="lastResult" class="card result" :class="lastResult.ok ? 'ok' : 'err'">
        {{ lastResult.summary }}
        <button class="btn ghost" style="padding: 2px 8px" @click="lastResult = null">✕</button>
      </div>

      <!-- 差异栏 -->
      <div class="diff-bar" :class="{ dirty: diffSummary !== '已同步' }">
        <span class="diff-label">部署差异</span>
        <b>{{ diffLoading ? "计算中…" : diffSummary }}</b>
        <button v-if="diffSummary !== '已同步' && !diffLoading" class="btn" style="margin-left: auto; padding: 4px 10px" @click="showPreview = true">
          查看明细
        </button>
      </div>

      <!-- 技能开关列表 -->
      <div class="skill-groups">
        <div v-for="[cat, list] in grouped" :key="cat" class="group">
          <div class="group-head">
            <span>{{ cat === "(根)" ? "全部技能" : cat }}</span>
            <span class="group-count">{{ list.length }}</span>
          </div>
          <div class="group-list">
            <label v-for="s in list" :key="s.key" class="skill-item" :class="{ unhealthy: !s.healthy }">
              <span class="switch">
                <input
                  type="checkbox"
                  :checked="isEnabled(current.agent.id, s.key)"
                  @change="(e: any) => onToggle(s.key, (e.target as HTMLInputElement).checked)"
                />
                <span class="track"></span>
              </span>
              <span class="s-name" :title="s.key">{{ s.name }}</span>
              <span class="s-desc">
                {{ showDesc(s.description) }}
                <button
                  v-if="needsTranslation(s.description) && !transMap[s.description]"
                  class="trans-btn"
                  title="翻译成中文"
                  @click.prevent.stop="ensureTranslate(s.description)"
                >译</button>
              </span>
              <span class="badge gray" v-if="!s.healthy" title="SKILL.md 缺失或 frontmatter 不合法">异常</span>
            </label>
          </div>
        </div>
      </div>
    </section>

    <div v-else class="card empty" style="flex: 1">
      <div class="icon">▣</div>
      从左侧选择一个 AI Agent 工具开始配置
    </div>

    <!-- 部署预览弹窗 -->
    <div v-if="showPreview" class="modal-mask" @click.self="showPreview = false">
      <div class="modal" style="max-width: 720px">
        <div class="modal-head">
          <b>部署预览 · {{ current?.agent.name }}</b>
          <button class="btn ghost" @click="showPreview = false">✕</button>
        </div>
        <div class="modal-body">
          <div class="preview-note">
            以下操作将同步「技能配置」到 Agent 的 skills 目录（创建/删除 Junction，源文件不动）。
          </div>
          <table class="diff-table">
            <thead>
              <tr>
                <th>操作</th>
                <th>技能</th>
                <th>链接路径 / 说明</th>
              </tr>
            </thead>
            <tbody>
              <tr v-for="(d, i) in diff" :key="i" :class="d.kind">
                <td><span class="badge" :class="diffTone(d.kind)">{{ diffLabel(d.kind) }}</span></td>
                <td>{{ d.skill_key }}</td>
                <td class="cell-path" :title="d.link_path">
                  {{ d.link_path }}
                  <div v-if="d.current_target" class="cell-sub">当前: {{ d.current_target }}</div>
                  <div v-if="d.expected_target" class="cell-sub">目标: {{ d.expected_target }}</div>
                </td>
              </tr>
            </tbody>
          </table>
          <div v-if="!diff.filter((d) => d.kind !== 'keep' && d.kind !== 'external' && d.kind !== 'skip').length" class="empty" style="padding: 24px">
            配置已与实况一致，无需部署。
          </div>
        </div>
        <div class="modal-foot">
          <button class="btn" @click="showPreview = false">取消</button>
          <button class="btn" :disabled="busy" @click="doDeploy(true)">仅预览（干跑）</button>
          <button class="btn primary" :disabled="busy" @click="doDeploy(false)">确认部署</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.config-layout {
  display: flex;
  gap: 16px;
  height: calc(100vh - 52px);
}

.agent-list {
  width: 212px;
  flex-shrink: 0;
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
  overflow: auto;
}
.list-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 2px 4px 8px;
  font-size: 13px;
  color: var(--muted);
}
.agent-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--text);
  cursor: pointer;
  font-size: 13px;
  text-align: left;
  width: 100%;
  transition: background 0.15s;
}
.agent-item:hover {
  background: var(--bg-soft);
}
.agent-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.pulse {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  flex-shrink: 0;
}
.pulse.green { background: var(--green); }
.pulse.amber { background: var(--amber); }
.pulse.red { background: var(--red); }
.agent-name {
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.agent-count {
  font-size: 11px;
  color: var(--faint);
  background: var(--bg-soft);
  border-radius: 10px;
  padding: 0 7px;
}

.config-main {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding-right: 2px;
}
.agent-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 14px;
  margin-bottom: 14px;
}
.head-line {
  display: flex;
  align-items: center;
  gap: 10px;
}
h2 {
  font-size: 19px;
  font-weight: 700;
}
.dir {
  display: block;
  font-size: 12px;
  color: var(--faint);
  margin-top: 4px;
}
.head-actions {
  display: flex;
  gap: 8px;
}

.result {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 12px;
  font-size: 13px;
  padding: 10px 14px;
}
.result.ok {
  background: var(--green-soft);
  border-color: rgba(52, 211, 153, 0.4);
  color: var(--green);
}
.result.err {
  background: var(--red-soft);
  border-color: rgba(248, 113, 113, 0.4);
  color: var(--red);
}

.parent-banner {
  background: var(--amber-soft);
  border-color: rgba(251, 191, 36, 0.45);
  color: #fde68a;
  font-size: 13px;
  padding: 11px 14px;
  margin-bottom: 12px;
  line-height: 1.7;
}

.diff-bar {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 14px;
  border-radius: var(--radius-sm);
  background: var(--panel);
  border: 1px solid var(--border-soft);
  font-size: 13px;
  margin-bottom: 14px;
}
.diff-bar.dirty {
  border-color: rgba(251, 191, 36, 0.45);
  background: var(--amber-soft);
}
.diff-label {
  color: var(--faint);
}

.skill-groups {
  display: flex;
  flex-direction: column;
  gap: 16px;
}
.group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  font-weight: 600;
  color: var(--muted);
  margin-bottom: 6px;
}
.group-count {
  font-size: 11px;
  color: var(--faint);
  background: var(--bg-soft);
  border-radius: 10px;
  padding: 0 7px;
}
.group-list {
  background: var(--panel);
  border: 1px solid var(--border-soft);
  border-radius: var(--radius);
  overflow: hidden;
}
.skill-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 8px 14px;
  border-bottom: 1px solid var(--border-soft);
  cursor: pointer;
  transition: background 0.12s;
}
.skill-item:last-child {
  border-bottom: none;
}
.skill-item:hover {
  background: var(--bg-soft);
}
.skill-item.unhealthy .s-name {
  color: var(--faint);
  text-decoration: line-through;
}
.s-name {
  font-size: 13px;
  font-weight: 600;
  min-width: 130px;
  max-width: 170px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.s-desc {
  flex: 1;
  min-width: 0;
  font-size: 12.5px;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

/* 描述翻译按钮 */
.trans-btn {
  display: inline-block;
  padding: 0 7px;
  border-radius: 9px;
  border: 1px solid rgba(124, 92, 252, 0.35);
  background: rgba(124, 92, 252, 0.08);
  color: #7c5cfc;
  font-size: 10.5px;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;
}
.trans-btn:hover {
  background: rgba(124, 92, 252, 0.16);
}

.preview-note {
  font-size: 12.5px;
  color: var(--muted);
  background: var(--bg-soft);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  margin-bottom: 12px;
}
.diff-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12.5px;
}
.diff-table th {
  text-align: left;
  color: var(--faint);
  font-weight: 500;
  padding: 6px 8px;
  border-bottom: 1px solid var(--border);
}
.diff-table td {
  padding: 6px 8px;
  border-bottom: 1px solid var(--border-soft);
  vertical-align: top;
}
.diff-table tr.to_remove td { background: rgba(248, 113, 113, 0.05); }
.diff-table tr.to_create td, .diff-table tr.to_replace td { background: rgba(99, 102, 241, 0.05); }
.diff-table tr.conflict td { background: rgba(251, 191, 36, 0.06); }
.cell-path {
  font-family: Consolas, monospace;
  font-size: 11.5px;
  word-break: break-all;
}
.cell-sub {
  color: var(--faint);
  margin-top: 2px;
}
</style>
