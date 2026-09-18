<script setup lang="ts">
import { computed, ref } from "vue";
import { store, loadAll, toast, ensureTranslate, needsTranslation, transMap, isEnabled, ui } from "../store";
import { listExternalSkills, adoptSkill } from "../api/tauri";
import type { ExternalSkill, SkillInfo } from "../types";

const totalSkills = computed(() => store.skills.length);
const healthy = computed(() => store.skills.filter((s) => s.healthy).length);
const agents = computed(() => store.agents);
const pendingAgents = computed(() => agents.value.filter((a) => a.drift));
const readyAgents = computed(() => agents.value.filter((a) => !a.drift && a.dir_ok));

/// 没有分配给任何 Agent 的技能（装了但没启用，提醒用户别浪费）
const unassigned = computed(() =>
  store.skills.filter((s) => !store.agents.some((a) => isEnabled(a.agent.id, s.key)))
);
function gotoAgents() {
  ui.page = "agents";
}

// 外部技能弹窗
const externalFor = ref<{ id: string; name: string } | null>(null);
const externalList = ref<ExternalSkill[]>([]);
const externalLoading = ref(false);
const adopting = ref("");

function statusTone(a: { drift: boolean; dir_ok: boolean }) {
  if (!a.dir_ok) return "red";
  if (a.drift) return "amber";
  return "green";
}

async function openExternal(id: string, name: string, count: number) {
  if (!count) return;
  externalFor.value = { id, name };
  externalList.value = [];
  externalLoading.value = true;
  try {
    externalList.value = await listExternalSkills(id);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    externalLoading.value = false;
  }
}

async function doAdopt(dirName: string) {
  if (!externalFor.value || adopting.value) return;
  adopting.value = dirName;
  try {
    await adoptSkill(externalFor.value.id, dirName);
    toast(`「${dirName}」已收编到技能库，可在「Agent 配置」中分配给其他工具`, "success");
    await loadAll();
    externalList.value = externalList.value.filter((s) => s.dir_name !== dirName);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    adopting.value = "";
  }
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
  // 打开后自动把英文描述翻成中文（走全局缓存，只翻一次）
  configuredSkills.value.forEach((s) => {
    if (s.info) ensureTranslate(s.info.description);
  });
}
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>总览</h1>
        <p class="sub">技能库与所有 AI Agent 的接入状态一目了然</p>
      </div>
      <button class="btn primary" @click="loadAll()" :disabled="store.loading">
        <span v-if="store.loading">扫描中…</span>
        <span v-else>重新扫描</span>
      </button>
    </header>

    <div class="stats">
      <div class="card stat">
        <div class="stat-icon blue">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2 3 7v10l9 5 9-5V7l-9-5z"/></svg>
        </div>
        <div class="stat-body">
          <div class="stat-num">{{ totalSkills }}</div>
          <div class="stat-label">技能总数</div>
        </div>
      </div>
      <div class="card stat">
        <div class="stat-icon green">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4L9 16.2z"/></svg>
        </div>
        <div class="stat-body">
          <div class="stat-num">{{ healthy }}</div>
          <div class="stat-label">健康技能</div>
        </div>
      </div>
      <div class="card stat">
        <div class="stat-icon purple">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M12 2a4 4 0 0 1 4 4c0 1.4-.7 2.6-1.8 3.3A6 6 0 0 1 18 15v2H6v-2a6 6 0 0 1 3.8-5.7A4 4 0 0 1 12 2z"/></svg>
        </div>
        <div class="stat-body">
          <div class="stat-num">{{ agents.length }}</div>
          <div class="stat-label">Agent 数量</div>
        </div>
      </div>
      <div class="card stat">
        <div class="stat-icon amber">
          <svg viewBox="0 0 24 24" fill="currentColor"><path d="M13 3a9 9 0 0 0-9 9H1l3.9 3.9.1.1L9 12H6a7 7 0 1 1 2 4.9l-1.4 1.4A9 9 0 1 0 13 3zm-1 5v5l4.3 2.5.7-1.2-3.5-2.1V8H12z"/></svg>
        </div>
        <div class="stat-body">
          <div class="stat-num">{{ pendingAgents.length }}</div>
          <div class="stat-label">待部署</div>
        </div>
      </div>
    </div>

    <!-- 未分配技能提醒 -->
    <div v-if="unassigned.length" class="card unassigned-banner" @click="gotoAgents">
      <span class="unassigned-icon">📦</span>
      <span class="unassigned-txt">
        有 <b>{{ unassigned.length }}</b> 个技能还没有分配给任何 Agent
        <span class="unassigned-preview">{{ unassigned.slice(0, 4).map((s) => s.name).join("、") }}{{ unassigned.length > 4 ? " 等" : "" }}</span>
      </span>
      <span class="unassigned-go">去分配 →</span>
    </div>

    <h2 class="section-title">Agent 状态</h2>
    <div class="agent-grid">
      <div
        v-for="a in agents"
        :key="a.agent.id"
        class="card agent-card"
        :class="statusTone(a)"
        :title="`点击查看 ${a.agent.name} 配置的技能`"
        @click="openSkills({ id: a.agent.id, name: a.agent.name })"
      >
        <div class="agent-row">
          <span class="pulse" :class="statusTone(a)"></span>
          <div class="agent-info">
            <b>{{ a.agent.name }}</b>
            <span class="agent-dir" :title="a.agent.skills_dir">{{ a.agent.skills_dir }}</span>
          </div>
          <span class="badge" :class="statusTone(a)">
            {{ a.drift ? "待部署" : !a.dir_ok ? "异常" : "就绪" }}
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
          <div
            class="meta-item"
            :class="{ clickable: a.external_count }"
            v-if="a.external_count"
            :title="`点击查看 ${a.external_count} 个外部技能`"
            @click.stop="openExternal(a.agent.id, a.agent.name, a.external_count)"
          >
            <span class="meta-num purple">{{ a.external_count }}</span>
            <span class="meta-label">外部目录 ⌕</span>
          </div>
        </div>
        <div class="agent-msg">{{ a.message }}</div>
      </div>
      <div v-if="!agents.length" class="card empty">
        <div class="icon">▣</div>
        尚未注册任何 Agent，请前往「设置」添加。
      </div>
    </div>

    <p class="foot-note" v-if="readyAgents.length">
      ✅ {{ readyAgents.length }} 个 Agent 已就绪；{{ pendingAgents.length }} 个待部署，可在「Agent 配置」中处理。
    </p>

    <!-- 外部技能弹窗 -->
    <div v-if="externalFor" class="modal-mask" @click.self="externalFor = null">
      <div class="modal" style="max-width: 680px">
        <div class="modal-head">
          <b>{{ externalFor.name }} 的外部技能（实体目录，工具不会触碰）</b>
          <button class="btn ghost" @click="externalFor = null">✕</button>
        </div>
        <div class="modal-body">
          <div class="ext-note">
            这些是该工具自带 / 对话中自动安装的实体技能。点「收编」可把它的副本复制进技能库，
            之后就能软链接给其他 Agent 使用；原目录不动，不影响该工具自身运行。
          </div>
          <div v-if="externalLoading" class="empty" style="padding: 24px">扫描中…</div>
          <div v-else class="ext-list">
            <div v-for="s in externalList" :key="s.dir_name" class="ext-item">
              <div class="ext-info">
                <div class="ext-line">
                  <b>{{ s.dir_name }}</b>
                  <span v-if="s.system" class="badge gray" title="工具自带的系统技能（点开头目录），收编后可共享给其他工具">系统</span>
                  <span v-if="!s.healthy" class="badge amber" title="SKILL.md 缺失或格式不合法">无标准元信息</span>
                </div>
                <div class="ext-desc">{{ s.description || "（无描述）" }}</div>
                <code class="ext-path" :title="s.path">{{ s.path }}</code>
              </div>
              <button class="btn primary ext-btn" :disabled="adopting === s.dir_name" @click="doAdopt(s.dir_name)">
                {{ adopting === s.dir_name ? "收编中…" : "收编" }}
              </button>
            </div>
            <div v-if="!externalList.length" class="empty" style="padding: 24px">
              没有外部实体技能，全部是链接，很干净 ✅
            </div>
          </div>
        </div>
        <div class="modal-foot">
          <span class="ext-count">{{ externalList.length }} 个技能</span>
          <button class="btn" @click="externalFor = null">关闭</button>
        </div>
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
            删除链接不影响源文件。要增删技能请到「Agent 配置」页。
          </div>
          <div v-if="!configuredSkills.length" class="empty" style="padding: 24px">
            还没有配置任何技能，去「Agent 配置」勾选吧。
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
.stats {
  display: grid;
  grid-template-columns: repeat(4, 1fr);
  gap: 16px;
  margin-bottom: 26px;
}
.stat {
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 18px;
}
.stat-icon {
  width: 46px;
  height: 46px;
  border-radius: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
}
.stat-icon svg {
  width: 22px;
  height: 22px;
}
.stat-icon.blue { background: var(--accent-soft); color: var(--accent); }
.stat-icon.green { background: var(--green-soft); color: var(--green); }
.stat-icon.purple { background: rgba(124, 92, 252, 0.1); color: #7c5cfc; }
.stat-icon.amber { background: var(--amber-soft); color: #d97706; }

.stat-body {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.stat-num {
  font-size: 26px;
  font-weight: 800;
  letter-spacing: -0.5px;
  font-variant-numeric: tabular-nums;
}
.stat-label {
  color: var(--muted);
  font-size: 12.5px;
}

.section-title {
  font-size: 15px;
  font-weight: 600;
  margin-bottom: 12px;
}

/* 未分配技能提醒横幅 */
.unassigned-banner {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 12px 16px;
  margin-bottom: 20px;
  border: 1px dashed rgba(245, 158, 11, 0.5);
  cursor: pointer;
  transition: all 0.15s;
}
.unassigned-banner:hover {
  border-color: var(--accent);
  transform: translateY(-1px);
  box-shadow: var(--shadow);
}
.unassigned-icon {
  font-size: 20px;
}
.unassigned-txt {
  flex: 1;
  font-size: 13px;
  color: var(--text);
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.unassigned-preview {
  color: var(--muted);
  font-size: 12px;
  margin-left: 8px;
}
.unassigned-go {
  color: var(--accent);
  font-size: 12.5px;
  font-weight: 600;
  flex-shrink: 0;
}

.agent-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
  gap: 16px;
}
.agent-card {
  transition: all 0.16s;
  position: relative;
  overflow: hidden;
}
.agent-card::before {
  content: "";
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  background: var(--border-soft);
}
.agent-card.green::before { background: linear-gradient(90deg, #12b76a, #4ade80); }
.agent-card.amber::before { background: linear-gradient(90deg, #f59e0b, #fbbf24); }
.agent-card.red::before { background: linear-gradient(90deg, #ef4444, #f87171); }
.agent-card:hover {
  box-shadow: var(--shadow);
  transform: translateY(-2px);
}
.agent-card {
  cursor: pointer;
}

.agent-row {
  display: flex;
  align-items: center;
  gap: 10px;
}
.pulse {
  width: 9px;
  height: 9px;
  border-radius: 50%;
  flex-shrink: 0;
}
.pulse.green { background: var(--green); box-shadow: 0 0 0 3px var(--green-soft); }
.pulse.amber { background: var(--amber); box-shadow: 0 0 0 3px var(--amber-soft); }
.pulse.red { background: var(--red); box-shadow: 0 0 0 3px var(--red-soft); }

.agent-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.agent-info b {
  font-size: 14.5px;
}
.agent-dir {
  color: var(--faint);
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.agent-meta {
  display: flex;
  gap: 20px;
  margin-top: 14px;
  padding-top: 12px;
  border-top: 1px solid var(--border-soft);
}
.meta-item {
  display: flex;
  flex-direction: column;
  align-items: center;
}
.meta-item.clickable {
  cursor: pointer;
  border-radius: 8px;
  padding: 2px 8px;
  margin: -2px -8px;
  transition: background 0.15s;
}
.meta-item.clickable:hover {
  background: rgba(124, 92, 252, 0.08);
}
.meta-num.purple { color: #7c5cfc; }

/* 外部技能弹窗 */
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
.ext-btn {
  padding: 5px 14px;
  font-size: 12.5px;
  flex-shrink: 0;
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
.meta-num.green { color: var(--green); }
.meta-num.amber { color: #d97706; }
.meta-label {
  font-size: 11px;
  color: var(--faint);
}

.agent-msg {
  margin-top: 10px;
  font-size: 12px;
  color: var(--muted);
  background: var(--bg-soft);
  border-radius: 8px;
  padding: 6px 10px;
}

.foot-note {
  margin-top: 20px;
  color: var(--muted);
  font-size: 13px;
}

/* 描述翻译按钮 */
.trans-btn {
  display: inline-block;
  margin-left: 6px;
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
</style>
