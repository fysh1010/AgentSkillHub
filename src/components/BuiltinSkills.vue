<script setup lang="ts">
import { computed, ref, watch, onMounted } from "vue";
import { discovery, discoverMachine, store, loadAll, toast } from "../store";
import { builtinSourceSkills, adoptBuiltinSkill, revealPath } from "../api/tauri";
import type { BuiltinSource, ExternalSkill } from "../types";
const keyword = ref("");
const selectedSource = ref("");
const selectedProduct = ref("all");
const sourceLists = ref<Record<string, ExternalSkill[]>>({});
const loadingSource = ref("");
const sourceErrors = ref<Record<string, string>>({});
let sourceRequest = 0;
const adopting = ref("");
const sources = computed(() => discovery.inventory?.sources || []);
const skillCount = computed(() => sources.value.reduce((n,s) => n+s.skill_count,0));
const products = computed(() => [...new Set(sources.value.map(s => productName(s.group)))].sort((a,b) => a.localeCompare(b,"zh")));
const visibleSources = computed(() => sources.value.filter(s => (selectedProduct.value === "all" || productName(s.group) === selectedProduct.value) && `${s.name} ${s.group} ${s.skills_dir}`.toLowerCase().includes(keyword.value.trim().toLowerCase())));
const activeSource = computed(() => visibleSources.value.find(s => s.id === selectedSource.value) || visibleSources.value[0]);
const skillsKeyword = ref("");
const visibleSkills = computed(() => (sourceLists.value[activeSource.value?.id || ""] || []).filter(s => `${s.name} ${s.description} ${s.dir_name}`.toLowerCase().includes(skillsKeyword.value.trim().toLowerCase())));
function productName(group: string) {
  for (const [prefix, name] of [["WorkBuddy", "WorkBuddy"], ["Codex", "Codex"], ["Claude", "Claude Code"], ["豆包", "豆包"], ["Qoder CN", "Qoder CN"], ["Qoder", "Qoder"], ["Trae CN", "Trae CN"]]) {
    if (group.startsWith(prefix)) return name;
  }
  return group;
}
async function openPath(path: string) {
  try { await revealPath(path); } catch (e) { toast(String(e), "error"); }
}
async function openSource(source: BuiltinSource) {
  selectedSource.value = source.id;
  skillsKeyword.value = "";
  if (sourceLists.value[source.id]) return;
  const request = ++sourceRequest;
  loadingSource.value = source.id;
  sourceErrors.value[source.id] = "";
  try { sourceLists.value[source.id] = await builtinSourceSkills(source.id); }
  catch (e) { sourceErrors.value[source.id] = String(e); }
  finally { if (request === sourceRequest) loadingSource.value = ""; }
}
async function refresh() { await discoverMachine(); }
function alreadyInLibrary(dir: string) { return store.skills.some(s => s.key === dir); }
async function adopt(skill: ExternalSkill) {
  const id=activeSource.value?.id;
  if (!id || adopting.value) return;
  adopting.value=skill.dir_name;
  try { await adoptBuiltinSkill(id,skill.dir_name); await loadAll(); toast(`已复制「${skill.name}」到技能库`,"success"); }
  catch(e) { toast(String(e),"error"); }
  finally { adopting.value=""; }
}
watch(() => discovery.inventory, () => { sourceRequest++; sourceLists.value={}; sourceErrors.value={}; loadingSource.value=""; if(activeSource.value) void openSource(activeSource.value); });
watch(() => activeSource.value?.id, () => { if(activeSource.value) void openSource(activeSource.value); }, {immediate:true});
onMounted(() => { if(!discovery.inventory) void discoverMachine(); });
</script>
<template><section>
<div class="builtin-heading"><div><h2>工具内置技能</h2><p>{{ skillCount }} 个技能目录 · {{ sources.length }} 处来源；可查看位置并复制到技能库。</p></div><button class="btn" :disabled="discovery.loading" @click="refresh">{{ discovery.loading ? '扫描中…' : '↻ 重新扫描' }}</button></div>
<div class="builtin-filters"><input class="input" v-model="keyword" placeholder="搜索来源名称或位置…" aria-label="搜索内置技能来源" /><select class="input" v-model="selectedProduct" aria-label="按工具筛选技能来源"><option value="all">所有工具</option><option v-for="product in products" :key="product" :value="product">{{ product }}</option></select></div>
<div v-if="discovery.error" class="discovery-alert error">{{ discovery.error }}</div>
      <p class="inventory-note">只读浏览工具随附与插件缓存中的标准 SKILL.md。数量按技能目录统计，包含不同版本副本，不代表运行时启用数量。</p>
      <div v-if="visibleSources.length" class="builtin-browser card">
        <aside class="source-list"><div class="source-list-label">技能来源 <span>{{ visibleSources.length }}</span></div><button v-for="source in visibleSources" :key="source.id" :class="{ active: activeSource?.id === source.id }" @click="openSource(source)"><span class="source-product">{{ productName(source.group) }}</span><b>{{ source.name }}</b><span class="source-count">{{ source.skill_count }} 个技能</span></button></aside>
        <section v-if="activeSource" class="source-detail">
          <header><div><span class="eyebrow">{{ productName(activeSource.group) }}</span><h2>{{ activeSource.name }}</h2></div><span class="badge gray">只读来源</span></header>
          <div class="source-location"><span>来源位置</span><code>{{ activeSource.skills_dir }}</code><button class="text-button" @click="openPath(activeSource.skills_dir)">打开目录 ↗</button></div>
          <div v-if="activeSource.skills_dir.toLowerCase().replaceAll('/', '\\') !== activeSource.resolved_dir.toLowerCase().replaceAll('/', '\\')" class="source-location"><span>实际位置</span><code>{{ activeSource.resolved_dir }}</code></div>
          <div class="source-skill-toolbar"><b>技能清单 <span>{{ activeSource.skill_count }}</span></b><input v-model="skillsKeyword" class="input" placeholder="在此来源中搜索技能…" aria-label="搜索来源内技能" /></div>
          <div v-if="loadingSource === activeSource.id" class="empty">正在读取技能清单…</div>
          <div v-else-if="sourceErrors[activeSource.id]" class="discovery-alert error">{{ sourceErrors[activeSource.id] }}<button class="btn" @click="openSource(activeSource)">重试</button></div>
          <div v-else-if="!sourceLists[activeSource.id]" class="empty"><button class="btn" @click="openSource(activeSource)">读取此来源的技能</button></div>
          <div v-else class="builtin-skills"><article v-for="skill in visibleSkills" :key="skill.dir_name"><div class="builtin-skill-icon">◇</div><div><h3>{{ skill.name }} <span v-if="!skill.healthy" class="badge amber">元信息不完整</span></h3><p>{{ skill.description || '该技能暂未提供描述。' }}</p><code>{{ skill.path }}</code></div><button class="btn" :disabled="!!adopting || alreadyInLibrary(skill.dir_name)" @click="adopt(skill)">{{ alreadyInLibrary(skill.dir_name) ? "已在技能库" : adopting === skill.dir_name ? "复制中…" : "复制到技能库" }}</button><button class="icon-button" title="打开此技能目录" @click="openPath(skill.path)">↗</button></article><div v-if="!visibleSkills.length" class="empty">没有匹配的技能。</div></div>
        </section>
      </div>
      <div v-else class="card empty"><div class="icon">◇</div>{{ discovery.loading ? '正在发现内置技能…' : '当前范围未发现标准内置技能。工具版本、安装位置或尚未下载技能都可能影响结果。' }}<button class="btn" @click="refresh">重新扫描</button></div>
</section></template>
<style scoped>
.builtin-heading { display:flex; justify-content:space-between; gap:18px; align-items:center; margin-bottom:20px; }.builtin-heading h2 { font-size:17px; }.builtin-heading p,.inventory-note { color:var(--muted); font-size:11px; margin-top:8px; }.builtin-filters { display:flex; gap:12px; }.builtin-filters input { flex:1; }.builtin-filters select { width:180px; }.inventory-note { margin-bottom:18px; line-height:1.8; }
.text-button,.icon-button { background:none; border:0; cursor:pointer; color:var(--accent); font:inherit; font-size:12px; }.icon-button { font-size:17px; flex-shrink:0; }
.builtin-browser { display:grid; grid-template-columns:240px minmax(0,1fr); padding:0; overflow:hidden; min-height:500px; }
.source-list { border-right:1px solid var(--border); background:var(--bg-soft); padding:14px 10px; max-height:690px; overflow:auto; }
.source-list-label { padding:4px 9px 13px; display:flex; justify-content:space-between; color:var(--faint); font-size:11px; }
.source-list button { width:100%; border:1px solid transparent; background:transparent; padding:11px; border-radius:7px; text-align:left; display:flex; flex-direction:column; gap:4px; cursor:pointer; margin-bottom:4px; color:var(--text); }
.source-list button:hover { background:var(--panel-2); }
.source-list button.active { background:var(--nav-active-bg); border-color:var(--border); box-shadow:none; }
.source-product { color:var(--accent); font-size:10px; }
.source-list button b { font-size:12px; font-weight:550; overflow-wrap:anywhere; }
.source-count { color:var(--faint); font-size:11px; }
.source-detail { padding:24px; min-width:0; }
.source-detail > header { display:flex; align-items:center; justify-content:space-between; gap:15px; margin-bottom:18px; }
.source-detail h2 { font-size:18px; overflow-wrap:anywhere; }
.source-location { padding:12px; display:flex; flex-wrap:wrap; gap:6px 12px; background:var(--bg-soft); border:1px solid var(--border-soft); border-radius:6px; margin-bottom:6px; }
.source-location > span { font-size:10px; color:var(--muted); width:100%; }
.source-location code { font-size:12px; overflow-wrap:anywhere; flex:1; min-width:0; }
.source-skill-toolbar { display:flex; justify-content:space-between; align-items:center; gap:10px; margin:24px 0 6px; }
.source-skill-toolbar b { font-size:12px; white-space:nowrap; }
.source-skill-toolbar b span { color:var(--faint); margin-left:6px; font-weight:400; }
.source-skill-toolbar input { max-width:230px; font-size:11px; }
.builtin-skills { max-height:470px; overflow:auto; }
.builtin-skills article { display:flex; gap:12px; padding:17px 0; border-bottom:1px solid var(--border-soft); align-items:flex-start; }
.builtin-skill-icon { width:29px; height:29px; background:var(--accent-soft); color:var(--accent); display:grid; place-items:center; border-radius:6px; flex-shrink:0; }
.builtin-skills article > div:nth-child(2) { flex:1; min-width:0; }
.builtin-skills h3 { font-size:13px; font-weight:600; }
.builtin-skills p { font-size:12px; color:var(--muted); margin:5px 0 8px; overflow-wrap:anywhere; }
.builtin-skills code { font-size:11px; color:var(--faint); overflow-wrap:anywhere; }
.discovery-alert { padding:12px 16px; background:var(--amber-soft); border:1px solid #e9dabb; border-radius:8px; font-size:12px; margin-bottom:14px; display:flex; align-items:center; justify-content:space-between; gap:12px; }
.discovery-alert.error { background:var(--red-soft); color:var(--red); border-color:#e4c3bd; }
@media(max-width:1100px) { .builtin-browser { grid-template-columns:190px minmax(0,1fr); }.source-detail { padding:16px; }.builtin-skills article { flex-wrap:wrap; }.builtin-skills article .btn { margin-left:41px; font-size:10px; } }
</style>
