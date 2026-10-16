<script setup lang="ts">
import { computed, onMounted, onUnmounted, nextTick, ref, shallowRef, Transition, watch } from "vue";
import { discoverMachine, discovery, loadAll, removeToast, store, ui } from "./store";
import pkg from "../package.json";
import Dashboard from "./views/Dashboard.vue";
import Skills from "./views/Skills.vue";
import AgentConfig from "./views/AgentConfig.vue";
import AuditView from "./views/AuditView.vue";
import Settings from "./views/Settings.vue";
import Market from "./views/Market.vue";
import { useAppearance } from "./composables/useAppearance";

const { appearance, setAppearance } = useAppearance();

const pages = [
  { id:"dashboard",name:"总览",icon:"M3 3h7v7H3z M14 3h7v7h-7z M3 14h7v7H3z M14 14h7v7h-7z" },
  { id:"skills",name:"技能库",icon:"m12 3 9 5-9 5-9-5 9-5ZM3 8v9l9 5 9-5V8 M12 13v9" },
  { id:"market",name:"技能市场",icon:"M3 10h18 M5 10v11h14V10 M3 10l2-7h14l2 7 M9 21v-7h6v7" },
  { id:"agents",name:"Agent 配置",icon:"M4 5h16 M4 12h16 M4 19h16 M8 3v4 M16 10v4 M10 17v4" },
  { id:"audit",name:"部署记录",icon:"M8 3h10l3 3v15H3V3h2 M8 3v4h8V3 M7 12h10 M7 16h7" },
  { id:"settings",name:"设置",icon:"m9 3-1 3-3 1-2 4 2 2v3l3 3h3l2 2 4-2 1-3 3-1V9l-3-1-1-3-4-2-2 2H9Z M15 12a3 3 0 1 1-6 0 3 3 0 0 1 6 0Z" },
];
const mainEl = ref<HTMLElement>();

const current = ref(ui.page);
const view = shallowRef(Dashboard);

const TOAST_ICONS: Record<string, string> = { success: "✓", error: "✕", info: "i" };

const pageMap: Record<string, any> = {
  dashboard: Dashboard,
  skills: Skills,
  market: Market,
  agents: AgentConfig,
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
          <span>Agent 技能管理</span>
        </div>
      </div>

      <div class="nav-caption">工作空间</div>
      <nav class="nav" aria-label="主导航">
        <button
          v-for="p in pages"
          :key="p.id"
          class="nav-item"
          :class="{ active: current === p.id, 'nav-management':p.id === 'agents', 'nav-settings':p.id === 'settings' }"
          :aria-current="current === p.id ? 'page' : undefined"
          @click="nav(p.id)"
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.65" stroke-linecap="round" stroke-linejoin="round">
            <path :d="p.icon" />
          </svg>
          <span>{{ p.name }}</span>
          <span v-if="p.id === 'skills'" class="nav-count">{{ store.skills.length }}</span>
          <span v-if="p.id === 'agents'" class="nav-count">{{ store.agents.length }}</span>
        </button>
      </nav>

      <div class="sidebar-foot">
        <div class="workspace-label"><span class="workspace-symbol">S</span><div><b>本地技能空间</b><span>一个技能库，连接所有 Agent</span></div></div>
        <div class="lib-path" :title="store.config?.skills_root">
          <span class="dot" :class="{ ok: store.config?.skills_root }"></span>
          {{ store.config?.skills_root || "未设置技能库" }}
        </div>
        <div class="ver">v{{ pkg.version }}<span>本地文件管理</span></div>
      </div>
    </aside>

    <main ref="mainEl" class="main">
      <div class="workspace-bar">
        <div class="breadcrumb">工作空间 <span>/</span> <b>{{ pageName }}</b></div>
        <div class="workspace-tools">
          <span class="local-status"><i></i>{{ discovery.loading ? '正在发现本机工具' : '本地运行' }}</span>
          <div class="appearance-switch" role="group" aria-label="界面配色">
            <button :class="{ active: appearance === 'monochrome' }" :aria-pressed="appearance === 'monochrome'" @click="setAppearance('monochrome')"><i class="swatch mono"></i>黑白</button>
            <button :class="{ active: appearance === 'blue' }" :aria-pressed="appearance === 'blue'" @click="setAppearance('blue')"><i class="swatch blue"></i>蓝白</button>
          </div>
        </div>
      </div>
      <!-- 扫描进行中的顶部进度条 -->
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
  padding: 26px 14px 20px;
}

.logo {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 0 8px 30px;
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

.nav {
  display: flex;
  flex-direction: column;
  gap: 4px;
  flex: 1;
}
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
  border-top: 1px solid var(--border);
  padding-top: 18px;
  font-size: 11px;
}
.lib-path {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  padding: 0 4px;
}
.lib-path .dot {
  width: 7px;
  height: 7px;
  border-radius: 50%;
  background: var(--faint);
  flex-shrink: 0;
}
.lib-path .dot.ok {
  background: var(--green);
  box-shadow: none;
}
.ver {
  color: var(--faint);
  padding: 16px 4px 0;
  display:flex;
  justify-content:space-between;
  font-size:10px;
}

.main {
  flex: 1;
  min-width: 0;
  overflow: auto;
  padding: 0 32px 32px;
  position: relative;
}
.nav-caption { padding:0 12px 12px; color:var(--faint); font-size:11px; }
.nav-count { margin-left:auto; background:var(--panel); color:var(--muted); padding:0 6px; border:1px solid var(--border); border-radius:4px; font:10px/17px "Consolas",monospace; }
.nav-activity { width:5px; height:5px; margin-left:auto; background:var(--accent); border-radius:50%; }
.nav-item.nav-management { margin-top:22px; }
.nav-item.nav-settings { margin-top:auto; margin-bottom:16px; }
.workspace-label { display:flex; gap:9px; align-items:center; margin-bottom:14px; }
.workspace-symbol { width:30px; height:30px; display:grid; place-items:center; background:var(--panel); color:var(--text); border:1px solid var(--border); border-radius:7px; font:14px "Bahnschrift",sans-serif; }
.workspace-label > div { display:flex; flex-direction:column; }
.workspace-label b { font-size:12px; font-weight:500; color:var(--text); }
.workspace-label > div > span { font-size:10px; color:var(--faint); margin-top:3px; }
.workspace-bar { display:flex; align-items:center; justify-content:space-between; min-height:64px; margin:0 -32px 30px; padding:12px 32px; border-bottom:1px solid var(--border); background:var(--panel); color:var(--faint); font-size:12px; position:sticky; top:0; z-index:6; }
.breadcrumb > span { margin:0 12px; color:#d4d4d8; }
.breadcrumb b { color:var(--text); font-weight:500; }
.workspace-tools { display:flex; align-items:center; gap:20px; }
.local-status { display:flex; align-items:center; gap:6px; font-size:11px; }
.local-status i { width:5px; height:5px; border-radius:50%; background:var(--green); }
.appearance-switch { display:flex; align-items:center; gap:2px; border:1px solid var(--border); border-radius:7px; background:var(--bg-soft); padding:3px; }
.appearance-switch button { display:flex; align-items:center; gap:6px; padding:4px 9px; font-size:11px; color:var(--muted); background:transparent; border:1px solid transparent; border-radius:4px; cursor:pointer; }
.appearance-switch button.active { background:var(--panel); border-color:var(--border); color:var(--text); box-shadow:var(--shadow-sm); }
.swatch { width:9px; height:9px; border-radius:50%; border:1px solid #00000015; }
.swatch.mono { background:#27272a; }.swatch.blue { background:#3b82f6; }
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
