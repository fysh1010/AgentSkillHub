<script setup lang="ts">
import { computed, ref, watch } from "vue";
const props = withDefaults(defineProps<{ id: string; name: string; size?: number }>(), { size: 20 });
const failed = ref(false);
const icons: Record<string, string> = {
  codex: "codex-color.svg", claude: "claudecode-color.svg", cursor: "cursor.svg",
  gemini: "geminicli-color.svg", qwen: "qwen-color.svg", qoder: "qoder.svg",
  workbuddy: "workbuddy-color.svg", codebuddy: "codebuddy-color.svg", antigravity: "antigravity-color.svg",
  trae: "trae.svg", windsurf: "windsurf.svg", cline: "cline.svg", copilot: "githubcopilot.svg",
  opencode: "opencode.svg", kilo: "kilocode.svg", kiro: "kiro-color.svg", "roo-code": "roocode.svg",
  kimi: "kimi.svg", openclaw: "openclaw.svg", clawdbot: "openclaw.svg", factory: "factory.svg",
  droid: "factory.svg", goose: "goose.svg", doubao: "doubao-color.svg", grok: "grok.svg",
  zcode: "zcode.png", hermes: "hermes.svg",
  lingma: "lingma.png",
  marvis: "marvis.png",
  "meituan-catpaw": "catpaw.png",
};
const src = computed(() => {
  const id = props.id.toLowerCase();
  const match = Object.keys(icons).find(key => id === key || id.startsWith(key + "-") || id.startsWith(key + "_"));
  return match ? "/agent-icons/" + icons[match] : "";
});
watch(src, () => { failed.value = false; });
</script>

<template>
  <span class="agent-icon" :style="{ width: size + 'px', height: size + 'px' }" aria-hidden="true">
    <svg v-if="id==='agents-shared'" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><path d="M3 7h7l2 2h9v11H3zM3 7V4h7l2 3"/><circle cx="9" cy="13" r="1.5"/><circle cx="16" cy="16" r="1.5"/><path d="m10.5 13.5 4 2"/></svg>
    <img v-else-if="src && !failed" :src="src" alt="" @error="failed = true" />
    <span v-else class="agent-initial">{{ name.replace(/^OpenAI\s+/i, '').slice(0, 1).toUpperCase() }}</span>
  </span>
</template>

<style scoped>
.agent-icon { display:inline-flex; align-items:center; justify-content:center; flex-shrink:0; vertical-align:middle; }
.agent-icon img { width:100%; height:100%; object-fit:contain; }
.agent-icon svg { width:100%; height:100%; }
.agent-initial { display:grid; place-items:center; width:100%; height:100%; background:var(--panel-2); border:1px solid var(--border); border-radius:4px; color:var(--muted); font-size:11px; font-weight:600; }
</style>
