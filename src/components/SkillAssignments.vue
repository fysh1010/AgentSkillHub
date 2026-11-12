<script setup lang="ts">
import { computed } from "vue";
import { hasExternalSkill, isAssigned, store } from "../store";
import { assignmentAgents, assignmentChanges, previewAssignments, toggleAssignment } from "../composables/useSkillAssignments";
import AgentAssignmentList from "./AgentAssignmentList.vue";
const props = withDefaults(defineProps<{ skillKey: string; controls?: boolean }>(), { controls: true });
const writable = computed(() => assignmentAgents.value.filter(a => a.writable && !external(a.id)).map(a => a.id));
const agents = computed(() => assignmentAgents.value.map(a => ({ ...a, on:isAssigned(a.id, props.skillKey), title:title(a), disabled:!a.writable || external(a.id), external:external(a.id) })));
function external(id: string) {
  return hasExternalSkill(id, props.skillKey);
}
function bulk(mode: "all" | "none" | "invert") {
  previewAssignments(assignmentChanges(writable.value, [props.skillKey], mode), "分配技能 · " + props.skillKey);
}
function title(a: typeof assignmentAgents.value[number]) {
  if (!a.writable) return a.name + (a.installed ? ' · 在设置中开启「允许配置」后可分配' : ' · 未确认安装，仅显示目录');
  if (external(a.id)) return a.name + ' · 存在同名外部技能，请切换到「Agent 自有技能」查看';
  return a.name + (isAssigned(a.id, props.skillKey) ? " · 已启用，点击停用" : " · 未启用，点击启用");
}
</script>

<template>
  <AgentAssignmentList class="skill-assignments" :agents="agents" :busy="store.assigning" :controls="controls" label="点击 Agent 图标启停技能" @toggle="id => toggleAssignment(id, skillKey)" @bulk="bulk" />
</template>

