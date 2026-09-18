<script setup lang="ts">
import { onMounted, ref, shallowRef, Transition, watch } from "vue";
import { loadAll, store, ui } from "./store";
import Dashboard from "./views/Dashboard.vue";
import Skills from "./views/Skills.vue";
import AgentConfig from "./views/AgentConfig.vue";
import AuditView from "./views/AuditView.vue";
import Settings from "./views/Settings.vue";
import Market from "./views/Market.vue";

const pages = [
  { id: "dashboard", name: "总览", icon: "M4 13h6V4H4v9zm0 7h6v-5H4v5zm10 0h6v-9h-6v9zm0-16v5h6V4h-6z" },
  { id: "skills", name: "技能库", icon: "M12 2 3 7v10l9 5 9-5V7l-9-5zm0 2.3 6.8 3.8-2.4 1.3L12 6.3 7.6 8.4 5.2 7.1 12 4.3zM5 16.4v-7l5 2.8v7l-5-2.8zm9 .8 5-2.8v-7l-5 2.8v7z" },
  { id: "market", name: "技能市场", icon: "M4 4h2v12h14v2H4V4zm4 2h12v8H8V6zm3 2v1h2V8h-2zm-3 8h2v2h-2v-2zm4 0h2v2h-2v-2z" },
  { id: "agents", name: "Agent 配置", icon: "M4 4h7v7H4V4zm9 0h7v7h-7V4zM4 13h7v7H4v-7zm9 0h7v7h-7v-7z" },
  { id: "audit", name: "部署记录", icon: "M12 2a10 10 0 1 0 10 10A10 10 0 0 0 12 2zm1 15h-2v-6h2zm0-8h-2V7h2z" },
  { id: "settings", name: "设置", icon: "M19.4 13a7.6 7.6 0 0 0 0-2l2.1-1.6-2-3.5-2.5 1a7.6 7.6 0 0 0-1.7-1l-.4-2.6h-4l-.4 2.6a7.6 7.6 0 0 0-1.7 1l-2.5-1-2 3.5L6.6 11a7.6 7.6 0 0 0 0 2l-2.1 1.6 2 3.5 2.5-1a7.6 7.6 0 0 0 1.7 1l.4 2.6h4l.4-2.6a7.6 7.6 0 0 0 1.7-1l2.5 1 2-3.5-2.1-1.6zM12 15.5A3.5 3.5 0 1 1 15.5 12 3.5 3.5 0 0 1 12 15.5z" },
];

const current = ref(ui.page);
const view = shallowRef(Dashboard);

const pageMap: Record<string, any> = {
  dashboard: Dashboard,
  skills: Skills,
  market: Market,
  agents: AgentConfig,
  audit: AuditView,
  settings: Settings,
};

function nav(id: string) {
  current.value = id;
  ui.page = id;
  view.value = pageMap[id] || Dashboard;
}

// 其他页面（如总览的「去分配」按钮）通过 ui.page 请求跳转
watch(
  () => ui.page,
  (p) => {
    if (p !== current.value) nav(p);
  }
);

onMounted(() => {
  loadAll();
});
</script>

<template>
  <div class="layout">
    <aside class="sidebar">
      <div class="logo">
        <div class="logo-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
            <path d="M12 2 3 7v10l9 5 9-5V7l-9-5z" />
            <path d="M12 12 3 7" />
            <path d="M12 12l9-5" />
            <path d="M12 12v10" />
          </svg>
        </div>
        <div class="logo-text">
          <b>AgentSkillHub</b>
          <span>技能统一管理台</span>
        </div>
      </div>

      <nav class="nav">
        <button
          v-for="p in pages"
          :key="p.id"
          class="nav-item"
          :class="{ active: current === p.id }"
          @click="nav(p.id)"
        >
          <svg class="nav-icon" viewBox="0 0 24 24" fill="currentColor">
            <path :d="p.icon" />
          </svg>
          <span>{{ p.name }}</span>
        </button>
      </nav>

      <div class="sidebar-foot">
        <div class="lib-path" :title="store.config?.skills_root">
          <span class="dot" :class="{ ok: store.config?.skills_root }"></span>
          {{ store.config?.skills_root || "未设置技能库" }}
        </div>
        <div class="ver">v0.1.0 · 本地运行 · 数据不出本机</div>
      </div>
    </aside>

    <main class="main">
      <!-- 扫描进行中的顶部进度条 -->
      <div v-if="store.loading" class="scan-bar"></div>
      <Transition name="page" mode="out-in">
        <component :is="view" :key="current" />
      </Transition>
    </main>

    <!-- Toast -->
    <div class="toast-wrap">
      <div v-for="t in store.toasts" :key="t.id" class="toast" :class="t.type">
        {{ t.text }}
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
  background: var(--panel);
  border-right: 1px solid var(--border-soft);
  display: flex;
  flex-direction: column;
  padding: 20px 12px 16px;
}

.logo {
  display: flex;
  align-items: center;
  gap: 11px;
  padding: 2px 8px 20px;
}
.logo-icon {
  width: 42px;
  height: 42px;
  border-radius: 12px;
  background: var(--grad);
  color: #fff;
  display: flex;
  align-items: center;
  justify-content: center;
  box-shadow: 0 6px 16px rgba(78, 110, 242, 0.35);
}
.logo-icon svg {
  width: 22px;
  height: 22px;
}
.logo-text {
  display: flex;
  flex-direction: column;
  line-height: 1.3;
}
.logo-text b {
  font-size: 15px;
  letter-spacing: -0.2px;
}
.logo-text span {
  font-size: 11px;
  color: var(--faint);
}

.nav {
  display: flex;
  flex-direction: column;
  gap: 3px;
  flex: 1;
}
.nav-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 9px 12px;
  border-radius: var(--radius-sm);
  border: none;
  background: transparent;
  color: var(--muted);
  font-size: 13.5px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.16s;
  text-align: left;
}
.nav-item:hover {
  background: var(--bg-soft);
  color: var(--text);
}
.nav-item.active {
  background: var(--accent-soft);
  color: var(--accent);
  font-weight: 600;
}
.nav-icon {
  width: 17px;
  height: 17px;
  flex-shrink: 0;
}

.sidebar-foot {
  border-top: 1px solid var(--border-soft);
  padding-top: 12px;
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
  box-shadow: 0 0 6px rgba(18, 183, 106, 0.6);
}
.ver {
  color: var(--faint);
  padding: 6px 4px 0;
}

.main {
  flex: 1;
  overflow: auto;
  padding: 26px 30px;
  position: relative;
}

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
