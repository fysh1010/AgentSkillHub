<script setup lang="ts">
import { computed, ref, watch } from "vue";
import { store, toast } from "../store";
import { assignmentAgents, assignmentChanges, previewAssignments } from "../composables/useSkillAssignments";
import SkillIcon from "./SkillIcon.vue";
import SkillAssignments from "./SkillAssignments.vue";
import type { SkillInfo } from "../types";
defineEmits<{ detail:[skill:SkillInfo] }>();
const keyword = ref("");
const checked = ref<string[]>([]);
const target = ref("all");
const mode = ref<"all" | "none" | "invert">("all");
const visible = computed(() => store.skills.filter(s => `${s.name} ${s.key} ${s.description}`.toLowerCase().includes(keyword.value.trim().toLowerCase())));
const writable = computed(() => assignmentAgents.value.filter(a => a.writable));
const visibleKeys = computed(() => visible.value.map(s => s.key));
const selectedKeys = computed(() => visibleKeys.value.filter(key => checked.value.includes(key)));
watch(() => store.skills.map(s => s.key), keys => { checked.value = checked.value.filter(key => keys.includes(key)); });
function select(mode: "all" | "none" | "invert") {
  const selection = new Set(checked.value);
  for (const key of visibleKeys.value) {
    if (mode === "none" || mode === "invert" && selection.has(key)) selection.delete(key);
    else selection.add(key);
  }
  checked.value = [...selection];
}
function applySelection() {
  const ids = target.value === "all" ? writable.value.map(a => a.id) : writable.value.filter(a => a.id === target.value).map(a => a.id);
  if (!ids.length) { toast("请先在设置中允许配置 Agent"); return; }
  previewAssignments(assignmentChanges(ids, selectedKeys.value, mode.value), "批量分配 · " + selectedKeys.value.length + " 个技能");
}
</script>

<template>
  <section class="allocation">
    <header class="allocation-head"><div><h2>按技能分配</h2><p>图标亮起即已启用，点击即可切换。悬停查看 Agent 名称。</p></div><input v-model="keyword" class="input" placeholder="搜索技能名称 / 描述…" aria-label="搜索待分配技能" /></header>
    <div class="selection-bar">
      <span class="selection-count">已选 <b>{{ selectedKeys.length }}</b><span> / {{ visible.length }}</span></span>
      <div class="selection-buttons"><button class="btn ghost" @click="select('all')">全选</button><button class="btn ghost" @click="select('none')">全不选</button><button class="btn ghost" @click="select('invert')">反选</button></div>
      <div class="selection-apply"><select v-model="target" class="input" aria-label="批量分配目标"><option value="all">全部可配置 Agent</option><option v-for="a in writable" :key="a.id" :value="a.id">{{ a.name }}</option></select><select v-model="mode" class="input" aria-label="批量分配操作"><option value="all">启用</option><option value="none">停用</option><option value="invert">反转启停</option></select><button class="btn primary" :disabled="!selectedKeys.length || store.assigning || !writable.length" @click="applySelection">预览分配</button></div>
    </div>
    <div class="allocation-table card">
      <div class="table-caption"><span>技能 <b>{{ visible.length }}</b></span><span>分配到 Agent</span></div>
      <div v-for="skill in visible" :key="skill.key" class="allocation-row" :class="{ selected: checked.includes(skill.key) }">
        <label class="skill-select"><input v-model="checked" type="checkbox" :value="skill.key" :aria-label="'选择技能 ' + skill.name" /><SkillIcon :name="skill.name" :identity="skill.key" :description="skill.description" :src="skill.icon_b64" /><span class="allocation-skill"><button type="button" class="skill-open" :title="skill.key" @click.stop.prevent="$emit('detail', skill)"><b>{{ skill.name }}</b></button><span :title="skill.description">{{ skill.description || '暂无描述' }}</span><code v-if="skill.key !== skill.name">{{ skill.key }}</code></span></label>
        <SkillAssignments :skill-key="skill.key" />
      </div>
      <div v-if="!visible.length" class="empty">{{ store.skills.length ? '没有匹配的技能' : '技能库为空，先添加或导入技能' }}</div>
    </div>
    <p class="allocation-note">仅向设置中已开启的 Agent 分配。批量选择只作用于当前筛选结果；琥珀色同名外部技能会保留并跳过。</p>
  </section>
</template>

<style scoped>
.allocation-head { display:flex; align-items:center; justify-content:space-between; gap:24px; margin-bottom:20px; }
h2 { font-size:20px; font-weight:650; letter-spacing:-.4px; }
.allocation-head p, .allocation-note { color:var(--muted); font-size:12px; margin-top:5px; }
.allocation-head .input { width:270px; }
.selection-bar { display:flex; align-items:center; gap:12px; margin:16px 0 12px; flex-wrap:wrap; }
.selection-count { font-size:12px; color:var(--muted); }
.selection-count b { color:var(--text); margin-left:4px; }
.selection-count > span { color:var(--faint); }
.selection-buttons { display:flex; gap:2px; border-left:1px solid var(--border); padding-left:8px; }
.selection-buttons .btn { padding:4px 8px; font-size:12px; }
.selection-apply { margin-left:auto; display:flex; gap:7px; }
.selection-apply .input { width:auto; padding:6px 10px; font-size:12px; }
.allocation-table { padding:0; overflow:visible; }
.table-caption, .allocation-row { display:grid; grid-template-columns:minmax(240px,1fr) minmax(340px,1fr); gap:24px; align-items:center; }
.table-caption { color:var(--muted); padding:12px 18px; font-size:11px; background:var(--bg-soft); border-radius:var(--radius) var(--radius) 0 0; }
.table-caption b { margin-left:5px; font-weight:500; color:var(--faint); }
.allocation-row { padding:17px 18px; border-top:1px solid var(--border-soft); transition:background .15s; }
.allocation-row.selected { background:var(--accent-soft); }
.skill-select { display:flex; align-items:center; gap:12px; min-width:0; cursor:pointer; }
.skill-select input { accent-color:var(--accent); width:14px; height:14px; flex-shrink:0; }
.allocation-skill { display:grid; gap:3px; min-width:0; }
.skill-open { display:block; width:100%; text-align:left; border:0; background:none; color:var(--text); font:inherit; cursor:pointer; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.skill-open:hover { color:var(--accent); text-decoration:underline; }
.allocation-skill b { font-size:13px; font-weight:600; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.allocation-skill > span, .allocation-skill code { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--muted); font-size:11px; }
.allocation-skill code { color:var(--faint); }
.allocation-note { margin:14px 0 0; }
@media (max-width:1150px) { .table-caption,.allocation-row { grid-template-columns:minmax(200px,1fr) minmax(290px,1fr); gap:16px; } }
</style>
