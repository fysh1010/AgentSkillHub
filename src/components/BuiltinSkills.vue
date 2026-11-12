<script setup lang="ts">
import { computed, onMounted, ref, watch } from "vue";
import { discovery, discoverMachine, loadAll, openAgentSettings, store, toast } from "../store";
import { builtinSourceSkills, adoptBuiltinSkill, revealPath } from "../api/tauri";
import type { BuiltinSource, ExternalSkill } from "../types";
import { installedBuiltinSources, isSharedSkillSource, sourceProduct } from "../utils/builtinSources";
import AgentIcon from "./AgentIcon.vue";
import SkillIcon from "./SkillIcon.vue";
const props = defineProps<{ initialProduct?: string; dialog?: boolean }>();
const keyword = ref("");
const skillKeyword = ref("");
const selectedId = ref("");
const skills = ref<ExternalSkill[]>([]);
const loading = ref(false);
const error = ref("");
const adopting = ref("");
let request = 0;
const sources = computed(() => installedBuiltinSources(discovery.inventory));
const visibleSources = computed(() => sources.value.filter(s => (s.group + " " + s.name + " " + s.skills_dir).toLowerCase().includes(keyword.value.trim().toLowerCase())));
const activeSource = computed(() => visibleSources.value.find(s => s.id === selectedId.value)
  || visibleSources.value.find(s => sourceProduct(s.group) === props.initialProduct) || visibleSources.value[0]);
const visibleSkills = computed(() => skills.value.filter(s => (s.name + " " + s.description + " " + s.dir_name).toLowerCase().includes(skillKeyword.value.trim().toLowerCase())));
const count = computed(() => sources.value.reduce((n,s) => n + s.skill_count, 0));
function agentId(source: BuiltinSource) {
  if (isSharedSkillSource(source)) return "agents-shared";
  return store.agents.find(a => sourceProduct(a.agent.name) === sourceProduct(source.group))?.agent.id || source.group;
}
function inLibrary(name: string) { return store.skills.some(s => s.key.split("/").pop() === name); }
function assigned(name: string) {
  const agent = store.agents.find(a => a.agent.id === (activeSource.value && agentId(activeSource.value)));
  return agent?.assigned_skills.some(key => key.split("/").pop() === name);
}
watch(() => [activeSource.value?.id, store.revision, discovery.inventory], async () => {
  const source = activeSource.value;
  const id = ++request;
  error.value = "";
  skills.value = [];
  if (!source) { loading.value = false; return; }
  loading.value = true;
  try { const result = await builtinSourceSkills(source.id); if (id === request) skills.value = result; }
  catch(e) { if (id === request) error.value = String(e); }
  finally { if (id === request) loading.value = false; }
}, { immediate:true });
async function open(path: string) {
  try { await revealPath(path); } catch(e) { toast(String(e), "error"); }
}
async function adopt(skill: ExternalSkill) {
  const source = activeSource.value;
  if (!source || adopting.value || loading.value) return;
  adopting.value = skill.dir_name;
  try { await adoptBuiltinSkill(source.id, skill.dir_name); await loadAll(true); toast("已复制「" + skill.name + "」到技能库", "success"); }
  catch(e) { toast(String(e), "error"); }
  finally { adopting.value = ""; }
}
onMounted(() => { if (!discovery.inventory) void discoverMachine(); });
</script>

<template>
  <section class="builtin-root">
    <header v-if="!dialog" class="builtin-heading"><div><h2>用户技能</h2><p>{{ count }} 个技能目录 · {{ sources.length }} 处用户来源</p></div><button class="btn" @click="openAgentSettings()">管理 Agent</button></header>
    <p class="inventory-note">仅查看已开启工具的用户安装、分配和自定义技能。内置技能由各 Agent 自行管理。</p>
    <div class="builtin-filters"><input v-model="keyword" class="input" placeholder="搜索工具或用户目录…" aria-label="搜索工具技能来源" /><button class="btn" :disabled="discovery.loading" @click="discoverMachine()">{{ discovery.loading ? '扫描中…' : '↻ 重新扫描' }}</button></div>
    <div v-if="sources.length" class="builtin-browser card">
      <aside class="source-list"><div class="source-list-label">用户技能来源</div><button v-for="source in visibleSources" :key="source.id" class="source-item" :title="source.skills_dir" :class="{active:activeSource?.id === source.id}" @click="selectedId = source.id; skillKeyword = ''"><AgentIcon :id="agentId(source)" :name="source.group" :size="24" /><span><b>{{ sourceProduct(source.group) }}</b><small>{{ source.skill_count }} 个用户技能 · {{ isSharedSkillSource(source) ? '跨工具共享' : '用户目录' }}</small></span></button></aside>
      <section v-if="activeSource" class="source-detail">
        <header><div><small>{{ sourceProduct(activeSource.group) }}</small><h3>{{ isSharedSkillSource(activeSource) ? '通用共享技能' : '用户技能' }}</h3></div><span class="badge gray">用户目录 · 只读查看</span></header>
        <div class="source-location"><span>{{ isSharedSkillSource(activeSource) ? '通用共享目录' : '用户 Skills 目录' }}</span><code>{{ activeSource.skills_dir }}</code><button class="btn ghost" @click="open(activeSource.skills_dir)">打开目录 ↗</button></div>
        <p v-if="isSharedSkillSource(activeSource)" class="inventory-note">.agents/skills 是跨客户端共享约定，不属于某个 Agent；实际加载范围取决于各 Agent 的支持。</p>
        <div class="source-toolbar"><span>技能清单 {{ visibleSkills.length }}</span><input v-model="skillKeyword" class="input" placeholder="在此工具中搜索技能…" /></div>
        <p v-if="error" class="source-error" role="alert">{{ error }}</p>
        <div v-else-if="loading" class="empty">正在读取用户技能…</div>
        <div v-else class="source-skills"><article v-for="skill in visibleSkills" :key="skill.dir_name" class="source-skill"><SkillIcon :name="skill.name" :identity="skill.path" :description="skill.description" /><div><b>{{ skill.name }}</b><span class="badge" :class="assigned(skill.dir_name) ? 'green' : 'gray'">{{ assigned(skill.dir_name) ? '本工具分配' : '用户安装 / 自定义' }}</span><p>{{ skill.description }}</p><code>{{ skill.path }}</code></div><button class="btn" :disabled="!!adopting || inLibrary(skill.dir_name)" @click="adopt(skill)">{{ adopting === skill.dir_name ? '导入中…' : inLibrary(skill.dir_name) ? '已在技能库' : '导入技能库' }}</button></article><div v-if="!visibleSkills.length" class="empty">没有匹配的用户技能</div></div>
      </section>
      <div v-else class="empty">没有匹配的工具</div>
    </div>
    <div v-else class="card empty">已开启的 Agent 暂无用户技能。<button class="btn" @click="openAgentSettings()">前往 Agent 工具</button></div>
  </section>
</template>

<style scoped>
.builtin-heading,.builtin-filters,.source-detail > header,.source-toolbar,.source-location,.source-skill { display:flex; align-items:center; gap:12px; }
.builtin-heading,.source-detail > header,.source-toolbar { justify-content:space-between; }
.builtin-heading h2 { font-size:17px; }.builtin-heading p,.inventory-note { margin-top:8px; color:var(--muted); font-size:12px; line-height:1.8; }
.inventory-note { margin-bottom:16px; }.builtin-filters { margin-bottom:16px; }.builtin-filters input { flex:1; }
.builtin-browser { display:grid; grid-template-columns:225px minmax(0,1fr); padding:0; overflow:hidden; min-height:420px; }
.source-list { background:var(--bg-soft); border-right:1px solid var(--border); padding:16px 10px; max-height:650px; overflow:auto; }
.source-list-label { color:var(--faint); font-size:11px; margin:0 8px 12px; }
.source-item { display:flex; align-items:center; gap:10px; width:100%; text-align:left; padding:12px 8px; border:1px solid transparent; border-radius:7px; background:none; color:var(--text); cursor:pointer; margin-bottom:4px; }
.source-item.active { background:var(--panel); border-color:var(--border); }.source-item span { display:grid; gap:5px; min-width:0; }.source-item b { font-size:12px; }.source-item small { font-size:10px; color:var(--muted); }
.source-detail { padding:22px; min-width:0; }.source-detail header small { color:var(--muted); font-size:11px; }.source-detail h3 { font-size:19px; margin-top:7px; }
.source-location { margin:18px 0; padding:14px; flex-wrap:wrap; background:var(--bg-soft); border:1px solid var(--border); border-radius:7px; }.source-location > span { font-size:11px; color:var(--muted); }.source-location code { font-size:11px; overflow-wrap:anywhere; flex:1; }.source-location .btn { font-size:11px; padding:3px; }
.source-toolbar { font-size:12px; margin-bottom:12px; }.source-toolbar .input { width:240px; font-size:11px; }
.source-skills { max-height:520px; overflow:auto; }.source-skill { padding:17px 0; border-bottom:1px solid var(--border-soft); align-items:flex-start; }.source-skill > div { min-width:0; flex:1; }.source-skill b { font-size:13px; margin-right:8px; }.source-skill p { color:var(--muted); font-size:12px; line-height:1.7; margin:7px 0; }.source-skill code { font-size:10px; color:var(--faint); overflow-wrap:anywhere; }.source-skill .btn { font-size:11px; flex-shrink:0; }.source-skill .badge { font-size:9px; }
.source-error { color:var(--red); font-size:12px; }
@media(max-width:1100px) { .builtin-browser { grid-template-columns:190px minmax(0,1fr); }.source-detail { padding:16px; } }
</style>
