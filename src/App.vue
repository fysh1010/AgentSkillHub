<script setup lang="ts">
import { computed, onMounted, onUnmounted, nextTick, ref, shallowRef, Transition, watch } from "vue";
import { discoverMachine, discovery, loadAll, removeToast, store, toast, ui } from "./store";
import { revealPath } from "./api/tauri";
import { useUpdates } from "./composables/useUpdates";
import pkg from "../package.json";
import Dashboard from "./views/Dashboard.vue";
import Skills from "./views/Skills.vue";
import AgentConfig from "./views/AgentConfig.vue";
import AuditView from "./views/AuditView.vue";
import Settings from "./views/Settings.vue";
import Market from "./views/Market.vue";
import McpManager from "./views/McpManager.vue";
import { appearanceOptions, useAppearance, type Appearance } from "./composables/useAppearance";

const { appearance, setAppearance } = useAppearance();
const updates = useUpdates();
const appearanceChoice = computed<Appearance>({ get: () => appearance.value, set: setAppearance });
const activeAppearance = computed(() => appearanceOptions.find(option => option.id === appearance.value)!);

const pages = [
  { id:"dashboard",name:"总览",icon:"M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z" },
  { id:"skills",name:"技能库",icon:"m12 3 9 5-9 5-9-5 9-5ZM3 8v9l9 5 9-5V8 M12 13v9" },
  { id:"market",name:"技能市场",icon:"M3 10h18 M5 10v11h14V10 M3 10l2-7h14l2 7 M9 21v-7h6v7" },
  { id:"agents",name:"Agent 配置",icon:"M4 5h16 M4 12h16 M4 19h16 M8 3v4 M16 10v4 M10 17v4" },
  { id:"mcp",name:"MCP 管理",icon:"M8 3v4 M16 3v4 M5 7h14v4a7 7 0 0 1-14 0V7 M12 18v4" },
  { id:"audit",name:"部署记录",icon:"M8 3h10l3 3v15H3V3h2 M8 3v4h8V3 M7 12h10 M7 16h7" },
  { id:"settings",name:"设置",icon:"m9 3-1 3-3 1-2 4 2 2v3l3 3h3l2 2 4-2 1-3 3-1V9l-3-1-1-3-4-2-2 2H9Z M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" },
];
const mainEl = ref<HTMLElement>();
const navGroups = [
  { name: "工作空间", pages: pages.slice(0, 3) },
  { name: "管理", pages: pages.slice(3, 6) },
];
const settingsPage = pages.find(p => p.id === "settings")!;
const workspaceName = computed(() => store.config?.skills_root.replace(/[\\/]+$/, "").split(/[\\/]/).pop() || "未设置目录");
const installedAgentCount = computed(() => {
  const installed = new Set(discovery.inventory?.agents.filter(agent => agent.installed && !agent.shared).map(agent => agent.id) || []);
  return store.agents.filter(agent => installed.has(agent.agent.id)).length;
});
function openLibrarySettings() {
  ui.settingsTab = "general";
  nav("settings");
}
async function openLibraryFolder() {
  if (!store.config?.skills_root) return;
  try { await revealPath(store.config.skills_root); }
  catch (e) { toast(`打开技能库失败：${e}`, "error"); }
}

const current = ref(ui.page);
const view = shallowRef(Dashboard);

const TOAST_ICONS: Record<string, string> = { success: "✓", error: "✕", info: "i" };

const pageMap: Record<string, any> = {
  dashboard: Dashboard,
  skills: Skills,
  market: Market,
  agents: AgentConfig,
  mcp: McpManager,
  audit: AuditView,
  settings: Settings,
};
const pageName = computed(() => pages.find((p) => p.id === current.value)?.name || "总览");

function nav(id: string) {
  if (id === "discovery") { ui.settingsTab = "agents"; id = "settings"; }
  current.value = id;
  ui.page = id;
  view.value = pageMap[id] || Dashboard;
  void nextTick(() => mainEl.value?.scrollTo({top:0}));
}

// 其他页面（如总览的「去分配」按钮）通过 ui.page 请求跳转
watch(
  () => ui.page,
  (p) => {
    if (p !== current.value) nav(p);
  }
);

function refreshOnFocus() {
  if (!discovery.loading && (!discovery.inventory || Date.now()/1000-discovery.inventory.scanned_at > 30)) void discoverMachine();
}
onMounted(async () => {
  window.addEventListener("focus",refreshOnFocus);
  void updates.initialize();
  await loadAll();
  await discoverMachine();
});
onUnmounted(() => window.removeEventListener("focus",refreshOnFocus));
</script>

<template>
  <div class="layout">
    <aside class="sidebar">
      <div class="logo">
        <div class="logo-icon">
          <img src="/app-icon.svg" alt="" />
        </div>
        <div class="logo-text">
          <b>AgentSkillHub</b>
          <span>Agent 技能与 MCP 管理</span>
        </div>
      </div>

      <div class="sidebar-navigation">
        <nav v-for="group in navGroups" :key="group.name" class="nav" :aria-label="group.name">
          <div class="nav-caption">{{ group.name }}</div>
          <button
            v-for="p in group.pages"
            :key="p.id"
            class="nav-item"
            :class="{ active: current === p.id }"
            :aria-current="current === p.id ? 'page' : undefined"
            @click="nav(p.id)"
          >
            <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
              <path :d="p.icon" />
            </svg>
            <span>{{ p.name }}</span>
            <span v-if="p.id === 'skills'" class="nav-count">{{ store.skills.length }}</span>
            <span v-if="p.id === 'agents'" class="nav-count">{{ installedAgentCount }}</span>
          </button>
        </nav>
      </div>

      <div class="sidebar-foot">
        <button class="nav-item nav-settings" :class="{ active: current === 'settings' }" :aria-current="current === 'settings' ? 'page' : undefined" @click="nav('settings')">
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round"><path :d="settingsPage.icon" /></svg>
          <span>设置</span><span class="ver">v{{ pkg.version }}</span>
        </button>
        <details class="workspace-location">
          <summary :title="store.config?.skills_root || '配置本地技能库目录'">
            <span class="workspace-symbol"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7V5h6l2 2h10v13H3V7Z" /></svg></span>
            <span class="workspace-label"><b>本地技能库<span class="dot" :class="{ ok: store.config?.skills_root }"></span></b><span>{{ workspaceName }} · {{ store.skills.length }} 个技能</span></span>
            <svg class="workspace-chevron" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.8" stroke-linecap="round" stroke-linejoin="round"><path d="m9 5 7 7-7 7" /></svg>
          </summary>
          <div class="workspace-details">
            <div class="lib-path">{{ store.config?.skills_root || '设置目录后即可管理本地技能' }}</div>
            <div class="workspace-actions"><button :disabled="!store.config?.skills_root" @click="openLibraryFolder">打开目录 ↗</button><button @click="openLibrarySettings">目录设置</button></div>
          </div>
        </details>
      </div>
    </aside>

    <main ref="mainEl" class="main">
      <div class="workspace-bar">
        <div class="breadcrumb">工作空间 <span>/</span> <b>{{ pageName }}</b></div>
        <div class="workspace-tools">
          <span class="local-status"><i></i>{{ discovery.loading ? '正在发现本机工具' : '本地运行' }}</span>
          <div class="appearance-picker">
            <i class="swatch" :style="{ background: activeAppearance.color }"></i>
            <select v-model="appearanceChoice" aria-label="界面配色">
              <option v-for="option in appearanceOptions" :key="option.id" :value="option.id">{{ option.name }}</option>
            </select>
          </div>
        </div>
      </div>
      <!-- 扫描进行中的顶部进度条 -->
      <div v-if="updates.info.value?.available && !updates.dismissed.value" class="update-banner" role="status">
        <span>发现新版本 {{ updates.info.value.latest_version }}</span>
        <button class="btn" @click="updates.open(updates.info.value!.release_url).catch(e => toast(String(e),'error'))">查看更新</button>
        <button v-if="updates.info.value.download_url" class="btn primary" @click="updates.open(updates.info.value!.download_url).catch(e => toast(String(e),'error'))">下载更新</button>
        <button class="btn ghost" aria-label="暂时关闭更新通知" @click="updates.dismissed.value = true">✕</button>
      </div>
      <div v-if="store.loading" class="scan-bar"></div>
      <Transition name="page" mode="out-in">
        <component :is="view" :key="current" />
      </Transition>
    </main>

    <!-- Toast -->
    <div class="toast-wrap">
      <div v-for="t in store.toasts" :key="t.id" class="toast" :class="t.type">
        <span class="toast-icon">{{ TOAST_ICONS[t.type] || "i" }}</span>
        <span class="toast-text">{{ t.text }}</span>
        <button class="toast-close" aria-label="关闭" @click="removeToast(t.id)">✕</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.update-banner { display:flex; align-items:center; flex-wrap:wrap; gap:10px; margin-bottom:20px; padding:12px 16px; border:1px solid var(--border); border-radius:8px; background:var(--panel); font-size:13px; }.update-banner > span { flex:1; }
.layout {
  display: flex;
  height: 100vh;
}

.sidebar {
  width: 224px;
  flex-shrink: 0;
  background: var(--sidebar-bg);
  color: var(--text);
  border-right: 1px solid var(--border);
  display: flex;
  flex-direction: column;
  min-height: 0;
  padding: 22px 12px 12px;
}

.logo {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 8px 26px;
}
.logo-icon {
  width: 35px;
  height: 35px;
  border-radius: 8px;
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: none;
}
.logo-icon img {
  width: 35px;
  height: 35px;
  display: block;
}
.logo-text {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.logo-text b {
  font-size: 15px;
  font-family:"Bahnschrift", "Microsoft YaHei UI", sans-serif;
  letter-spacing: -0.2px;
}
.logo-text span {
  font-size: 11px;
  color: var(--muted);
  margin-top:4px;
}

.sidebar-navigation { flex: 1; min-height: 0; overflow-y: auto; }
.nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.nav + .nav { margin-top: 14px; padding-top: 14px; border-top: 1px solid var(--border-soft); }
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 12px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 13px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.16s;
  text-align: left;
}
.nav-item:hover {
  background: var(--panel-2);
  color: var(--text);
}
.nav-item.active {
  background: var(--nav-active-bg);
  color: var(--nav-active-text);
  font-weight: 600;
}
.nav-icon {
  width: 16px;
  height: 16px;
  flex-shrink: 0;
}

.sidebar-foot {
  flex-shrink: 0;
  margin-top: 16px;
  font-size: 11px;
}
.nav-settings { width: 100%; margin-bottom: 10px; }
.workspace-location {
  border-top: 1px solid var(--border);
  padding-top: 10px;
}
.workspace-location summary { display: flex; align-items: center; gap: 9px; padding: 8px; border-radius: 8px; cursor: pointer; list-style: none; }
.workspace-location summary::-webkit-details-marker { display: none; }
.workspace-location summary:hover { background: var(--panel-2); }
.workspace-symbol { width: 30px; height: 32px; display: grid; place-items: center; background: var(--panel); border: 1px solid var(--border); border-radius: 7px; flex-shrink: 0; }
.workspace-symbol svg { width: 17px; height: 17px; color: var(--muted); }
.workspace-label { display: flex; flex-direction: column; min-width: 0; flex: 1; gap: 2px; }
.workspace-label b { display: flex; align-items: center; gap: 7px; font-size: 11px; font-weight: 600; }
.workspace-label > span { font-size: 10px; color: var(--faint); overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.workspace-chevron { width: 13px; height: 13px; flex-shrink: 0; color: var(--faint); transition: transform .15s; }
.workspace-location[open] .workspace-chevron { transform: rotate(90deg); }
.workspace-details { padding: 6px 8px 8px; }
.lib-path {
  color: var(--muted);
  font: 10px/1.7 "Consolas", "Microsoft YaHei UI", monospace;
  overflow-wrap: anywhere;
  max-height: 68px;
  overflow-y: auto;
}
.dot {
  width: 5px;
  height: 5px;
  border-radius: 50%;
  background: var(--faint);
  flex-shrink: 0;
}
.dot.ok {
  background: var(--green);
  box-shadow: none;
}
.ver {
  margin-left: auto;
  color: var(--faint);
  font: 10px "Consolas", monospace;
}
.workspace-actions { display: flex; gap: 14px; margin-top: 8px; }
.workspace-actions button { border: 0; background: none; color: var(--muted); font: inherit; font-size: 11px; cursor: pointer; }
.workspace-actions button:hover { color: var(--accent); }
.workspace-actions button:disabled { opacity: .4; cursor: not-allowed; }
.sidebar button:focus-visible, .workspace-location summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 2px; }

.main {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 0 32px 32px;
  position: relative;
}
.nav-caption { padding:0 12px 6px; color:var(--faint); font-size:10px; letter-spacing:.5px; }
.nav-count { margin-left:auto; background:var(--panel); color:var(--muted); padding:0 6px; border:1px solid var(--border); border-radius:4px; font:10px/17px "Consolas",monospace; }
.workspace-bar { display:flex; align-items:center; justify-content:space-between; min-height:64px; margin:0 -32px 30px; padding:12px 32px; border-bottom:1px solid var(--border); background:var(--panel); color:var(--faint); font-size:12px; position:sticky; top:0; z-index:6; }
.breadcrumb > span { margin:0 12px; color:#d4d4d8; }
.breadcrumb b { color:var(--text); font-weight:500; }
.workspace-tools { display:flex; align-items:center; gap:20px; }
.local-status { display:flex; align-items:center; gap:6px; font-size:11px; }
.local-status i { width:5px; height:5px; border-radius:50%; background:var(--green); }
.appearance-picker { display:flex; align-items:center; gap:7px; border:1px solid var(--border); border-radius:7px; background:var(--panel); padding:4px 8px; }
.appearance-picker select { max-width:150px; border:0; outline:0; background:var(--panel); color:var(--text); font-size:11px; cursor:pointer; }
.swatch { width:10px; height:10px; flex:none; border-radius:50%; border:1px solid var(--border); }
@media(max-width:1100px) { .sidebar { width:200px; } .main { padding-left:24px; padding-right:24px; } .workspace-bar { margin-left:-24px; margin-right:-24px; padding-left:24px; padding-right:24px; } .local-status { display:none; } }

/* 扫描进度条：细长渐变条贴在主区顶部循环滑动 */
.scan-bar {
  position: absolute;
  top: 0;
  left: 0;
  right: 0;
  height: 3px;
  overflow: hidden;
  z-index: 5;
}
.scan-bar::before {
  content: "";
  position: absolute;
  inset: 0;
  background: linear-gradient(90deg, transparent, var(--accent), var(--accent-2), transparent);
  animation: scan-slide 1.1s linear infinite;
}
@keyframes scan-slide {
  from {
    transform: translateX(-100%);
  }
  to {
    transform: translateX(100%);
  }
}
@media (prefers-reduced-motion: reduce) {
  .scan-bar::before {
    animation-duration: 3s;
  }
}

/* 页面切换过渡 */
.page-enter-active,
.page-leave-active {
  transition:
    opacity 0.14s ease,
    transform 0.14s ease;
}
.page-enter-from {
  opacity: 0;
  transform: translateY(6px);
}
.page-leave-to {
  opacity: 0;
  transform: translateY(-4px);
}
@media (prefers-reduced-motion: reduce) {
  .page-enter-active,
  .page-leave-active {
    transition: none;
  }
}
</style>
