<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { marked } from "marked";
import {
  store,
  ui,
  openAgentSettings,
  loadAll,
  isEnabled,
  deleteSkill,
  toast,
  ensureTranslate,
  needsTranslation,
  transMap,
  skillMeta,
  updateSkillMeta,
  secLevels,
  loadSecuritySummary,
} from "../store";
import {
  formatBytes,
  formatTs,
  readSkillDetail,
  listTrash,
  restoreTrash,
  purgeTrash,
  scanSkillSecurity,
} from "../api/tauri";
import type { SkillInfo, SkillDetail, TrashItem, SecurityReport } from "../types";
import { useEsc } from "../composables/useEsc";
import SkillIcon from "../components/SkillIcon.vue";
import SkillAssignments from "../components/SkillAssignments.vue";
import AgentOwnedSkills from "../components/AgentOwnedSkills.vue";
import ResourceTabs from "../components/ResourceTabs.vue";
import SkillAllocation from "../components/SkillAllocation.vue";
import AgentConfig from "./AgentConfig.vue";
import SkillCategories from "../components/SkillCategories.vue";
import { categoryLabel, inSkillCategory, ROOT_CATEGORY, skillCategories } from "../utils/skillCategories";
const resourceTab = ref("library");

// Esc 按弹窗层级关闭：全文预览 > 删除确认 > 技能详情
useEsc(() => {
  if (detail.value) detail.value = null;
  else if (confirming.value && !deleting.value) confirming.value = null;
  else if (selected.value) selected.value = null;
});

const keyword = ref("");
const activeCat = ref("");
const activeTag = ref("");
try { activeCat.value = localStorage.getItem("ash.skillCategory") || ""; } catch { /* 存储不可用时使用全部技能。 */ }
watch(activeCat, category => {
  try { localStorage.setItem("ash.skillCategory", category); } catch { /* 不影响本次筛选。 */ }
});
watch(() => [store.config?.skills_root, store.skills] as const, () => {
  if (store.loading) return;
  if (activeCat.value && activeCat.value !== ROOT_CATEGORY && !skillCategories(store.skills).includes(activeCat.value)) activeCat.value = "";
});
const selected = ref<SkillInfo | null>(null);
watch(() => store.skills, skills => {
  if (selected.value) selected.value = skills.find(s => s.key === selected.value!.key) || null;
});

// ---------- 删除技能（移入 Windows 回收站）----------

/** 待确认删除的技能 */
const confirming = ref<SkillInfo | null>(null);
const deleting = ref(false);

/** 该技能当前分配给了几个 Agent */
function assignedCount(key: string): number {
  return store.agents.filter((a) => isEnabled(a.agent.id, key)).length;
}

function askDelete(s: SkillInfo) {
  confirming.value = s;
}

async function doDelete() {
  if (!confirming.value || deleting.value) return;
  deleting.value = true;
  const r = await deleteSkill(confirming.value.key);
  deleting.value = false;
  if (r) {
    toast(r.message, "success");
    confirming.value = null;
    selected.value = null;
  }
}

const tags = computed(() => [...new Set(store.skills.flatMap(s => skillMeta(s.key).tags))].sort((a, b) => a.localeCompare(b, "zh")));
watch(tags, available => { if (activeTag.value && !available.includes(activeTag.value)) activeTag.value = ""; });
const filterKey = computed(() => JSON.stringify([activeCat.value, keyword.value, activeTag.value, showStarred.value]));

const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  return store.skills.filter((s) => {
    if (showStarred.value && !isStarred(s.key)) return false;
    if (!inSkillCategory(s.category, activeCat.value)) return false;
    if (activeTag.value && !skillMeta(s.key).tags.includes(activeTag.value)) return false;
    if (!kw) return true;
    return (
      s.name.toLowerCase().includes(kw) ||
      s.description.toLowerCase().includes(kw) ||
      s.key.toLowerCase().includes(kw)
    );
  });
});

// ---------- 英文描述翻译 ----------

function showDesc(d: string): string {
  if (!needsTranslation(d)) return d || "（无描述）";
  return transMap[d] || d;
}

function openDetail(s: SkillInfo) {
  selected.value = s;
  ensureTranslate(s.description);
  tagInput.value = "";
}

// ---------- 收藏 / 标签 ----------

const showStarred = ref(false);

function isStarred(key: string): boolean {
  return skillMeta(key).starred;
}

async function toggleStar(s: SkillInfo) {
  const m = skillMeta(s.key);
  await updateSkillMeta(s.key, !m.starred, m.tags);
  if (!m.starred) toast(`已收藏「${s.name}」`, "success");
}

const tagInput = ref("");

function addTag() {
  if (!selected.value || !tagInput.value.trim()) return;
  const m = skillMeta(selected.value.key);
  const t = tagInput.value.trim();
  if (!m.tags.includes(t)) {
    updateSkillMeta(selected.value.key, m.starred, [...m.tags, t]);
  }
  tagInput.value = "";
}

function removeTag(t: string) {
  if (!selected.value) return;
  const m = skillMeta(selected.value.key);
  updateSkillMeta(selected.value.key, m.starred, m.tags.filter((x) => x !== t));
}

// ---------- 查看全文（SKILL.md 渲染 + 文件清单） ----------

const detail = ref<SkillDetail | null>(null);
const detailLoading = ref(false);
const secReport = ref<SecurityReport | null>(null);
watch(() => store.revision, async () => {
  void loadSecuritySummary();
  const key = detail.value?.key;
  if (!key || detailLoading.value) return;
  if (!store.skills.some(s => s.key === key)) { detail.value = null; return; }
  try {
    const [updated, security] = await Promise.all([readSkillDetail(key), scanSkillSecurity(key)]);
    if (detail.value?.key === key) { detail.value = updated; secReport.value = security; }
  } catch { /* 下次目录刷新时重试，保留当前只读预览。 */ }
});

async function openFull() {
  if (!selected.value || detailLoading.value) return;
  detailLoading.value = true;
  detail.value = null;
  secReport.value = null;
  try {
    const key = selected.value.key;
    const [d, s] = await Promise.all([readSkillDetail(key), scanSkillSecurity(key)]);
    detail.value = d;
    secReport.value = s;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    detailLoading.value = false;
  }
}

const mdHtml = computed(() =>
  detail.value ? (marked.parse(detail.value.content, { async: false }) as string) : ""
);

// ---------- 应用内回收站 ----------

const showTrash = ref(false);
const trash = ref<TrashItem[]>([]);
const trashLoading = ref(false);
const trashBusy = ref("");

function toggleTrash() {
  showTrash.value = !showTrash.value;
  if (showTrash.value) loadTrash();
}

async function loadTrash() {
  trashLoading.value = true;
  try {
    trash.value = await listTrash();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    trashLoading.value = false;
  }
}

async function doRestore(item: TrashItem) {
  if (trashBusy.value) return;
  trashBusy.value = item.name;
  try {
    const key = await restoreTrash(item.name);
    toast(`「${key}」已还原，重新部署后即可使用`, "success");
    await loadTrash();
    await loadAll();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    trashBusy.value = "";
  }
}

async function doPurge(item: TrashItem) {
  if (trashBusy.value) return;
  if (!confirm(`彻底删除「${item.key}」？将移入 Windows 回收站`)) return;
  trashBusy.value = item.name;
  try {
    await purgeTrash(item.name);
    await loadTrash();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    trashBusy.value = "";
  }
}

onMounted(() => {
  loadSecuritySummary().catch(() => {});
});
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>技能库</h1>
        <p class="sub">来源：{{ store.config?.skills_root }}</p>
      </div>
    </header>
    <ResourceTabs v-model="resourceTab" label="技能库来源视图" :items="[{id:'library',name:'技能库',count:store.skills.length},{id:'owned',name:'Agent 自有技能',count:store.ownedSkills.length}]" />
    <template v-if="resourceTab === 'library'">
    <div class="library-toolbar"><div class="library-view-switch" role="group" aria-label="技能库视图"><button :class="{active:ui.skillsView === 'list'}" @click="ui.skillsView = 'list'">列表</button><button :class="{active:ui.skillsView === 'cards'}" @click="ui.skillsView = 'cards'">卡片</button><button :class="{active:ui.skillsView === 'agents'}" @click="ui.skillsView = 'agents'">按 Agent</button></div><button class="btn" @click="openAgentSettings()">管理 Agent</button></div>
    <div class="library-layout">
    <SkillCategories v-model="activeCat" :skills="store.skills" />
    <section class="library-content" aria-label="分类技能列表">
    <div class="library-filters">
      <input v-model="keyword" class="input library-search" aria-label="搜索技能" placeholder="搜索名称 / 描述 / 文件夹…" />
      <select v-model="activeTag" class="input tag-filter" aria-label="筛选技能标签"><option value="">全部标签</option><option v-for="tag in tags" :key="tag" :value="tag">{{ tag }}</option></select>
      <button class="btn" :class="{ 'starred-only': showStarred }" :aria-pressed="showStarred" @click="showStarred = !showStarred">★ 仅收藏</button>
    </div>
    <div class="library-result"><span>{{ activeCat ? categoryLabel(activeCat) : '全部技能' }}</span><small>{{ filtered.length }} 个技能</small></div>
    <SkillAllocation v-if="ui.skillsView === 'list'" :skills="filtered" :filter-key="filterKey" @detail="openDetail" />
    <AgentConfig v-else-if="ui.skillsView === 'agents'" :skills="filtered" />
    <template v-else>

    <div class="grid">
      <div
        v-for="s in filtered"
        :key="s.key"
        class="card skill"
        @click="openDetail(s)"
      >
        <div class="skill-top">
          <SkillIcon :name="s.name" :identity="s.key" :description="s.description" :src="s.icon_b64" />
          <span class="skill-name-wrap">
            <span class="skill-name">{{ s.name }}</span>
            <span class="skill-slug" v-if="s.key !== s.name" :title="s.key">{{ s.key }}</span>
          </span>
          <span v-if="!s.healthy" class="badge red">异常</span>
          <button
            class="star-btn"
            :class="{ on: isStarred(s.key) }"
            :title="isStarred(s.key) ? '取消收藏' : '收藏'"
            @click.stop="toggleStar(s)"
          >★</button>
        </div>
        <div class="skill-desc">
          {{ showDesc(s.description) }}
          <button
            v-if="needsTranslation(s.description) && !transMap[s.description]"
            class="trans-btn"
            title="翻译成中文"
            @click.stop="ensureTranslate(s.description)"
          >译</button>
        </div>
        <div class="skill-meta">
          <span class="badge gray">{{ categoryLabel(s.category) }}</span>
          <span class="meta-text">{{ formatBytes(s.size_bytes) }}</span>
          <span class="meta-text" v-if="s.has_scripts">scripts/</span>
          <span
            v-if="secLevels[s.key] && secLevels[s.key] !== 'none'"
            class="badge"
            :class="{
              red: secLevels[s.key] === 'high',
              amber: secLevels[s.key] === 'medium',
              blue: secLevels[s.key] === 'low'
            }"
          >
            风险·{{ secLevels[s.key] === 'high' ? '高' : secLevels[s.key] === 'medium' ? '中' : '低' }}
          </span>
          <button
            class="skill-del"
            title="删除技能（移入回收站）"
            aria-label="删除技能"
            @click.stop="askDelete(s)"
          >
            <svg viewBox="0 0 24 24" width="13" height="13" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round">
              <path d="M3 6h18M8 6V4h8v2M6 6l1 14h10l1-14M10 11v6M14 11v6" />
            </svg>
          </button>
        </div>
        <SkillAssignments :skill-key="s.key" />
      </div>
      <div v-if="!filtered.length" class="card empty" style="grid-column: 1/-1">
        <div class="icon">⌕</div>
        没有匹配的技能，换个关键词试试。
      </div>
    </div>

    </template>
    </section>
    </div>
    </template>
    <AgentOwnedSkills v-else />

    <!-- 详情弹窗 -->
    <div v-if="selected" class="modal-mask" @click.self="selected = null">
      <div class="modal">
        <div class="modal-head">
          <span class="detail-head">
            <SkillIcon :name="selected!.name" :identity="selected!.key" :description="selected!.description" :src="selected!.icon_b64" />
            <span class="detail-head-txt">
              <b style="font-size: 15px">{{ selected!.name }}</b>
              <code class="detail-key">{{ selected!.key }}</code>
            </span>
          </span>
          <button class="btn ghost" @click="selected = null">✕</button>
        </div>
        <div class="modal-body">
          <div class="detail-row">
            <span class="lbl">描述</span>
            <span>{{ showDesc(selected!.description) }}</span>
          </div>
          <div class="detail-row">
            <span class="lbl">路径</span>
            <code class="path">{{ selected!.path }}</code>
          </div>
          <div class="detail-row">
            <span class="lbl">大小 / 修改</span>
            <span>{{ formatBytes(selected!.size_bytes) }} · {{ formatTs(selected!.modified_ts) }}</span>
          </div>
          <div class="detail-row">
            <span class="lbl">健康</span>
            <span :style="{ color: selected!.healthy ? 'var(--green)' : 'var(--red)' }">
              {{ selected!.healthy ? "SKILL.md 合法" : "SKILL.md 缺失或 frontmatter 不合法" }}
            </span>
          </div>
          <div class="detail-row" v-if="selected!.version">
            <span class="lbl">版本</span>
            <span>{{ selected!.version }}</span>
          </div>
          <div class="detail-row">
            <span class="lbl">标签</span>
            <span class="tag-editor">
              <span v-for="t in skillMeta(selected!.key).tags" :key="t" class="tag-chip">
                {{ t }}
                <button class="tag-del" @click="removeTag(t)">✕</button>
              </span>
              <input
                v-model="tagInput"
                class="tag-input"
                placeholder="加标签，回车确认"
                @keyup.enter="addTag()"
              />
            </span>
          </div>

          <h3 style="margin: 16px 0 10px; font-size: 13.5px; color: var(--muted)">分配到 Agent · 点击图标启停</h3>
          <div v-if="!store.agents.length" class="empty" style="padding:12px">请先在设置中允许配置本机工具。<button class="btn" @click="selected = null; openAgentSettings()">管理工具 →</button></div>
          <SkillAssignments :skill-key="selected.key" />
        </div>
        <div class="modal-foot">
          <button class="btn" style="margin-right: auto" :disabled="detailLoading" @click="openFull()">
            {{ detailLoading ? "读取中…" : "📄 查看全文" }}
          </button>
          <button class="btn danger" @click="confirming = selected">移入回收站</button>
          <button class="btn" @click="selected = null">关闭</button>
        </div>
      </div>
    </div>

    <!-- 删除确认弹窗 -->
    <div v-if="confirming" class="modal-mask" @click.self="deleting ? undefined : (confirming = null)">
      <div class="modal" style="max-width: 440px">
        <div class="modal-head">
          <b style="font-size: 15px">删除技能</b>
          <button class="btn ghost" :disabled="deleting" @click="confirming = null">✕</button>
        </div>
        <div class="modal-body">
          <p class="del-title">即将把「{{ confirming.name }}」移入 Windows 回收站</p>
          <code class="path">{{ confirming.path }}</code>
          <ul class="del-list">
            <li>清理已部署到各 Agent 的目录链接</li>
            <li>从 {{ assignedCount(confirming.key) }} 个 Agent 的配置中摘除</li>
            <li>移入回收站后可随时还原</li>
          </ul>
        </div>
        <div class="modal-foot">
          <button class="btn danger" :disabled="deleting" @click="doDelete()">
            {{ deleting ? "正在删除…" : "确认删除" }}
          </button>
          <button class="btn" :disabled="deleting" @click="confirming = null">取消</button>
        </div>
      </div>
    </div>
    <!-- 全文预览弹窗 -->
    <div v-if="detail" class="modal-mask" @click.self="detail = null">
      <div class="modal" style="max-width: 820px">
        <div class="modal-head">
          <b>📄 {{ detail.key }}</b>
          <button class="btn ghost" @click="detail = null">✕</button>
        </div>
        <div class="modal-body full-body">
          <div v-if="detail.risks.length" class="risk-box">
            <b>⚠ 安全提示（{{ detail.risks.length }}）</b>
            <ul>
              <li v-for="r in detail.risks" :key="r">{{ r }}</li>
            </ul>
          </div>
          <!-- 内容级安全扫描 -->
          <div v-if="secReport && secReport.level !== 'none'" class="risk-box">
            <b>🛡 内容安全扫描（{{ secReport.findings.length }} 条命中）</b>
            <div style="margin-top: 6px; display: flex; align-items: center; gap: 8px; flex-wrap: wrap">
              <span class="badge" :class="{ red: secReport.level === 'high', amber: secReport.level === 'medium', blue: secReport.level === 'low' }">
                {{ secReport.level === 'high' ? '高风险' : secReport.level === 'medium' ? '中风险' : '低风险' }}
              </span>
              <span style="font-size: 12px; color: var(--muted)">扫描 {{ secReport.scanned_files }} 个文件</span>
            </div>
            <div v-for="f in secReport.findings.slice(0, 30)" :key="f.file + f.line" class="finding-row">
              <code>{{ f.file }}{{ f.line > 0 ? ':' + f.line : '' }}</code>
              <span class="badge" :class="{ red: f.level === 'high', amber: f.level === 'medium', blue: f.level === 'low' }" style="margin-left: 6px; font-size: 10px">{{ f.category }}</span>
              <span style="margin-left: 6px; color: var(--muted)">{{ f.pattern }}</span>
            </div>
          </div>
          <div class="md-view" v-html="mdHtml"></div>
          <h3 class="files-title">文件清单（{{ detail.files.length }}）</h3>
          <div class="files-list">
            <div v-for="f in detail.files" :key="f.name" class="file-row" :class="{ risky: f.risky }">
              <span class="file-name">{{ f.is_dir ? "📁" : "📄" }} {{ f.name }}</span>
              <span class="file-size">{{ f.is_dir ? "目录" : formatBytes(f.size_bytes) }}</span>
            </div>
          </div>
        </div>
        <div class="modal-foot">
          <button class="btn" @click="detail = null">关闭</button>
        </div>
      </div>
    </div>

    <!-- 应用内回收站 -->
    <div v-if="resourceTab === 'library'" class="trash-section">
      <button class="trash-toggle" @click="toggleTrash">
        🗑 应用内回收站
        <span v-if="trash.length" class="cat-count">{{ trash.length }}</span>
        <span class="trash-arrow">{{ showTrash ? "▲" : "▼" }}</span>
      </button>
      <div v-if="showTrash" class="card trash-list">
        <div v-if="trashLoading" class="empty" style="padding: 18px">读取中…</div>
        <div v-else-if="!trash.length" class="empty" style="padding: 18px">
          回收站是空的，删除的技能会先放在这里，可随时还原。
        </div>
        <div v-else class="trash-items">
          <div class="trash-row" v-for="t in trash" :key="t.name">
            <span class="trash-key">{{ t.key }}</span>
            <span class="trash-meta">{{ formatBytes(t.size_bytes) }} · 删除于 {{ formatTs(t.deleted_ts) }}</span>
            <span class="trash-actions">
              <button class="btn primary" :disabled="trashBusy === t.name" @click="doRestore(t)">
                {{ trashBusy === t.name ? "还原中…" : "还原" }}
              </button>
              <button class="btn danger" :disabled="trashBusy === t.name" @click="doPurge(t)">彻底删除</button>
            </span>
          </div>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.library-layout { display:grid; grid-template-columns:180px minmax(0,1fr); align-items:start; gap:22px; }
.library-content { min-width:0; }
.library-filters { display:flex; align-items:center; gap:9px; flex-wrap:wrap; margin-bottom:14px; }
.library-search { flex:1; min-width:180px; }
.tag-filter { width:140px; }
.starred-only { color:var(--amber); background:var(--amber-soft); border-color:var(--amber); }
.library-result { display:flex; align-items:baseline; gap:10px; margin-bottom:22px; }
.library-result > span { font-size:15px; font-weight:600; }
.library-result small { color:var(--faint); font-size:11px; }
@media (max-width:1000px) { .library-layout { grid-template-columns:minmax(0,1fr); gap:18px; } }
.library-toolbar { display:flex; align-items:center; justify-content:space-between; gap:16px; margin-bottom:20px; }
.library-view-switch { display:flex; padding:3px; gap:3px; background:var(--panel-2); border:1px solid var(--border-soft); border-radius:8px; }
.library-view-switch button { padding:6px 16px; border:1px solid transparent; border-radius:5px; color:var(--muted); background:none; font:inherit; cursor:pointer; font-size:12px; }
.library-view-switch button.active { background:var(--panel); border-color:var(--border); color:var(--text); box-shadow:var(--shadow-sm); }
.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 16px;
  gap: 16px;
}
h1 {
  font-size: 26px;
  font-weight: 650;
}
.sub {
  color: var(--muted);
  font-size: 12.5px;
  margin-top: 2px;
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 16px;
}
.skill {
  cursor: pointer;
  transition: all 0.15s;
  display: flex;
  flex-direction: column;
  gap: 12px;
}
.skill:hover {
  border-color: var(--accent);
  box-shadow: var(--shadow);
}
.skill-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}
.skill-name-wrap {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 1px;
}
.skill-name {
  font-weight: 600;
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
/* 中文名下的英文 slug 小字副标题 */
.skill-slug {
  font-size: 10px;
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.skill-desc {
  color: var(--muted);
  font-size: 12.5px;
  min-height: 38px;
  display: -webkit-box;
  -webkit-line-clamp: 2;
  -webkit-box-orient: vertical;
  overflow: hidden;
}
.skill-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: auto;
}
.meta-text {
  color: var(--faint);
  font-size: 11.5px;
}

/* 卡片上的删除按钮：图标化，默认隐藏，悬浮显示 */
.skill-del {
  margin-left: auto;
  width: 24px;
  height: 24px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  border-radius: 7px;
  border: 1px solid transparent;
  background: transparent;
  color: var(--red);
  cursor: pointer;
  opacity: 0.35;
  transition: all 0.15s;
}
.skill:hover .skill-del,
.skill-del:focus-visible {
  opacity: 1;
}
.skill-del:hover {
  background: rgba(239, 68, 68, 0.1);
  border-color: var(--red);
}

/* 删除确认弹窗 */
.del-title {
  font-size: 13.5px;
  margin-bottom: 10px;
}
.del-list {
  margin: 12px 0 0 0;
  padding-left: 18px;
  color: var(--muted);
  font-size: 12.5px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.detail-head {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}
.detail-head-txt {
  display: flex;
  flex-direction: column;
  gap: 2px;
  min-width: 0;
}
.detail-key {
  font-size: 11px;
  color: var(--faint);
  background: var(--bg-soft);
  padding: 1px 7px;
  border-radius: 5px;
  align-self: flex-start;
  word-break: break-all;
}
.detail-row {
  display: flex;
  gap: 12px;
  padding: 7px 0;
  font-size: 13px;
  border-bottom: 1px dashed var(--border-soft);
}
.detail-row .lbl {
  color: var(--faint);
  flex-shrink: 0;
  width: 52px;
}
.path {
  font-size: 12px;
  background: var(--bg-soft);
  padding: 2px 8px;
  border-radius: 5px;
  color: var(--accent);
  word-break: break-all;
}

.assign-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

/* 收藏星标 */
.star-btn {
  width: 22px;
  height: 22px;
  border: none;
  background: transparent;
  color: var(--border);
  font-size: 14px;
  cursor: pointer;
  flex-shrink: 0;
  padding: 0;
  transition: all 0.15s;
}
.star-btn:hover {
  color: #facc15;
  transform: scale(1.15);
}
.star-btn.on {
  color: #facc15;
}

/* 标签编辑 */
.tag-editor {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
  align-items: center;
}
.tag-chip {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 1px 8px;
  border-radius: 10px;
  background: rgba(124, 92, 252, 0.1);
  border: 1px solid rgba(124, 92, 252, 0.22);
  color: #a78bfa;
  font-size: 11px;
}
.tag-del {
  border: none;
  background: transparent;
  color: inherit;
  font-size: 10px;
  cursor: pointer;
  padding: 0;
}
.tag-input {
  width: 110px;
  padding: 2px 8px;
  border-radius: 10px;
  border: 1px solid var(--border);
  background: var(--bg-soft);
  font-size: 11px;
  font-family: inherit;
  color: var(--text);
  outline: none;
}
.tag-input:focus {
  border-color: var(--accent);
}

/* 全文预览 */
.full-body {
  display: flex;
  flex-direction: column;
  gap: 14px;
}
.risk-box {
  background: rgba(245, 158, 11, 0.08);
  border: 1px solid rgba(245, 158, 11, 0.35);
  border-radius: var(--radius-sm);
  padding: 10px 14px;
  font-size: 12.5px;
  color: #d97706;
}
.risk-box ul {
  margin: 6px 0 0 0;
  padding-left: 18px;
  display: flex;
  flex-direction: column;
  gap: 3px;
}
.md-view {
  font-size: 13px;
  line-height: 1.75;
  color: var(--text);
  word-break: break-word;
}
.md-view :deep(h1),
.md-view :deep(h2),
.md-view :deep(h3) {
  margin: 14px 0 8px;
  font-size: 15px;
  border-bottom: 1px solid var(--border-soft);
  padding-bottom: 4px;
}
.md-view :deep(p) {
  margin: 8px 0;
}
.md-view :deep(code) {
  background: var(--bg-soft);
  padding: 1px 6px;
  border-radius: 4px;
  font-size: 12px;
}
.md-view :deep(pre) {
  background: var(--bg-soft);
  padding: 10px 12px;
  border-radius: 8px;
  overflow-x: auto;
}
.md-view :deep(pre code) {
  background: transparent;
  padding: 0;
}
.md-view :deep(ul),
.md-view :deep(ol) {
  padding-left: 20px;
  margin: 8px 0;
}
.md-view :deep(table) {
  border-collapse: collapse;
  margin: 8px 0;
}
.md-view :deep(th),
.md-view :deep(td) {
  border: 1px solid var(--border);
  padding: 4px 10px;
  font-size: 12px;
}
.files-title {
  font-size: 13.5px;
  color: var(--muted);
}
.files-list {
  display: flex;
  flex-direction: column;
  gap: 2px;
}
.file-row {
  display: flex;
  justify-content: space-between;
  gap: 10px;
  font-size: 12px;
  padding: 4px 10px;
  border-radius: 6px;
  background: var(--bg-soft);
}
.file-row.risky {
  background: rgba(245, 158, 11, 0.1);
  color: #d97706;
}
.file-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.file-size {
  color: var(--faint);
  flex-shrink: 0;
}

/* 应用内回收站 */
.trash-section {
  margin-top: 26px;
}
.trash-toggle {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 7px 14px;
  border-radius: 10px;
  border: 1px dashed var(--border);
  background: transparent;
  color: var(--muted);
  font-size: 12.5px;
  font-family: inherit;
  cursor: pointer;
  transition: all 0.15s;
}
.trash-toggle:hover {
  color: var(--text);
  border-color: var(--accent);
}
.trash-arrow {
  font-size: 10px;
}
.trash-list {
  margin-top: 10px;
  padding: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.trash-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 8px 10px;
  border-radius: var(--radius-sm);
  background: var(--bg-soft);
}
.trash-key {
  font-weight: 600;
  font-size: 13px;
}
.trash-meta {
  flex: 1;
  color: var(--faint);
  font-size: 11.5px;
}
.trash-actions {
  display: flex;
  gap: 8px;
}

/* 全文预览安全扫描命中行 */
.finding-row {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  padding: 3px 0;
  border-bottom: 1px solid var(--border-soft);
}
.finding-row code {
  font-size: 11px;
  color: var(--faint);
  background: var(--bg-soft);
  padding: 0 5px;
  border-radius: 4px;
  flex: 1;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
