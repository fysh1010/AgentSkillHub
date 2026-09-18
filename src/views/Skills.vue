<script setup lang="ts">
import { computed, ref } from "vue";
import { marked } from "marked";
import {
  store,
  loadAll,
  isEnabled,
  toggleSkill,
  deleteSkill,
  toast,
  ensureTranslate,
  needsTranslation,
  transMap,
  skillMeta,
  updateSkillMeta,
} from "../store";
import {
  formatBytes,
  formatTs,
  readSkillDetail,
  listTrash,
  restoreTrash,
  purgeTrash,
} from "../api/tauri";
import type { SkillInfo, SkillDetail, TrashItem } from "../types";

const keyword = ref("");
const activeCat = ref("全部");
const selected = ref<SkillInfo | null>(null);

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

const categories = computed(() => {
  const set = new Set<string>();
  store.skills.forEach((s) => set.add(s.category));
  const cats = Array.from(set);
  // 点开头与下划线开头的分类排后
  cats.sort((a, b) => {
    const ap = a.startsWith(".") || a.startsWith("_") ? 1 : 0;
    const bp = b.startsWith(".") || b.startsWith("_") ? 1 : 0;
    if (ap !== bp) return ap - bp;
    return a.localeCompare(b, "zh");
  });
  return ["全部", ...cats];
});

const filtered = computed(() => {
  const kw = keyword.value.trim().toLowerCase();
  return store.skills.filter((s) => {
    if (showStarred.value && !isStarred(s.key)) return false;
    if (activeCat.value !== "全部" && s.category !== activeCat.value) return false;
    if (!kw) return true;
    return (
      s.name.toLowerCase().includes(kw) ||
      s.description.toLowerCase().includes(kw) ||
      s.key.toLowerCase().includes(kw)
    );
  });
});

function catCount(cat: string) {
  if (cat === "全部") return store.skills.length;
  return store.skills.filter((s) => s.category === cat).length;
}

// ---------- 技能图标：官方图优先，emoji 渐变兜底 ----------

const SKILL_EMOJI: Array<[RegExp, string]> = [
  [/browser|web|http|html|url|page|chrome/i, "🌐"],
  [/pdf|document|file|doc|markdown/i, "📄"],
  [/video|yt|youtube|bilibili|ffmpeg|subtitle|screen/i, "🎬"],
  [/audio|tts|voice|speech|music|podcast/i, "🎵"],
  [/image|photo|picture|screenshot|vision|ocr/i, "🖼️"],
  [/search|fetch|scrape|lookup|spider/i, "🔍"],
  [/excel|sheet|csv|data|table|sql|database|analytics/i, "📊"],
  [/word|office|text|write|draft|letter/i, "✍️"],
  [/translate|language|i18n|locale/i, "🌏"],
  [/design|ui|ux|art|style|brand|figma/i, "🎨"],
  [/code|api|sdk|terminal|dev|git|github/i, "💻"],
  [/agent|bot|llm|ai\b|gpt|claude|model/i, "🤖"],
  [/security|privacy|auth|password/i, "🛡️"],
  [/calendar|schedule|time|meeting|remind/i, "📅"],
  [/mail|email|message|chat|im\b|wechat/i, "💬"],
  [/game|play|unity/i, "🎮"],
  [/tool|utils|utility|helper/i, "🧰"],
];

function skillEmoji(s: SkillInfo): string {
  const hay = `${s.name} ${s.category} ${s.description}`;
  for (const [re, e] of SKILL_EMOJI) {
    if (re.test(hay)) return e;
  }
  return "🧩";
}

const SKILL_GRADS = [
  "linear-gradient(135deg,#7c5cff,#b16cff)",
  "linear-gradient(135deg,#ff6b9d,#ff9a76)",
  "linear-gradient(135deg,#00b8d9,#3ddc97)",
  "linear-gradient(135deg,#f59e0b,#ef4444)",
  "linear-gradient(135deg,#06b6d4,#3b82f6)",
  "linear-gradient(135deg,#8b5cf6,#ec4899)",
];

function skillGrad(s: SkillInfo): string {
  let idx = 0;
  for (const ch of s.key) idx = (idx + ch.charCodeAt(0)) % 997;
  return SKILL_GRADS[idx % SKILL_GRADS.length];
}

/// 官方图标（技能目录内自带）→ emoji 兜底；官方图加载失败降级为 emoji
function onIconError(s: SkillInfo) {
  s.icon_b64 = "";
}

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

async function openFull() {
  if (!selected.value || detailLoading.value) return;
  detailLoading.value = true;
  detail.value = null;
  try {
    detail.value = await readSkillDetail(selected.value.key);
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
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>技能库</h1>
        <p class="sub">来源：{{ store.config?.skills_root }}</p>
      </div>
      <input v-model="keyword" class="input search" placeholder="搜索技能名称 / 描述 / 分类…" />
    </header>

    <div class="cats" v-if="categories.length > 2">
      <button class="cat" :class="{ active: showStarred }" @click="showStarred = !showStarred">
        ★ 收藏 <span class="cat-count">{{ store.skills.filter((s) => isStarred(s.key)).length }}</span>
      </button>
      <button
        v-for="c in categories"
        :key="c"
        class="cat"
        :class="{ active: activeCat === c }"
        @click="activeCat = c"
      >
        {{ c === "(根)" ? "全部技能" : c }} <span class="cat-count">{{ catCount(c) }}</span>
      </button>
    </div>

    <div class="grid">
      <div
        v-for="s in filtered"
        :key="s.key"
        class="card skill"
        @click="openDetail(s)"
      >
        <div class="skill-top">
          <span class="skill-ico" :style="s.icon_b64 ? undefined : { background: skillGrad(s) }">
            <img
              v-if="s.icon_b64"
              class="skill-ico-img"
              :src="s.icon_b64"
              alt=""
              loading="lazy"
              @error="onIconError(s)"
            />
            <template v-else>{{ skillEmoji(s) }}</template>
          </span>
          <span class="skill-name-wrap">
            <span class="skill-name">{{ s.name }}</span>
            <span class="skill-slug" v-if="s.key !== s.name" :title="s.key">{{ s.key }}</span>
          </span>
          <span class="badge" :class="s.healthy ? 'green' : 'red'">
            {{ s.healthy ? "OK" : "异常" }}
          </span>
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
          <span class="badge gray">{{ s.category }}</span>
          <span class="meta-text">{{ formatBytes(s.size_bytes) }}</span>
          <span class="meta-text" v-if="s.has_scripts">scripts/</span>
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
      </div>
      <div v-if="!filtered.length" class="card empty" style="grid-column: 1/-1">
        <div class="icon">⌕</div>
        没有匹配的技能，换个关键词试试。
      </div>
    </div>

    <!-- 详情弹窗 -->
    <div v-if="selected" class="modal-mask" @click.self="selected = null">
      <div class="modal">
        <div class="modal-head">
          <span class="detail-head">
            <span
              class="skill-ico"
              :style="selected!.icon_b64 ? undefined : { background: skillGrad(selected!) }"
            >
              <img
                v-if="selected!.icon_b64"
                class="skill-ico-img"
                :src="selected!.icon_b64"
                alt=""
                @error="onIconError(selected!)"
              />
              <template v-else>{{ skillEmoji(selected!) }}</template>
            </span>
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

          <h3 style="margin: 16px 0 10px; font-size: 13.5px; color: var(--muted)">已分配给以下 Agent</h3>
          <div class="assign-list">
            <label v-for="a in store.agents" :key="a.agent.id" class="assign-item">
              <span>{{ a.agent.name }}</span>
              <span class="switch">
                <input
                  type="checkbox"
                  :checked="isEnabled(a.agent.id, selected!.key)"
                  @change="(e: any) => toggleSkill(a.agent.id, selected!.key, (e.target as HTMLInputElement).checked)"
                />
                <span class="track"></span>
              </span>
            </label>
          </div>
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
    <div class="trash-section">
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
.page-head {
  display: flex;
  align-items: flex-end;
  justify-content: space-between;
  margin-bottom: 16px;
  gap: 16px;
}
h1 {
  font-size: 21px;
  font-weight: 700;
}
.sub {
  color: var(--muted);
  font-size: 12.5px;
  margin-top: 2px;
}
.search {
  width: 260px;
}

.cats {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 16px;
}
.cat {
  padding: 5px 12px;
  border-radius: 20px;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--muted);
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s;
}
.cat:hover {
  border-color: var(--accent);
  color: var(--text);
}
.cat.active {
  background: linear-gradient(135deg, var(--accent), var(--accent-2));
  border-color: transparent;
  color: #fff;
  font-weight: 600;
}
.cat-count {
  opacity: 0.7;
  font-size: 11px;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
  gap: 12px;
}
.skill {
  cursor: pointer;
  transition: all 0.15s;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.skill:hover {
  border-color: var(--accent);
  transform: translateY(-2px);
  box-shadow: var(--shadow);
}
.skill-ico {
  width: 34px;
  height: 34px;
  border-radius: 10px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 17px;
  color: #fff;
  flex-shrink: 0;
  box-shadow: 0 3px 8px rgba(0, 0, 0, 0.25);
  overflow: hidden;
}
/* 展示官方原图：白底、去投影，避免自绘风格干扰官方视觉 */
.skill-ico:has(.skill-ico-img) {
  background: #fff;
  border: 1px solid var(--border-soft);
  box-shadow: none;
}
.skill-ico-img {
  width: 100%;
  height: 100%;
  object-fit: contain;
  display: block;
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
  opacity: 0;
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
.assign-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 12px;
  border-radius: var(--radius-sm);
  background: var(--bg-soft);
  cursor: pointer;
  font-size: 13px;
}
.assign-item:hover {
  background: var(--panel-2);
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
