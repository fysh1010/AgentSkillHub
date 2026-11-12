<script setup lang="ts">
import { computed } from "vue";
import AgentIcon from "./AgentIcon.vue";
const props = withDefaults(defineProps<{
  agents: { id: string; name: string; on: boolean; title: string; disabled?: boolean; external?: boolean }[];
  busy?: boolean;
  controls?: boolean;
  label?: string;
}>(), { controls:true, label:"点击 Agent 图标分配" });
defineEmits<{ toggle:[id:string]; bulk:[mode:"all" | "none" | "invert"] }>();
const count = computed(() => props.agents.filter(a => a.on).length);
</script>

<template>
  <div class="agent-assignment-list" @click.stop>
    <div class="assignment-top"><span>Agent <b>{{ count }}</b><span class="assignment-total"> / {{ agents.length }}</span></span>
      <div v-if="controls && agents.some(a => !a.disabled)" class="assignment-actions"><button :disabled="busy" @click="$emit('bulk', 'all')">全选</button><button :disabled="busy" @click="$emit('bulk', 'none')">全不选</button><button :disabled="busy" @click="$emit('bulk', 'invert')">反选</button></div>
    </div>
    <div class="agent-toggles" role="group" :aria-label="label">
      <button v-for="a in agents" :key="a.id" class="agent-toggle" :class="{ on:a.on, external:a.external }" :data-agent="a.id" :title="a.title" :aria-label="a.title" :aria-pressed="a.on" :disabled="busy || a.disabled" @click="$emit('toggle', a.id)">
        <AgentIcon :id="a.id" :name="a.name" :size="17" /><i v-if="a.on" class="enabled-dot"></i>
      </button>
      <span v-if="!agents.length" class="assignment-empty">先到设置添加本机 Agent</span>
    </div>
  </div>
</template>

<style scoped>
.agent-assignment-list { display:grid; gap:8px; min-width:0; }
.assignment-top { display:flex; align-items:center; justify-content:space-between; gap:12px; color:var(--muted); font-size:11px; }
.assignment-top b { color:var(--text); margin-left:5px; font-weight:600; }
.assignment-total { color:var(--faint); }
.assignment-actions { display:flex; gap:10px; }
.assignment-actions button { color:var(--muted); background:none; border:0; cursor:pointer; font-size:11px; padding:0; }
.assignment-actions button:hover { color:var(--text); }
.assignment-actions button:disabled { opacity:.45; cursor:wait; }
.agent-toggles { display:flex; align-items:center; flex-wrap:wrap; gap:2px; }
.agent-toggle { position:relative; display:grid; place-items:center; width:24px; height:24px; padding:2px; border:1px solid var(--border-soft); border-radius:6px; background:var(--panel); cursor:pointer; transition:background .15s,border-color .15s; }
.agent-toggle :deep(.agent-icon) { opacity:.4; filter:grayscale(1); transition:opacity .15s,filter .15s; }
.agent-toggle.on { background:var(--green-soft); border-color:color-mix(in srgb,var(--green) 38%,var(--border)); }
.agent-toggle.external { opacity:1 !important; background:var(--amber-soft); border-color:color-mix(in srgb,var(--amber) 35%,var(--border)); }
.agent-toggle.external :deep(.agent-icon) { opacity:.9; filter:none; }
.agent-toggle.on :deep(.agent-icon), .agent-toggle:hover:not(:disabled) :deep(.agent-icon) { filter:none; opacity:1; }
.agent-toggle:focus-visible { outline:2px solid var(--accent); outline-offset:2px; }
.agent-toggle:disabled { cursor:default; }
.agent-toggle:disabled:not(.on) { opacity:.55; }
.enabled-dot { position:absolute; right:2px; bottom:2px; width:5px; height:5px; border-radius:50%; background:var(--green); border:1px solid var(--panel); }
.assignment-empty { color:var(--faint); font-size:12px; }
</style>
