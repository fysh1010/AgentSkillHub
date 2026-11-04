import { readonly, ref } from "vue";

export const appearanceOptions = [
  { id: "monochrome", name: "黑白", color: "#27272a" },
  { id: "blue", name: "蓝白", color: "#3b82f6" },
  { id: "latte", name: "Catppuccin Latte", color: "#8839ef" },
  { id: "mocha", name: "Catppuccin Mocha", color: "#cba6f7" },
  { id: "tokyo", name: "Tokyo Night", color: "#7aa2f7" },
  { id: "nord", name: "Nord", color: "#88c0d0" },
] as const;
export type Appearance = typeof appearanceOptions[number]["id"];
const storageKey = "agentskillhub.appearance";

function loadAppearance(): Appearance {
  try {
    const saved = localStorage.getItem(storageKey);
    return appearanceOptions.some(option => option.id === saved) ? saved as Appearance : "monochrome";
  } catch {
    return "monochrome";
  }
}

const appearance = ref<Appearance>(loadAppearance());

function setAppearance(value: Appearance) {
  appearance.value = value;
  document.documentElement.dataset.appearance = value;
  try { localStorage.setItem(storageKey, value); } catch { /* 受限存储环境仍允许本次切换。 */ }
}

// 在组件首次渲染前应用，避免已保存主题在启动时闪回默认配色。
document.documentElement.dataset.appearance = appearance.value;

export function useAppearance() {
  return { appearance: readonly(appearance), setAppearance };
}
