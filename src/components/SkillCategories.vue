<script setup lang="ts">
import { computed } from "vue";
import type { SkillInfo } from "../types";
import { categoryLabel, inSkillCategory, ROOT_CATEGORY, skillCategories } from "../utils/skillCategories";

const props = defineProps<{ skills: SkillInfo[]; modelValue: string }>();
defineEmits<{ "update:modelValue": [category: string] }>();
const items = computed(() => [
  { id: "", name: "全部技能", count: props.skills.length },
  { id: ROOT_CATEGORY, name: "未分类", count: props.skills.filter(s => s.category === ROOT_CATEGORY).length },
  ...skillCategories(props.skills).map(id => ({ id, name: categoryLabel(id.split("/").pop()!), count: props.skills.filter(s => inSkillCategory(s.category, id)).length })),
]);
</script>

<template>
  <nav class="skill-categories" aria-label="技能文件夹分类">
    <div class="category-heading"><span>文件夹分类</span><small>{{ items.length - 2 }}</small></div>
    <button v-for="item in items" :key="item.id" type="button" :class="{ active: modelValue === item.id }" :style="{ paddingLeft: 10 + Math.min(item.id.split('/').length - 1, 8) * 10 + 'px' }" :aria-current="modelValue === item.id ? 'true' : undefined" :title="item.id || item.name" @click="$emit('update:modelValue', item.id)">
      <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="1.6" aria-hidden="true"><path d="M3 7V5a1 1 0 0 1 1-1h5l2 3h9a1 1 0 0 1 1 1v11a1 1 0 0 1-1 1H4a1 1 0 0 1-1-1Z" /></svg>
      <span>{{ item.name }}</span><small>{{ item.count }}</small>
    </button>
    <p>目录决定主分类<br />标签补充多个用途</p>
  </nav>
</template>

<style scoped>
.skill-categories { position:sticky; top:0; align-self:start; display:grid; gap:4px; padding:15px 10px; border:1px solid var(--border-soft); border-radius:var(--radius); background:var(--panel); }
.category-heading { display:flex; justify-content:space-between; padding:0 10px 14px; color:var(--muted); font-size:11px; letter-spacing:.06em; }
.category-heading small { color:var(--faint); }
button { display:flex; align-items:center; gap:9px; width:100%; padding:10px; border:1px solid transparent; border-radius:6px; background:none; color:var(--muted); text-align:left; font:inherit; font-size:12px; cursor:pointer; }
button:hover { color:var(--text); background:var(--bg-soft); }
button.active { color:var(--accent); background:var(--accent-soft); }
button span { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
button small { font-size:11px; font-variant-numeric:tabular-nums; opacity:.75; }
button svg { flex-shrink:0; }
p { padding:14px 10px 0; margin-top:8px; border-top:1px solid var(--border-soft); font-size:10px; color:var(--faint); line-height:1.8; }
@media (max-width:1000px) { .skill-categories { position:static; display:flex; flex-wrap:wrap; padding:8px; } .category-heading,p { display:none; } button { width:auto; padding:7px 9px; } }
</style>
