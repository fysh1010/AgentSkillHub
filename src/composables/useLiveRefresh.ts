import { onMounted, onUnmounted, reactive } from "vue";
import { workspaceStamp } from "../api/tauri";
import { discovery, discoverMachine, loadAll, store } from "../store";

export const liveRefresh = reactive({ refreshing: false, lastUpdated: 0, error: "" });

/** 无需新增系统依赖：两秒检查目录元数据，有变化才重新扫描。 */
export function useLiveRefresh() {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let stopped = false;
  let running = false;
  let previous: string | undefined;
  let lastDiscovery = 0;
  async function check() {
    if (stopped || running) return;
    running = true;
    try {
      if (document.hidden || store.assigning || store.loading || discovery.loading) return;
      liveRefresh.refreshing = true;
      const stamp = await workspaceStamp();
      if (stopped) return;
      if (stamp !== previous) {
        await discoverMachine(false);
        if (stopped) return;
        await loadAll(true);
        previous = stamp;
        lastDiscovery = Date.now();
        liveRefresh.lastUpdated = Date.now();
      } else if (Date.now() - lastDiscovery > 30_000) {
        await discoverMachine(false);
        await loadAll(true);
        lastDiscovery = Date.now();
      }
      liveRefresh.error = "";
    } catch (e) {
      liveRefresh.error = String(e);
    } finally {
      running = false;
      liveRefresh.refreshing = false;
      if (!stopped) timer = setTimeout(check, 2000);
    }
  }
  function wake() {
    if (timer) clearTimeout(timer);
    void check();
  }
  onMounted(() => {
    window.addEventListener("focus", wake);
    document.addEventListener("visibilitychange", wake);
    void check();
  });
  onUnmounted(() => {
    stopped = true;
    if (timer) clearTimeout(timer);
    window.removeEventListener("focus", wake);
    document.removeEventListener("visibilitychange", wake);
  });
}
