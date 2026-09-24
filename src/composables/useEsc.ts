import { onMounted, onUnmounted } from "vue";

/**
 * Esc 键关闭弹窗：组件挂载时监听全局 keydown，卸载时自动移除。
 * handler 内自行决定关闭哪个弹窗（多弹窗时按层级从最上层开始判断）。
 */
export function useEsc(handler: () => void) {
  const fn = (e: KeyboardEvent) => {
    if (e.key === "Escape") handler();
  };
  onMounted(() => window.addEventListener("keydown", fn));
  onUnmounted(() => window.removeEventListener("keydown", fn));
}
