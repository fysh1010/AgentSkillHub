<script setup lang="ts">
import { onMounted, ref, computed } from "vue";
import { store, loadAll, toast, toggleSkill, ensureTranslate, needsTranslation, transMap } from "../store";
import {
  marketSources,
  marketList,
  marketSkillDetail,
  marketInstall,
  marketAddSource,
  marketRemoveSource,
  ensureSkillIcon,
  deployAgent,
  formatTs,
  scanSkillSecurity,
  formatBytes,
} from "../api/tauri";
import type { MarketSource, MarketSkill, MarketSkillDetail, SecurityReport } from "../types";
import { useEsc } from "../composables/useEsc";
import SkillIcon from "../components/SkillIcon.vue";

// Esc 关闭「安装后启用」弹窗（部署进行中不响应，避免误关）
useEsc(() => {
  if (showEnable.value && !enabling.value) showEnable.value = false;
  else if (selected.value) closeDetail();
});

const sources = ref<MarketSource[]>([]);
const activeSource = ref("");
const query = ref("");
const searchInput = ref("");
const skills = ref<MarketSkill[]>([]);
const loading = ref(false);
const error = ref("");
const installing = ref("");
const showAddSource = ref(false);
const newSource = ref({ name: "", repo: "", subpath: "" });
const selected = ref<MarketSkill | null>(null);
const detail = ref<MarketSkillDetail | null>(null);
const detailLoading = ref(false);
const detailError = ref("");
const detailTab = ref<"content" | "files">("content");
const detailCache = new Map<string, MarketSkillDetail>();
let detailRequest = 0;

function closeDetail() {
  detailRequest++;
  selected.value = null;
  detailLoading.value = false;
}

async function openDetail(skill: MarketSkill) {
  selected.value = skill;
  detail.value = null;
  detailError.value = "";
  detailLoading.value = false;
  detailTab.value = "content";
  const request = ++detailRequest;
  if (skill.source_kind !== "skillhub") return;
  const key = `${skill.reference}@${skill.version}`;
  const cached = detailCache.get(key);
  if (cached && !cached.preview_error) { detail.value = cached; return; }
  detailLoading.value = true;
  try {
    const result = await marketSkillDetail(skill.reference);
    detailCache.set(key, result);
    if (request === detailRequest) detail.value = result;
  } catch (e) {
    if (request === detailRequest) detailError.value = String(e);
  } finally {
    if (request === detailRequest) detailLoading.value = false;
  }
}

function installFromDetail() {
  if (!selected.value) return;
  const skill = selected.value;
  closeDetail();
  void install(skill);
}

function updateFromDetail() {
  if (!selected.value) return;
  const skill = selected.value;
  closeDetail();
  void doUpdate(skill);
}

const activeKind = computed(
  () => sources.value.find((s) => s.id === activeSource.value)?.kind || ""
);

// ---------- 分类筛选 ----------

const activeCat = ref("全部");

/// 官方分类 key → 中文标签（兼容腾讯 SkillHub 与 ClawHub 两套分类体系）
const CATEGORY_LABELS: Record<string, string> = {
  "ai-agent": "AI Agent",
  "dev-programming": "开发编程",
  "office-efficiency": "办公效率",
  "business-ops": "商业运营",
  education: "教育学习",
  professional: "专业服务",
  "content-creation": "内容创作",
  "knowledge-management": "知识管理",
  "life-service": "生活服务",
  "data-analysis": "数据分析",
  "design-media": "设计媒体",
  knowledge: "知识",
  development: "开发",
  creative: "创意",
  productivity: "效率",
  github: "GitHub",
};

function catLabel(c: string): string {
  return c ? CATEGORY_LABELS[c] || c : "未分类";
}

/// ClawHub 推荐 feed 没有官方分类，用首个 topic 分组；常见 topic 给中文标签
const TOPIC_LABELS: Record<string, string> = {
  automation: "自动化",
  "ai-coding-tool": "AI 编程",
  "code-iteration": "代码迭代",
  workflow: "工作流",
  "claude-code": "Claude Code",
  search: "搜索",
  pdf: "PDF",
  browser: "浏览器",
  github: "GitHub",
  git: "Git",
  testing: "测试",
  security: "安全",
  documentation: "文档",
  design: "设计",
  writing: "写作",
  data: "数据",
  api: "API",
  database: "数据库",
  productivity: "效率",
  devops: "DevOps",
  monitoring: "监控",
  "mcp": "MCP",
};

function topicLabel(t: string): string {
  return TOPIC_LABELS[t.toLowerCase()] || t;
}

/// 当前结果集的分类分布（按技能数量降序）
const categories = computed(() => {
  const map = new Map<string, number>();
  skills.value.forEach((s) => {
    const c = s.category || "";
    if (c) map.set(c, (map.get(c) || 0) + 1);
  });
  return Array.from(map.entries()).sort((a, b) => b[1] - a[1]);
});

/// 应用分类筛选后的列表
const filteredSkills = computed(() => {
  if (activeCat.value === "全部") return skills.value;
  return skills.value.filter((s) => (s.category || "") === activeCat.value);
});

/// 分类分组的推荐区：每个分类一个区块，区内按下载量降序（推荐 = 各分类热门优先）
const grouped = computed(() => {
  const map = new Map<string, MarketSkill[]>();
  for (const s of filteredSkills.value) {
    const c = s.category || "";
    if (!map.has(c)) map.set(c, []);
    map.get(c)!.push(s);
  }
  return Array.from(map.entries())
    .map(([c, list]) => {
      list.sort((a, b) => b.downloads - a.downloads || b.stars - a.stars);
      return { cat: c, label: topicLabel(c), skills: list };
    })
    .sort((a, b) => b.skills.length - a.skills.length);
});

async function loadSources() {
  try {
    sources.value = await marketSources();
    const cfgSources = store.config?.market_sources || [];
    if (cfgSources.length) sources.value = cfgSources;
    if (!sources.value.find((s) => s.id === activeSource.value)) {
      // 默认选中腾讯源（排在最前）：官方中文描述 + 精选热门榜，首次加载最快
      const prefer = sources.value.find((s) => s.kind === "skillhub");
      activeSource.value = (prefer || sources.value[0])?.id || "";
    }
    await browse();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function browse() {
  if (!activeSource.value) return;
  loading.value = true;
  error.value = "";
  query.value = searchInput.value.trim();
  activeCat.value = "全部";
  page.value = 1;
  noMore.value = false;
  try {
    skills.value = await marketList(activeSource.value, query.value, 1);
    // 已安装但缺本地图标的技能，后台补官方图标（落盘后刷新技能库展示）
    backfillIcons();
  } catch (e) {
    skills.value = [];
    error.value = String(e);
  } finally {
    loading.value = false;
  }
}

/** 已安装但技能目录里没有官方图标的，逐个补图（市场卡片显示的那张：官方图优先，发布者头像兜底） */
function backfillIcons() {
  const todo = skills.value.filter((s) => s.installed && officialImg(s));
  if (!todo.length) return;
  let saved = 0;
  Promise.all(
    todo.map((s) =>
      ensureSkillIcon(s.name, officialImg(s))
        .then((r) => {
          if (r !== "exists") saved += 1;
        })
        .catch(() => {})
    )
  ).then(() => {
    if (saved) {
      toast(`已为 ${saved} 个技能补上官方图标`, "success");
      loadAll();
    }
  });
}

// ---------- 分页 + 无限滚动 ----------

const page = ref(1);
const CLAWHUB_MAX_PAGES = 6;
const loadingMore = ref(false);
const noMore = ref(false);

/// 能否继续翻页：只有 ClawHub 推荐支持（cursor 懒加载，上限 6 页）。
/// 腾讯公开接口不支持翻页（page/offset 均被忽略），固定返回 60 条精选。
const canLoadMore = computed(() => {
  if (loading.value || noMore.value) return false;
  return activeKind.value === "clawhub" && !query.value && page.value < CLAWHUB_MAX_PAGES;
});

/** 跨页去重（ClawHub 两个排序维度的流可能重复出现同一技能） */
function appendDeduped(more: MarketSkill[]) {
  const seen = new Set(skills.value.map((s) => s.name));
  skills.value = [...skills.value, ...more.filter((s) => !seen.has(s.name))];
}

async function loadMore() {
  if (loadingMore.value || !activeSource.value || !canLoadMore.value) return;
  loadingMore.value = true;
  try {
    const next = page.value + 1;
    const more = await marketList(activeSource.value, query.value, next);
    page.value = next;
    if (!more.length) {
      noMore.value = true;
      return;
    }
    appendDeduped(more);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    loadingMore.value = false;
  }
}

// 滚动到底自动加载：哨兵元素进入视口即触发
const sentinel = ref<HTMLElement | null>(null);
let observer: IntersectionObserver | null = null;

onMounted(() => {
  observer = new IntersectionObserver(
    (entries) => {
      if (entries.some((e) => e.isIntersecting)) loadMore();
    },
    { rootMargin: "400px" }
  );
  observer.observe(sentinel.value!);
});

async function install(s: MarketSkill) {
  if (installing.value) return;
  installing.value = s.name;
  try {
    await marketInstall(activeSource.value, s.reference, s.owner, officialImg(s));
    toast(
        s.name === s.display_name
          ? `「${s.display_name}」已安装到技能库`
          : `「${s.display_name}」已安装到技能库（目录：${s.name}）`,
        "success"
    );
    await loadAll();
    s.installed = true;
    // 安装成功后引导启用到 Agent
    pendingSkill.value = s;
    enableAgents.value = [];
    showEnable.value = true;
    // 安装到本地后可扫描内容安全
    scanSkillSecurity(s.name).then((r) => {
      secReport.value = r;
    }).catch(() => {});
  } catch (e) {
    toast(String(e), "error");
  } finally {
    installing.value = "";
  }
}

// ---------- 安装后一键启用到 Agent ----------

const showEnable = ref(false);
const pendingSkill = ref<MarketSkill | null>(null);
const enableAgents = ref<string[]>([]);
const enabling = ref(false);
const secReport = ref<SecurityReport | null>(null);

function toggleEnableAgent(id: string) {
  const i = enableAgents.value.indexOf(id);
  if (i >= 0) enableAgents.value.splice(i, 1);
  else enableAgents.value.push(id);
}

async function confirmEnable() {
  if (!pendingSkill.value || enabling.value) return;
  const slug = pendingSkill.value.name;
  enabling.value = true;
  try {
    for (const id of enableAgents.value) {
      await toggleSkill(id, slug, true);
      await deployAgent(id, false);
    }
    toast(
      enableAgents.value.length
        ? `已启用到 ${enableAgents.value.length} 个 Agent`
        : "已跳过启用，稍后可在 Agent 配置中手动勾选",
      "success"
    );
    showEnable.value = false;
    pendingSkill.value = null;
    await loadAll();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    enabling.value = false;
  }
}

async function addSource() {
  try {
    await marketAddSource(newSource.value.name, newSource.value.repo, newSource.value.subpath);
    await loadAll();
    newSource.value = { name: "", repo: "", subpath: "" };
    showAddSource.value = false;
    toast("市场源已添加", "success");
    await loadSources();
  } catch (e) {
    toast(String(e), "error");
  }
}

async function removeSource(s: MarketSource) {
  if (!confirm(`删除市场源「${s.name}」？`)) return;
  try {
    await marketRemoveSource(s.id);
    await loadAll();
    toast("已删除", "success");
    await loadSources();
  } catch (e) {
    toast(String(e), "error");
  }
}

function fmtDownloads(n: number): string {
  if (n >= 1000) return `${(n / 1000).toFixed(1)}k`;
  return String(n);
}

// ---------- 英文描述翻译 ----------

function showDesc(d: string): string {
  if (!needsTranslation(d)) return d || "（无描述）";
  return transMap[d] || d;
}

function fmtDate(ts: number): string {
  if (!ts) return "";
  const d = new Date(ts * 1000);
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(
    d.getDate()
  ).padStart(2, "0")}`;
}

// ---------- 图标 / 美化工具 ----------

// 官方图标优先级：技能官方图标 → 作者头像 → 自动分类图标。
function officialImg(s: MarketSkill): string {
  return s.icon_url || s.owner_image || "";
}

/// 热门标记：ClawHub 用官方 featured，其余按下载量判定
function isHot(s: MarketSkill): boolean {
  return s.downloads >= 50000;
}

// ---------- 更新检测 ----------

/** 本地已装技能的版本号 */
function localVersion(slug: string): string {
  const s = store.skills.find((x) => x.key === slug || x.key.endsWith("/" + slug));
  return s?.version || "";
}

/** 语义化版本比较：latest 是否比 current 新 */
function isNewer(latest: string, current: string): boolean {
  const parse = (v: string) =>
    v
      .replace(/^v/i, "")
      .split(/[.-]/)
      .map((x) => parseInt(x, 10) || 0);
  const a = parse(latest);
  const b = parse(current);
  for (let i = 0; i < Math.max(a.length, b.length); i++) {
    const x = a[i] || 0;
    const y = b[i] || 0;
    if (x !== y) return x > y;
  }
  return false;
}

function hasUpdate(s: MarketSkill): boolean {
  if (!s.installed || !s.version) return false;
  const cur = localVersion(s.name);
  if (!cur) return false;
  return isNewer(s.version, cur);
}

const updating = ref("");

async function doUpdate(s: MarketSkill) {
  if (updating.value) return;
  if (!confirm(`更新「${s.display_name}」到 ${s.version}？\n当前版本会先移入应用内回收站，更新失败可还原。`)) return;
  updating.value = s.name;
  try {
    await marketInstall(activeSource.value, s.reference, s.owner, officialImg(s), true);
    toast(`「${s.display_name}」已更新到 ${s.version}`, "success");
    await loadAll();
  } catch (e) {
    toast(String(e), "error");
  } finally {
    updating.value = "";
  }
}

/** 待启用技能的风险提示 */
const enableRisk = computed(() => {
  const slug = pendingSkill.value?.name;
  if (!slug) return "";
  const s = store.skills.find((x) => x.key === slug);
  if (s?.has_scripts) return "该技能包含 scripts/ 可执行脚本，建议先在技能库「查看全文」确认内容再启用";
  return "";
});

function sourceLabel(s: MarketSkill): string {
  if (s.source_kind === "skillhub") return "腾讯";
  if (s.source_kind === "clawhub") return "ClawHub";
  if (s.source_kind === "github") return "GitHub";
  return s.source_kind || "市场";
}

function sourcePillMark(s: MarketSource): string {
  if (s.kind === "skillhub") return "🎯";
  if (s.kind === "clawhub") return "🐙";
  if (s.kind === "github") return "📦";
  return "";
}

onMounted(loadSources);
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>技能市场</h1>
        <p class="sub">
          从社区市场 / 腾讯 SkillHub / GitHub 仓库一键安装技能到技能库（{{
            store.config?.skills_root
          }}）
        </p>
      </div>
      <button class="btn" @click="showAddSource = !showAddSource">
        {{ showAddSource ? "取消" : "+ 添加 GitHub 源" }}
      </button>
    </header>

    <!-- 新增源 -->
    <div v-if="showAddSource" class="card add-source">
      <input v-model="newSource.name" class="input" placeholder="显示名（如 Awesome Skills）" />
      <input v-model="newSource.repo" class="input" placeholder="GitHub 仓库（owner/repo）" />
      <input v-model="newSource.subpath" class="input" placeholder="技能子路径（可选，如 skills）" />
      <button class="btn primary" @click="addSource()">添加</button>
    </div>

    <!-- 源切换 -->
    <div class="sources">
      <button
        v-for="s in sources"
        :key="s.id"
        class="source-pill"
        :class="{ active: activeSource === s.id }"
        @click="activeSource = s.id; browse()"
      >
        <span class="pill-mark">{{ sourcePillMark(s) }}</span>
        {{ s.name }}
        <span v-if="s.kind === 'skillhub'" class="pill-official">官方</span>
        <span
          v-if="s.kind === 'github' && s.id !== 'clawhub'"
          class="src-del"
          title="删除此源"
          @click.stop="removeSource(s)"
        >✕</span>
      </button>
    </div>

    <!-- 搜索（clawhub / skillhub 支持关键词；github 留空列全部） -->
    <div class="search-bar">
      <input
        v-model="searchInput"
        class="input"
        :placeholder="
          activeKind === 'github'
            ? 'GitHub 源直接列出仓库内全部技能，无需搜索'
            : activeKind === 'clawhub'
              ? '搜索 ClawHub 技能（留空浏览社区推荐 feed）'
              : '搜索技能关键词，如 screenshot、pdf、design…（留空即浏览下载量热门榜）'
        "
        @keyup.enter="browse()"
      />
      <button class="btn primary" @click="browse()" :disabled="loading">
        {{ loading ? "加载中…" : "搜索" }}
      </button>
    </div>

    <!-- 分类筛选（基于当前结果集动态生成） -->
    <div v-if="categories.length > 1" class="cats">
      <button class="cat" :class="{ active: activeCat === '全部' }" @click="activeCat = '全部'">
        全部 <span class="cat-count">{{ skills.length }}</span>
      </button>
      <button
        v-for="[c, n] in categories"
        :key="c"
        class="cat"
        :class="{ active: activeCat === c }"
        @click="activeCat = c"
      >
        {{ topicLabel(c) }} <span class="cat-count">{{ n }}</span>
      </button>
    </div>

    <div v-if="error" class="err-box">{{ error }}</div>

    <!-- 技能卡片：按分类分组展示，区内热门优先 -->
    <template v-for="g in grouped" :key="g.cat">
      <div class="group-head">
        <span class="group-title">{{ g.label }}</span>
        <span class="group-count">{{ g.skills.length }} 个技能</span>
        <span class="group-line"></span>
      </div>
      <div class="grid">
      <div
        v-for="s in g.skills"
        :key="s.reference + s.name"
        class="card mk"
        :class="{ featured: s.featured }"
        tabindex="0"
        :aria-label="`查看 ${s.display_name} 的技能详情`"
        @click="openDetail(s)"
        @keydown.enter.self="openDetail(s)"
        @keydown.space.self.prevent="openDetail(s)"
      >
        <div class="mk-head">
          <SkillIcon class="mk-icon" :name="s.name + ' ' + s.display_name" :identity="s.name" :description="s.description" :src="officialImg(s)" :fallback-src="s.icon_url ? s.owner_image : ''" />
          <div class="mk-title">
            <div class="mk-name-row">
              <b class="mk-name" :title="s.name">{{ s.display_name }}</b>
              <span v-if="hasUpdate(s)" class="badge amber" title="市场有新版本">可更新</span>
              <span v-if="s.featured" class="badge gold" title="官方精选">精选</span>
              <span v-else-if="isHot(s)" class="badge hot" :title="`下载量 ${s.downloads}`">热门</span>
            </div>
            <div class="mk-sub">
              <span v-if="s.owner" class="mk-owner" :title="s.reference">
                <img
                  v-if="s.owner_image"
                  class="avatar"
                  :src="s.owner_image"
                  alt=""
                  @error="s.owner_image = ''"
                />
                @{{ s.owner }}
              </span>
              <span v-if="s.version" class="mk-version">{{ s.version }}</span>
              <span class="mk-src" :class="'src-' + (s.source_kind || 'other')">{{
                sourceLabel(s)
              }}</span>
            </div>
          </div>
        </div>

        <p class="mk-desc">
          {{ showDesc(s.description) }}
          <button
            v-if="needsTranslation(s.description) && !transMap[s.description]"
            class="trans-btn"
            title="翻译成中文"
            @click.stop="ensureTranslate(s.description)"
          >译</button>
        </p>

        <div v-if="s.topics && s.topics.length" class="mk-topics">
          <span v-for="t in s.topics.slice(0, 4)" :key="t" class="topic">{{ t }}</span>
        </div>

        <div class="mk-meta">
          <span class="meta-left">
            <span v-if="s.security === 'pass'" class="sec pass" title="已通过上游安全扫描">
              🛡️ 安全
            </span>
            <span v-else-if="s.security === 'warn'" class="sec warn" title="安全扫描有告警，请留意">
              ⚠️ 留意
            </span>
            <span v-if="s.downloads" class="meta-t" title="安装量">⬇ {{ fmtDownloads(s.downloads) }}</span>
            <span v-if="s.updated_ts" class="meta-t">{{ fmtDate(s.updated_ts) }}</span>
          </span>
          <span class="mk-actions">
            <template v-if="s.installed">
              <button
                v-if="hasUpdate(s)"
                class="btn primary mk-install"
                :disabled="updating === s.name"
                @click.stop="doUpdate(s)"
              >
                {{ updating === s.name ? "更新中…" : `更新到 ${s.version}` }}
              </button>
              <span v-else class="badge green">✓ 已安装</span>
            </template>
            <button
              v-else
              class="btn primary mk-install"
              :disabled="installing === s.name"
              @click.stop="install(s)"
            >
              {{ installing === s.name ? "安装中…" : "安装" }}
            </button>
          </span>
        </div>
        <button class="mk-detail-link" @click.stop="openDetail(s)">查看详情 <span aria-hidden="true">↗</span></button>
      </div>
      </div>
    </template>

    <!-- 无限滚动：滚到底部自动加载下一页 -->
    <div ref="sentinel" style="height: 1px"></div>
    <div
      v-if="loadingMore || (canLoadMore && filteredSkills.length > 6)"
      class="load-hint"
    >
      {{ loadingMore ? "正在加载更多…" : "下滑加载更多" }}
    </div>
    <div
      v-else-if="activeKind === 'skillhub' && !loading && skills.length"
      class="load-hint"
    >
      腾讯 SkillHub 接口最多返回 60 条精选，更多技能请用关键词搜索
    </div>

    <!-- 空状态 -->
    <div
      v-if="!loading && !filteredSkills.length && !error"
      class="card empty"
      style="margin-top: 8px"
    >
      <div class="icon">⌕</div>
      <template v-if="activeCat !== '全部'">该分类下没有技能，换个分类看看。</template>
      <template v-else-if="activeKind === 'clawhub' && !query">
        ClawHub 暂无推荐内容，试试关键词 screenshot、pdf、browser。
      </template>
      <template v-else-if="activeKind === 'github'">该仓库下没有技能目录。</template>
      <template v-else>没有找到技能，换个关键词试试。</template>
    </div>

    <!-- 市场详情：列表信息即刻可见，腾讯源再加载官方详情和公开 SKILL.md。 -->
    <div v-if="selected" class="modal-mask" @click.self="closeDetail">
      <section class="modal market-detail" role="dialog" aria-modal="true" :aria-label="`${selected.display_name} 技能详情`">
        <div class="modal-head market-detail-head">
          <SkillIcon :name="selected.name + ' ' + selected.display_name" :identity="selected.name" :description="selected.description" :src="officialImg(selected)" :fallback-src="selected.icon_url ? selected.owner_image : ''" />
          <div class="market-detail-title">
            <span class="market-detail-source">{{ sourceLabel(selected) }} <span v-if="detail?.publisher_source">· {{ detail.publisher_source === 'official' ? '官方发布' : '社区发布' }}</span></span>
            <h2>{{ selected.display_name }}</h2>
            <code>{{ selected.name }}</code>
          </div>
          <button class="btn ghost" aria-label="关闭详情" @click="closeDetail">✕</button>
        </div>
        <div class="modal-body market-detail-body">
          <div class="market-detail-stats">
            <div><b>{{ fmtDownloads(detail?.downloads ?? selected.downloads) }}</b><span>下载量</span></div>
            <div><b>{{ fmtDownloads(detail?.stars ?? selected.stars) }}</b><span>收藏</span></div>
            <div><b>{{ detail?.version || selected.version || '未标注' }}</b><span>最新版本</span></div>
          </div>
          <div class="market-detail-facts">
            <span>作者 <b>{{ detail?.owner_name || selected.owner || '未标注' }}</b></span>
            <span>分类 <b>{{ catLabel(detail?.category || selected.category) }}</b></span>
            <span v-if="detail?.updated_ts || selected.updated_ts">更新 <b>{{ fmtDate(detail?.updated_ts || selected.updated_ts) }}</b></span>
            <span v-if="detail?.created_ts">发布 <b>{{ fmtDate(detail.created_ts) }}</b></span>
            <span v-if="detail?.versions">版本数 <b>{{ detail.versions }}</b></span>
            <span v-if="detail && detail.installs">安装量 <b>{{ fmtDownloads(detail.installs) }}</b></span>
            <span v-if="detail?.requires_api_key === true">需要 API Key</span>
            <span v-if="detail?.pricing_type === 'paid'">付费技能</span>
          </div>
          <div v-if="selected.topics.length" class="mk-topics market-detail-topics"><span v-for="t in selected.topics" :key="t" class="topic">{{ t }}</span></div>
          <section class="market-detail-summary">
            <h3>技能介绍</h3>
            <p>{{ showDesc(detail?.description || selected.description) }}</p>
          </section>
          <p v-if="detail?.changelog" class="market-detail-change"><b>更新说明</b> {{ detail.changelog }}</p>
          <p v-if="detailLoading" class="market-detail-notice">正在读取官方详情和技能内容…</p>
          <p v-if="detailError" class="market-detail-notice error">官方详情暂时无法加载：{{ detailError }} <button @click="openDetail(selected)">重试</button></p>
          <template v-if="selected.source_kind === 'skillhub' && detail">
            <div class="market-detail-tabs" role="tablist" aria-label="技能内容">
              <button role="tab" :aria-selected="detailTab === 'content'" :class="{ active: detailTab === 'content' }" @click="detailTab = 'content'">SKILL.md</button>
              <button role="tab" :aria-selected="detailTab === 'files'" :class="{ active: detailTab === 'files' }" @click="detailTab = 'files'">文件清单 · {{ detail.files.length }}</button>
            </div>
            <div v-if="detailTab === 'content'" class="market-detail-content">
              <p v-if="detail.preview_error" class="market-detail-notice">{{ detail.preview_error }}</p>
              <pre v-else-if="detail.skill_md" class="market-skill-source">{{ detail.skill_md }}</pre>
              <p v-else class="market-detail-notice">该技能暂未公开正文预览。</p>
            </div>
            <div v-else class="market-detail-files">
              <div v-for="file in detail.files" :key="file.path"><code>{{ file.path }}</code><span>{{ formatBytes(file.size_bytes) }}</span></div>
              <p v-if="!detail.files.length" class="market-detail-notice">暂无公开文件清单。</p>
            </div>
          </template>
          <p v-else-if="selected.source_kind !== 'skillhub'" class="market-detail-notice">该市场源暂未提供在线文件预览。安装后可在技能库查看完整 SKILL.md。</p>
        </div>
        <div class="modal-foot">
          <span v-if="selected.installed && !hasUpdate(selected)" class="badge green">✓ 已安装</span>
          <button v-if="selected.installed && hasUpdate(selected)" class="btn primary" :disabled="!!updating" @click="updateFromDetail">更新到 {{ selected.version }}</button>
          <button v-if="!selected.installed" class="btn primary" :disabled="!!installing" @click="installFromDetail">{{ installing === selected.name ? '安装中…' : '安装技能' }}</button>
          <button class="btn" @click="closeDetail">关闭</button>
        </div>
      </section>
    </div>

    <!-- 安装后启用到 Agent -->
    <div v-if="showEnable" class="modal-mask" @click.self="showEnable = false">
      <div class="modal" style="max-width: 460px">
        <div class="modal-head">
          <b style="font-size: 15px">启用「{{ pendingSkill?.display_name }}」到 Agent</b>
          <button class="btn ghost" :disabled="enabling" @click="showEnable = false">✕</button>
        </div>
        <div class="modal-body">
          <p v-if="enableRisk" class="risk-hint">⚠ {{ enableRisk }}</p>
          <!-- 内容安全扫描摘要 -->
          <div v-if="secReport && secReport.level !== 'none'" class="risk-hint" style="border-color: rgba(239,68,68,.3)">
            🛡 内容扫描：<span class="badge" :class="{ red: secReport.level === 'high', amber: secReport.level === 'medium', blue: secReport.level === 'low' }">{{ secReport.level === 'high' ? '高风险' : secReport.level === 'medium' ? '中风险' : '低风险' }}</span>
            <span v-if="secReport.findings.length > 0" style="margin-left: 6px">{{ secReport.findings.filter(f => f.level === 'high').length }} 高 / {{ secReport.findings.filter(f => f.level === 'medium').length }} 中 / {{ secReport.findings.filter(f => f.level === 'low').length }} 低（扫描 {{ secReport.scanned_files }} 文件）</span>
          </div>
          <p class="enable-tip">勾选要启用该技能的 Agent，确认后自动配置并部署链接：</p>
          <div class="enable-list">
            <label v-for="a in store.agents" :key="a.agent.id" class="assign-item">
              <span>{{ a.agent.name }}</span>
              <span class="switch">
                <input
                  type="checkbox"
                  :checked="enableAgents.includes(a.agent.id)"
                  @change="toggleEnableAgent(a.agent.id)"
                />
                <span class="track"></span>
              </span>
            </label>
          </div>
        </div>
        <div class="modal-foot">
          <button class="btn primary" :disabled="enabling" @click="confirmEnable()">
            {{ enabling ? "正在部署…" : "确认启用" }}
          </button>
          <button class="btn" :disabled="enabling" @click="showEnable = false">跳过</button>
        </div>
      </div>
    </div>
  </div>
</template>

<style scoped>
.add-source {
  display: grid;
  grid-template-columns: 1fr 1.4fr 1fr auto;
  gap: 8px;
  padding: 12px;
  margin-bottom: 14px;
}

.sources {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
  margin-bottom: 14px;
}
.source-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding: 6px 14px;
  border-radius: 6px;
  border: 1px solid var(--border);
  background: var(--panel);
  color: var(--muted);
  font-size: 12.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.15s;
}
.source-pill:hover {
  border-color: var(--accent);
  color: var(--accent);
}
.source-pill.active {
  background: var(--grad);
  border-color: transparent;
  color: #fff;
  font-weight: 600;
}
.pill-mark {
  font-size: 13px;
}
.pill-official {
  font-size: 10px;
  font-weight: 700;
  padding: 1px 6px;
  border-radius: 10px;
  background: rgba(0, 110, 255, 0.14);
  color: #4da3ff;
  border: 1px solid rgba(0, 110, 255, 0.3);
}
.src-del {
  opacity: 0.55;
  font-size: 11px;
  padding: 0 2px;
}
.src-del:hover {
  opacity: 1;
  color: var(--red);
}

.search-bar {
  display: flex;
  gap: 10px;
  margin-bottom: 16px;
}
.search-bar .input {
  flex: 1;
}

.err-box {
  background: var(--red-soft);
  border: 1px solid rgba(239, 68, 68, 0.35);
  color: var(--red);
  border-radius: var(--radius-sm);
  padding: 10px 14px;
  font-size: 13px;
  margin-bottom: 14px;
}

.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(290px, 1fr));
  gap: 14px;
}

.load-hint {
  text-align: center;
  color: var(--faint);
  font-size: 12px;
  padding: 16px 0 6px;
}

/* 分类分组标题 */
.group-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin: 18px 0 12px;
}
.group-head:first-of-type {
  margin-top: 0;
}
.group-title {
  font-size: 14.5px;
  font-weight: 700;
  color: var(--text);
}
.group-count {
  font-size: 11.5px;
  color: var(--faint);
  padding: 1px 8px;
  border-radius: 10px;
  background: var(--bg-soft);
}
.group-line {
  flex: 1;
  height: 1px;
  background: var(--border-soft);
}

/* 安装后启用弹窗 */
.enable-tip {
  font-size: 13px;
  color: var(--muted);
  margin-bottom: 10px;
}
.risk-hint {
  font-size: 12.5px;
  color: #d97706;
  background: rgba(245, 158, 11, 0.08);
  border: 1px solid rgba(245, 158, 11, 0.3);
  border-radius: var(--radius-sm);
  padding: 8px 12px;
  margin-bottom: 10px;
  line-height: 1.6;
}
.enable-list {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.enable-list .assign-item {
  padding: 9px 12px;
}
.mk {
  display: flex;
  flex-direction: column;
  gap: 10px;
  cursor: pointer;
  transition: all 0.15s;
}
.mk:hover {
  box-shadow: var(--shadow);
  border-color: var(--accent);
}
.mk:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }
.mk-detail-link { align-self: flex-end; padding: 0; border: 0; background: none; color: var(--accent); font: inherit; font-size: 11px; cursor: pointer; }
.mk-detail-link:hover { text-decoration: underline; }
.mk.featured {
  border-color: var(--border);
}

.mk-head {
  display: flex;
  align-items: center;
  gap: 12px;
}
.mk-icon {
  width: 44px;
  height: 44px;
}
.mk-title {
  min-width: 0;
  flex: 1;
}
.mk-name-row {
  display: flex;
  align-items: center;
  gap: 6px;
}
.mk-name {
  font-size: 14px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.mk-sub {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-top: 4px;
  font-size: 11.5px;
  color: var(--muted);
  flex-wrap: wrap;
}
.mk-owner {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  max-width: 140px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
.avatar {
  width: 15px;
  height: 15px;
  border-radius: 50%;
  object-fit: cover;
  background: var(--border);
}
.mk-version {
  padding: 1px 7px;
  border-radius: 10px;
  background: rgba(127, 127, 127, 0.16);
  font-family: var(--mono, monospace);
  font-size: 10.5px;
}
.mk-src {
  font-size: 10.5px;
  font-weight: 600;
  padding: 1px 7px;
  border-radius: 10px;
}
.src-skillhub {
  background: rgba(0, 110, 255, 0.14);
  color: #4da3ff;
}
.src-clawhub {
  background: rgba(124, 92, 255, 0.16);
  color: #a78bfa;
}
.src-github {
  background: rgba(148, 163, 184, 0.18);
  color: #94a3b8;
}
.src-other {
  background: rgba(127, 127, 127, 0.14);
  color: var(--muted);
}

.mk-desc {
  font-size: 12.5px;
  line-height: 1.6;
  color: var(--muted);
  display: -webkit-box;
  -webkit-line-clamp: 3;
  -webkit-box-orient: vertical;
  overflow: hidden;
  min-height: 0;
}
.mk-desc .trans-btn {
  margin-left: 0;
}

.mk-topics {
  display: flex;
  flex-wrap: wrap;
  gap: 5px;
}
.topic {
  font-size: 10.5px;
  padding: 1px 8px;
  border-radius: 10px;
  background: rgba(124, 92, 255, 0.1);
  border: 1px solid rgba(124, 92, 255, 0.22);
  color: #a78bfa;
}

.mk-meta {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  margin-top: auto;
  padding-top: 4px;
}
.meta-left {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.meta-t {
  font-size: 11px;
  color: var(--muted);
}
.sec {
  font-size: 10.5px;
  padding: 1px 7px;
  border-radius: 10px;
}
.sec.pass {
  background: rgba(16, 185, 129, 0.13);
  color: #34d399;
}
.sec.warn {
  background: rgba(245, 158, 11, 0.14);
  color: #fbbf24;
}
.mk-actions {
  display: flex;
  align-items: center;
}
.mk-install {
  padding: 4px 14px;
  font-size: 12px;
  flex-shrink: 0;
}
.badge.gold {
  background: rgba(250, 204, 21, 0.15);
  color: #facc15;
  border-color: rgba(250, 204, 21, 0.35);
}
.badge.hot {
  background: var(--red-soft);
  color: var(--red);
  border: 1px solid rgba(239, 68, 68, 0.28);
}

.market-detail { max-width: 800px; max-height: min(88vh, 800px); border-radius: 12px; }
.market-detail-head { gap: 14px; align-items: center; }
.market-detail-head .skill-icon { width: 48px; height: 48px; }
.market-detail-title { flex: 1; min-width: 0; }
.market-detail-title h2 { font-size: 18px; line-height: 1.35; overflow-wrap: anywhere; }
.market-detail-title code { display: block; color: var(--faint); font-size: 11px; margin-top: 2px; overflow-wrap: anywhere; }
.market-detail-source { display: block; color: var(--accent); font-size: 11px; font-weight: 600; margin-bottom: 2px; }
.market-detail-body { padding: 20px 24px 24px; }
.market-detail-stats { display: grid; grid-template-columns: repeat(3, 1fr); gap: 10px; }
.market-detail-stats > div { display: flex; flex-direction: column; padding: 12px 14px; border: 1px solid var(--border-soft); border-radius: 8px; background: var(--bg-soft); }
.market-detail-stats b { font: 600 16px/1.4 "Bahnschrift", "Microsoft YaHei UI", sans-serif; overflow-wrap: anywhere; }
.market-detail-stats span { color: var(--faint); font-size: 11px; margin-top: 3px; }
.market-detail-facts { display: flex; flex-wrap: wrap; gap: 8px 22px; padding: 16px 0; color: var(--muted); font-size: 11.5px; border-bottom: 1px solid var(--border-soft); }
.market-detail-facts b { color: var(--text); font-weight: 550; margin-left: 4px; }
.market-detail-topics { margin-top: 14px; }
.market-detail-summary { margin: 18px 0; }
.market-detail-summary h3 { font-size: 12px; color: var(--faint); margin-bottom: 7px; }
.market-detail-summary p { font-size: 13px; line-height: 1.75; color: var(--text); white-space: pre-wrap; overflow-wrap: anywhere; }
.market-detail-change { padding: 10px 12px; border-radius: 7px; background: var(--bg-soft); font-size: 12px; color: var(--muted); white-space: pre-wrap; overflow-wrap: anywhere; }
.market-detail-change b { color: var(--text); margin-right: 6px; }
.market-detail-notice { color: var(--muted); font-size: 12px; padding: 15px 0; }
.market-detail-notice.error { color: var(--red); }
.market-detail-notice button { margin-left: 8px; padding: 0; border: 0; background: none; color: var(--accent); cursor: pointer; font: inherit; text-decoration: underline; }
.market-detail-tabs { display: flex; gap: 20px; margin-top: 22px; border-bottom: 1px solid var(--border); }
.market-detail-tabs button { padding: 0 2px 10px; border: 0; border-bottom: 2px solid transparent; background: none; color: var(--muted); font-size: 12px; cursor: pointer; }
.market-detail-tabs button.active { border-bottom-color: var(--accent); color: var(--text); font-weight: 600; }
.market-detail-content, .market-detail-files { padding-top: 14px; }
.market-skill-source { padding: 18px; background: var(--bg-soft); border: 1px solid var(--border-soft); border-radius: 8px; color: var(--text); font: 12px/1.75 "Cascadia Code", "Consolas", "Microsoft YaHei UI", monospace; white-space: pre-wrap; overflow-wrap: anywhere; }
.market-detail-files > div { display: flex; gap: 12px; justify-content: space-between; padding: 9px 3px; border-bottom: 1px solid var(--border-soft); font-size: 11px; }
.market-detail-files code { min-width: 0; overflow-wrap: anywhere; }
.market-detail-files span { color: var(--faint); white-space: nowrap; }
.market-detail .modal-foot { align-items: center; }
.market-detail .modal-foot .badge { margin-right: auto; }
@media (max-width: 760px) { .market-detail-body { padding: 16px; } .market-detail-facts { gap: 8px 14px; } }
</style>
