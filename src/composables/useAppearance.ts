import { readonly, ref } from "vue";

export type Appearance = "monochrome" | "blue";
const storageKey = "agentskillhub.appearance";

function loadAppearance(): Appearance {
  try {
    return localStorage.getItem(storageKey) === "blue" ? "blue" : "monochrome";
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
