<script setup lang="ts">
import { computed } from "vue";

const props = defineProps<{ label: string }>();
const glyphs: Array<[RegExp, string]> = [
  [/browser|web|search|http|浏览|搜索/i, "M21 12a9 9 0 1 1-18 0 9 9 0 0 1 18 0ZM3 12h18M12 3a17 17 0 0 1 0 18 17 17 0 0 1 0-18Z"],
  [/code|git|api|dev|代码|开发/i, "m8 6-6 6 6 6m8-12 6 6-6 6m-3-15-2 18"],
  [/sheet|excel|data|table|表格|数据/i, "M4 3h16v18H4V3Zm0 6h16M4 15h16M10 3v18"],
  [/image|design|photo|art|图像|设计/i, "M3 3h18v18H3V3Zm0 14 6-6 4 4 3-3 5 5M8 7h.01"],
  [/security|auth|安全/i, "m12 2 8 4v6c0 5-8 10-8 10S4 17 4 12V6l8-4Zm-4 10 3 3 5-6"],
  [/pdf|doc|write|文件|文档|写作/i, "M5 2h9l5 5v15H5V2Zm9 0v6h5M8 12h8M8 16h8"],
];
const path = computed(() => glyphs.find(([pattern]) => pattern.test(props.label))?.[1]
  || "m12 3 9 5-9 5-9-5 9-5ZM3 12l9 5 9-5M3 16l9 5 9-5");
</script>

<template>
  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path :d="path" /></svg>
</template>
