<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { store, openAgentSettings, openSkillAllocation, discovery, discoverMachine, ensureTranslate, needsTranslation, transMap, isEnabled, ui, tokenStats, refreshTokenStats } from "../store";
import { useEsc } from "../composables/useEsc";
import type { SkillInfo } from "../types";
import BuiltinSkills from "../components/BuiltinSkills.vue";
import { installedBuiltinSources, sourceProduct } from "../utils/builtinSources";

// Esc 关闭当前弹窗
useEsc(() => {
  if (skillsFor.value) skillsFor.value = null;
  else if (builtinFor.value) builtinFor.value = null;
});

watch(() => store.agents, (agents) => {
  if (agents.length) void refreshTokenStats();
}, { immediate: true });

const totalSkills = computed(() => store.skills.length);
const healthy = computed(() => store.skills.filter((s) => s.healthy).length);
const builtinSources = computed(() => installedBuiltinSources(discovery.inventory));
const nativeSkills = computed(() => builtinSources.value.reduce((n, s) => n + s.skill_count, 0));
const unverifiedCacheSkills = computed(() => builtinSources.value.filter(s => s.cache_only).reduce((n, s) => n + s.skill_count, 0));
const installedAgentIds = computed(() => new Set(discovery.inventory?.agents.filter(a => a.installed && !a.shared).map(a => a.id) || []));
const agents = computed(() => store.agents.filter(a => installedAgentIds.value.has(a.agent.id)));
function directoryLinks(id: string) {
  return discovery.inventory?.agents.find(agent => agent.id === id)?.path_links.filter(link => link.scope === "directory") || [];
}
const builtinProducts = computed(() => {
  const grouped = new Map<string, { name: string; sources: number; skills: number; system: number; unverified: number; manual: number }>();
  for (const source of builtinSources.value) {
    const name = sourceProduct(source.group);
    const entry = grouped.get(name) || { name, sources: 0, skills: 0, system: 0, unverified: 0, manual: 0 };
    entry.sources++;
    entry.skills += source.skill_count;
    if (source.custom) entry.manual += source.skill_count;
    else if (source.group.includes("系统技能")) entry.system += source.skill_count;
    if (source.cache_only) entry.unverified += source.skill_count;
    grouped.set(name, entry);
  }
  return [...grouped.values()].sort((a, b) => a.name.localeCompare(b.name, "zh"));
});
const builtinFor = ref<string | null>(null);
const pendingAgents = computed(() => agents.value.filter((a) => a.drift));
const readyAgents = computed(() => agents.value.filter((a) => !a.drift && a.dir_ok));

/// 没有分配给任何 Agent 的技能（装了但没启用，提醒用户别浪费）
const unassigned = computed(() =>
  store.skills.filter((s) => !agents.value.some((a) => isEnabled(a.agent.id, s.key)))
);
function gotoAgents() {
  openSkillAllocation();
}

function formatTokens(n: number): string {
  if (n < 1000) return `${n}`;
  return `${(n / 1000).toFixed(1)}k`;
}

function statusTone(a: { drift: boolean; dir_ok: boolean }) {
  if (!a.dir_ok) return "amber";
  if (a.drift) return "amber";
  return "green";
}

// ---------- 已配置技能弹窗 ----------

const skillsFor = ref<{ id: string; name: string } | null>(null);

/** 该 Agent 配置的技能（含在技能库中的完整信息与链接目标路径） */
const configuredSkills = computed(() => {
  if (!skillsFor.value) return [] as Array<{ key: string; info: SkillInfo | null; target: string }>;
  const root = store.config?.skills_root || "";
  return (store.config?.agent_configs[skillsFor.value.id]?.enabled_skills || []).map((key) => {
    const info = store.skills.find((s) => s.key === key) || null;
    return { key, info, target: info?.path || `${root}\\${key.replaceAll("/", "\\")}` };
  });
});

function openSkills(a: { id: string; name: string }) {
  skillsFor.value = { id: a.id, name: a.name };
}
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>工作空间</h1>
        <p class="sub">你的技能、Agent 和部署状态，集中管理。</p>
      </div>
      <div class="dashboard-actions">
        <button class="btn" @click="discoverMachine()" :disabled="store.loading || discovery.loading">
          <svg viewBox="0 0 24 24" width="14" height="14" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M20 7v5h-5M4 17v-5h5M6 8a7 7 0 0 1 11.8-2L20 9M4 15l2.2 3A7 7 0 0 0 18 16" /></svg>
          {{ store.loading || discovery.loading ? "扫描中…" : "重新扫描" }}
        </button>
        <button class="btn primary" @click="ui.page = 'skills'">浏览技能库 <span>↗</span></button>
      </div>
    </header>

    <div class="stats">
      <article class="card stat">
        <div class="stat-heading"><span>技能库</span><svg viewBox="0 0 24 24"><path d="m12 3 9 5-9 5-9-5 9-5ZM3 8v9l9 5 9-5V8M12 13v9" /></svg></div>
        <div class="stat-num">{{ totalSkills }}<span>个技能</span></div>
        <p class="stat-caption">{{ healthy }} 个技能元信息完整</p>
      </article>
      <article class="card stat builtin-stat" role="button" tabindex="0" title="查看已开启工具的用户技能" @click="builtinFor = 'all'" @keydown.enter="builtinFor = 'all'" @keydown.space.prevent="builtinFor = 'all'">
        <div class="stat-heading"><span>工具技能目录</span><svg viewBox="0 0 24 24"><path d="M9 3H4v7h3a2 2 0 1 1 0 4H4v7h7v-3a2 2 0 1 1 4 0v3h5v-7h-3a2 2 0 1 1 0-4h3V3h-5v3a3 3 0 1 1-6 0V3Z" /></svg></div>
        <div class="stat-num">{{ nativeSkills }}<span>个条目</span></div>
        <p class="stat-caption">{{ builtinSources.length }} 处目录<span v-if="unverifiedCacheSkills"> · {{ unverifiedCacheSkills }} 个缓存待确认</span> · 查看 ↗</p>
      </article>
      <article class="card stat">
        <div class="stat-heading"><span>已接入 Agent</span><svg viewBox="0 0 24 24"><rect x="3" y="5" width="18" height="13" rx="2"/><path d="M8 22h8M12 18v4M8 10h.01M16 10h.01M8 14h8"/></svg></div>
        <div class="stat-num">{{ agents.length }}<span>个工具</span></div>
        <p class="stat-caption">仅显示已开启管理的工具</p>
      </article>
      <article class="card stat">
        <div class="stat-heading"><span>待部署</span><svg viewBox="0 0 24 24"><path d="M12 3v12m-4-4 4 4 4-4M4 15v5h16v-5"/></svg></div>
        <div class="stat-num">{{ pendingAgents.length }}<span>个工具</span></div>
        <p class="stat-caption">{{ readyAgents.length }} 个工具的目录已就绪</p>
      </article>
    </div>

    <!-- 未分配技能提醒 -->
    <button v-if="unassigned.length" class="card unassigned-banner" @click="gotoAgents">
      <span class="unassigned-icon">◇</span>
      <span class="unassigned-txt">
        有 <b>{{ unassigned.length }}</b> 个技能还没有分配给任何 Agent
        <span class="unassigned-preview">{{ unassigned.slice(0, 4).map((s) => s.name).join("、") }}{{ unassigned.length > 4 ? " 等" : "" }}</span>
      </span>
      <span class="unassigned-go">去分配 →</span>
    </button>

    <div class="section-heading"><div><h2 class="section-title">你的 Agent <span>{{ agents.length }}</span></h2><p>查看接入状态，选择工具管理已配置的技能。</p></div><button class="section-link" @click="openAgentSettings()">管理工具 <span>↗</span></button></div>
    <div class="agent-grid">
      <div
        v-for="a in agents"
        :key="a.agent.id"
        class="card agent-card"
        :class="statusTone(a)"
        :title="`点击查看 ${a.agent.name} 配置的技能`"
        role="button"
        tabindex="0"
        @keydown.enter="openSkills({ id: a.agent.id, name: a.agent.name })"
        @keydown.space.prevent="openSkills({ id: a.agent.id, name: a.agent.name })"
        @click="openSkills({ id: a.agent.id, name: a.agent.name })"
      >
        <div class="agent-row">
          <span class="agent-avatar">{{ a.agent.name.replace('OpenAI ', '').replace('腾讯 ', '').slice(0, 2).toUpperCase() }}</span>
          <div class="agent-info">
            <b>{{ a.agent.name }}</b>
            <span class="agent-dir" :title="a.agent.skills_dir">{{ a.agent.skills_dir }}</span>
            <span v-if="directoryLinks(a.agent.id).length" class="agent-link-note" :title="directoryLinks(a.agent.id).map(link => `${link.path} → ${link.target}`).join('\n')">↪ 路径联接 · 查看设置中的实际目录</span>
          </div>
          <span class="badge" :class="statusTone(a)">
            {{ !a.dir_ok ? "待初始化" : a.drift ? "待部署" : "已就绪" }}
          </span>
        </div>
        <div class="agent-meta">
          <div class="meta-item">
            <span class="meta-num">{{ a.enabled_count }}</span>
            <span class="meta-label">已配置</span>
          </div>
          <div class="meta-item">
            <span class="meta-num green">{{ a.linked_count }}</span>
            <span class="meta-label">已链接</span>
          </div>
          <div class="meta-item" v-if="a.parent_visible">
            <span class="meta-num amber">{{ a.parent_visible }}</span>
            <span class="meta-label">整目录可见</span>
          </div>
        </div>
        <!-- Token 上下文预算条 -->
        <div v-if="tokenStats[a.agent.id]" class="token-bar-wrap">
          <span class="meta-label" style="align-self: center; margin-right: 6px">上下文</span>
          <div class="token-bar-track">
            <div
              class="token-bar-fill"
              :class="{
                red: tokenStats[a.agent.id]!.over_budget,
                amber: !tokenStats[a.agent.id]!.over_budget && tokenStats[a.agent.id]!.resident_tokens * 100 / tokenStats[a.agent.id]!.budget > 80
              }"
              :style="{ width: Math.min(100, tokenStats[a.agent.id]!.resident_tokens * 100 / Math.max(1, tokenStats[a.agent.id]!.budget)) + '%' }"
            ></div>
          </div>
          <code class="token-bar-text">{{ formatTokens(tokenStats[a.agent.id]!.resident_tokens) }} / {{ formatTokens(tokenStats[a.agent.id]!.budget) }} tok</code>
        </div>
        <div class="agent-msg">{{ a.message }}</div>
      </div>
      <div v-if="!agents.length" class="card empty">
        <div class="icon">▣</div>
        尚未发现可管理的已安装 Agent。<br /><button class="btn" style="margin-top:12px" @click="openAgentSettings()">前往设置查看本机工具 →</button>
      </div>
    </div>

    <div class="section-heading"><div><h2 class="section-title">已开启工具的用户技能 <span>{{ nativeSkills }}</span></h2><p>查看用户安装或分配的技能，内置技能由原 Agent 管理。</p></div><button class="section-link" @click="builtinFor = 'all'">查看全部 <span>↗</span></button></div>
    <div class="builtin-product-grid" v-if="builtinProducts.length">
      <button v-for="product in builtinProducts" :key="product.name" class="card builtin-product" @click="builtinFor = product.name">
        <span class="builtin-product-mark">{{ product.name.slice(0, 2).toUpperCase() }}</span>
        <span class="builtin-product-info"><b>{{ product.name }}</b><small>{{ product.sources }} 处来源 · {{ product.skills }} 个目录</small><small>用户安装 / 分配的技能</small></span>
        <span class="builtin-product-arrow">↗</span>
      </button>
    </div>
    <div v-else class="card empty">{{ discovery.loading ? '正在扫描工具技能目录…' : '已开启的工具暂未发现用户技能。' }}</div>

    <p class="foot-note" v-if="readyAgents.length">
      {{ readyAgents.length }} 个 Agent 已就绪 · {{ pendingAgents.length }} 个待部署
    </p>

    <div v-if="builtinFor" class="modal-mask" @click.self="builtinFor = null">
      <div class="modal builtin-modal" role="dialog" aria-modal="true" aria-label="工具技能目录">
        <div class="modal-head"><b>工具技能目录 · 按来源浏览</b><button class="btn ghost" @click="builtinFor = null">✕</button></div>
        <div class="modal-body builtin-modal-body"><BuiltinSkills :initial-product="builtinFor" dialog /></div>
      </div>
    </div>

    <!-- 已配置技能弹窗 -->
    <div v-if="skillsFor" class="modal-mask" @click.self="skillsFor = null">
      <div class="modal" style="max-width: 680px">
        <div class="modal-head">
          <b>{{ skillsFor.name }} 已配置的技能（{{ configuredSkills.length }}）</b>
          <button class="btn ghost" @click="skillsFor = null">✕</button>
        </div>
        <div class="modal-body">
          <div class="ext-note">
            配置方式：每个技能以 Windows Junction 链接（mklink /J）部署到该 Agent 的
            skills 目录，源文件始终唯一保存在技能库（{{ store.config?.skills_root }}）中，
            删除链接不影响源文件。要增删技能请到「技能库」的按 Agent 视图。
          </div>
          <div v-if="!configuredSkills.length" class="empty" style="padding: 24px">
            还没有配置任何技能，去「技能库」的按 Agent 视图勾选吧。
          </div>
          <div v-else class="ext-list">
            <div v-for="s in configuredSkills" :key="s.key" class="ext-item">
              <div class="ext-info">
                <div class="ext-line">
                  <b>{{ s.info?.name || s.key }}</b>
                  <span v-if="!s.info" class="badge red" title="技能库中找不到该技能目录">已失效</span>
                  <span v-else-if="!s.info.healthy" class="badge amber">SKILL.md 异常</span>
                </div>
                <div class="ext-desc" v-if="s.info">
                  {{ needsTranslation(s.info.description) && transMap[s.info.description] ? transMap[s.info.description] : s.info.description || "（无描述）" }}
                  <button
                    v-if="needsTranslation(s.info.description) && !transMap[s.info.description]"
                    class="trans-btn"
                    @click="ensureTranslate(s.info.description)"
                  >译</button>
                </div>
                <code class="ext-path" :title="s.target">链接 → {{ s.target }}</code>
              </div>
            </div>
          </div>
        </div>
        <div class="modal-foot">
          <span class="ext-count">源文件都在 {{ store.config?.skills_root }}</span>
          <button class="btn" @click="skillsFor = null">关闭</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>

.dashboard-actions { display:flex; gap:10px; align-items:center; }
.dashboard-actions .btn.primary { gap:18px; }
.stats { display:grid; grid-template-columns:repeat(4,minmax(0,1fr)); gap:16px; margin:28px 0 22px; }
.stat { padding:20px 22px 18px; }
.stat-heading { display:flex; justify-content:space-between; align-items:center; gap:10px; color:var(--muted); font-size:12px; font-weight:500; }
.stat-heading svg { width:17px; height:17px; stroke:currentColor; fill:none; stroke-width:1.5; stroke-linecap:round; stroke-linejoin:round; color:var(--faint); flex-shrink:0; }
.stat-num { font:500 32px/1.3 "Bahnschrift",sans-serif; letter-spacing:-1px; margin:14px 0 6px; font-variant-numeric:tabular-nums; }
.stat-num > span { font:400 11px "Microsoft YaHei UI",sans-serif; color:var(--faint); letter-spacing:0; margin-left:8px; }
.stat-caption { color:var(--faint); font-size:11px; }
.builtin-stat { cursor:pointer; }
.builtin-stat:hover { border-color:var(--accent); }
.builtin-stat:focus-visible { outline:2px solid var(--accent); outline-offset:2px; }
.builtin-product-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(230px,1fr)); gap:12px; }
.builtin-product { display:flex; align-items:center; gap:12px; width:100%; min-width:0; text-align:left; cursor:pointer; color:var(--text); font:inherit; }
.builtin-product:hover { border-color:var(--accent); }
.builtin-product-mark { width:36px; height:36px; flex:none; display:grid; place-items:center; border-radius:8px; background:var(--accent-soft); color:var(--accent); font:600 11px "Bahnschrift",sans-serif; }
.builtin-product-info { min-width:0; flex:1; display:flex; flex-direction:column; }
.builtin-product-info b { font-size:13px; }
.builtin-product-info small { color:var(--muted); font-size:11px; }
.builtin-product-arrow { color:var(--faint); }
.builtin-modal { width:min(1120px,calc(100vw - 48px)); max-width:none; height:min(790px,calc(100vh - 48px)); max-height:calc(100vh - 48px); display:flex; flex-direction:column; }
.builtin-modal-body { flex:1; display:flex; overflow:hidden; min-height:0; }
.builtin-modal-body :deep(.builtin-root) { width:100%; height:100%; min-height:0; display:flex; flex-direction:column; }
.builtin-modal-body :deep(.builtin-browser) { flex:1; min-height:0; }
.builtin-modal-body :deep(.source-list),.builtin-modal-body :deep(.source-detail) { max-height:none; min-height:0; overflow:auto; }
.builtin-modal-body :deep(.builtin-skills) { max-height:none; overflow:visible; }
.section-heading { display:flex; justify-content:space-between; align-items:center; gap:20px; margin:30px 0 18px; }
.section-title { font-size:16px; font-weight:600; }
.section-title > span { display:inline-block; margin-left:7px; padding:1px 6px; border:1px solid var(--border); border-radius:5px; font:11px/17px "Consolas",monospace; color:var(--muted); vertical-align:middle; }
.section-heading p { font-size:12px; color:var(--faint); margin-top:5px; }
.section-link { display:flex; align-items:center; gap:10px; background:none; border:0; padding:5px 0; color:var(--muted); font-size:12px; cursor:pointer; }
.section-link:hover { color:var(--accent); }
.unassigned-banner { width:100%; display:flex; align-items:center; text-align:left; gap:12px; padding:13px 16px; border:1px solid var(--border); background:var(--bg-soft); cursor:pointer; transition:border-color .15s; }
.unassigned-banner:hover { border-color:var(--accent); }
.unassigned-icon { display:grid; place-items:center; width:25px; height:25px; border:1px solid var(--border); border-radius:6px; color:var(--muted); flex-shrink:0; background:var(--panel); }
.unassigned-txt { flex:1; min-width:0; font-size:12px; color:var(--muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.unassigned-txt b { color:var(--text); font-weight:600; }
.unassigned-preview { color:var(--faint); font-size:11px; margin-left:10px; }
.unassigned-go { color:var(--accent); font-size:12px; font-weight:500; white-space:nowrap; }
.agent-grid { display:grid; grid-template-columns:repeat(auto-fill,minmax(280px,1fr)); gap:16px; }
.agent-card { cursor:pointer; padding:19px 20px 15px; transition:border-color .15s,box-shadow .15s; }
.agent-card:hover { border-color:#b8b8c1; box-shadow:var(--shadow); }
.agent-row { display:flex; align-items:center; gap:10px; }
.agent-avatar { display:grid; place-items:center; width:34px; height:34px; flex-shrink:0; border-radius:8px; border:1px solid var(--border); background:var(--bg-soft); font:500 11px "Bahnschrift",sans-serif; letter-spacing:-.4px; color:var(--text); }
.agent-info { flex:1; min-width:0; display:flex; flex-direction:column; }
.agent-info b { font-size:13px; font-weight:600; }
.agent-dir { color:var(--faint); font-size:10px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; margin-top:3px; font-family:"Cascadia Code","Consolas",monospace; }
.agent-link-note { color:var(--accent); font-size:10px; margin-top:4px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.agent-row .badge { font-size:10px; white-space:nowrap; padding:2px 6px; }
.agent-meta { display:flex; gap:28px; padding:19px 0 3px; margin-top:14px; border-top:1px solid var(--border-soft); }
.meta-item { display:flex; flex-direction:column; align-items:flex-start; gap:3px; }
@media (max-width:1100px) {
  .stats { gap:12px; }
  .stat { padding:16px; }
  .stat-num { font-size:29px; }
  .unassigned-preview { display:none; }
}

/* 已配置技能弹窗 */
.ext-note {
  font-size: 12.5px;
  color: var(--muted);
  background: var(--bg-soft);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  margin-bottom: 12px;
  line-height: 1.7;
}
.ext-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.ext-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--panel);
  transition: all 0.15s;
}
.ext-item:hover {
  border-color: var(--accent);
}
.ext-info {
  flex: 1;
  min-width: 0;
}
.ext-line {
  display: flex;
  align-items: center;
  gap: 8px;
}
.ext-line b {
  font-size: 13.5px;
}
.ext-desc {
  color: var(--muted);
  font-size: 12px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.ext-path {
  display: block;
  font-size: 11px;
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  margin-top: 2px;
}
.ext-count {
  color: var(--faint);
  font-size: 12px;
  margin-right: auto;
}
.meta-num {
  font-size: 17px;
  font-weight: 700;
  font-variant-numeric: tabular-nums;
}
.meta-num.green { color: var(--text); }
.meta-num.amber { color: #d97706; }
.meta-label {
  font-size: 11px;
  color: var(--faint);
}

.agent-msg {
  margin-top: 10px;
  font-size: 11px;
  color: var(--muted);
  padding: 10px 0 0;
  border-top: 1px solid var(--border-soft);
}

.foot-note {
  margin-top: 20px;
  color: var(--muted);
  font-size: 13px;
}

/* Token 上下文预算条 */
.token-bar-wrap {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 8px;
  padding-top: 8px;
  border-top: 1px solid var(--border-soft);
}
.token-bar-track {
  flex: 1;
  height: 5px;
  border-radius: 3px;
  background: var(--border-soft);
  overflow: hidden;
}
.token-bar-fill {
  height: 100%;
  border-radius: 3px;
  background: var(--green);
  transition: width 0.3s, background 0.3s;
}
.token-bar-fill.amber { background: #d97706; }
.token-bar-fill.red { background: var(--red); }
.token-bar-text {
  font-size: 11px;
  color: var(--faint);
  white-space: nowrap;
}
</style>
