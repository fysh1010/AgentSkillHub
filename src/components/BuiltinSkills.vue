<script setup lang="ts">
import { computed, ref, watch, onMounted } from "vue";
import { discovery, discoverMachine, store, loadAll, toast } from "../store";
import { builtinSourceSkills, adoptBuiltinSkill, revealPath, saveBuiltinSource, removeBuiltinSource } from "../api/tauri";
import type { BuiltinSource, CustomBuiltinSource, ExternalSkill } from "../types";
import SkillIcon from "./SkillIcon.vue";
import { installedBuiltinSources, sourceProduct } from "../utils/builtinSources";
const props = defineProps<{ initialProduct?: string; dialog?: boolean }>();
const keyword = ref("");
const selectedSource = ref("");
const expandedProduct = ref(props.initialProduct === "all" ? "" : props.initialProduct || "");
const sourceLists = ref<Record<string, ExternalSkill[]>>({});
const loadingSource = ref("");
const sourceErrors = ref<Record<string, string>>({});
let sourceRequest = 0;
const adopting = ref("");
const managing = ref(false);
const saving = ref(false);
const formError = ref("");
const emptySource = (): CustomBuiltinSource => ({ id: "", tool_name: props.initialProduct === "all" ? "" : props.initialProduct || "", name: "", skills_dir: "" });
const sourceForm = ref<CustomBuiltinSource>(emptySource());
const configuredSources = computed(() => store.config?.builtin_sources || []);
const toolNames = computed(() => [...new Set([
  ...(discovery.inventory?.agents.filter(a => !a.shared && (a.installed || a.registered)).map(a => a.name) || []),
  ...configuredSources.value.map(s => s.tool_name),
])].sort((a,b) => a.localeCompare(b,"zh")));
function editSource(source?: CustomBuiltinSource) {
  sourceForm.value = source ? { ...source } : emptySource();
  formError.value = "";
  managing.value = true;
}
async function saveSource() {
  saving.value = true;
  formError.value = "";
  try {
    store.config = await saveBuiltinSource({ ...sourceForm.value });
    expandedProduct.value = sourceProduct(sourceForm.value.tool_name.trim());
    selectedSource.value = "";
    sourceForm.value = emptySource();
    await refresh();
    toast("已保存技能来源", "success");
  } catch (e) { formError.value = String(e); }
  finally { saving.value = false; }
}
async function removeSource(source: CustomBuiltinSource) {
  saving.value = true;
  formError.value = "";
  try {
    store.config = await removeBuiltinSource(source.id);
    if (sourceForm.value.id === source.id) sourceForm.value = emptySource();
    await refresh();
    toast("已移除来源配置，原目录保留", "success");
  } catch (e) { formError.value = String(e); }
  finally { saving.value = false; }
}
const sources = computed(() => installedBuiltinSources(discovery.inventory));
const skillCount = computed(() => sources.value.reduce((n,s) => n+s.skill_count,0));
const visibleSources = computed(() => sources.value.filter(s => `${s.name} ${s.group} ${sourceProduct(s.group)} ${s.skills_dir}`.toLowerCase().includes(keyword.value.trim().toLowerCase())));
const groups = computed(() => {
  const byProduct = new Map<string, BuiltinSource[]>();
  for (const source of visibleSources.value) {
    const name = sourceProduct(source.group);
    if (!byProduct.has(name)) byProduct.set(name, []);
    byProduct.get(name)!.push(source);
  }
  return [...byProduct].map(([name, items]) => ({
    name, items,
    skillCount: items.reduce((count, source) => count + source.skill_count, 0),
    systemCount: items.filter(source => !source.custom && source.group.includes("系统技能")).reduce((count, source) => count + source.skill_count, 0),
    manualCount: items.filter(source => source.custom).reduce((count, source) => count + source.skill_count, 0),
    cacheOnlyCount: items.filter(source => source.cache_only).reduce((count, source) => count + source.skill_count, 0),
  })).sort((a, b) => {
    if (a.name === props.initialProduct) return -1;
    if (b.name === props.initialProduct) return 1;
    return a.name.localeCompare(b.name, "zh");
  });
});
const searching = computed(() => !!keyword.value.trim());
const activeSource = computed(() => {
  const candidates = searching.value ? visibleSources.value : visibleSources.value.filter(source => sourceProduct(source.group) === expandedProduct.value);
  return candidates.find(source => source.id === selectedSource.value) || candidates[0];
});
const skillsKeyword = ref("");
const visibleSkills = computed(() => (sourceLists.value[activeSource.value?.id || ""] || []).filter(s => `${s.name} ${s.description} ${s.dir_name}`.toLowerCase().includes(skillsKeyword.value.trim().toLowerCase())));
function isExpanded(name: string) { return searching.value || expandedProduct.value === name; }
function toggleProduct(name: string) {
  if (searching.value) {
    selectedSource.value = groups.value.find(group => group.name === name)?.items[0]?.id || "";
    return;
  }
  expandedProduct.value = expandedProduct.value === name ? "" : name;
  selectedSource.value = "";
}
function sourceLabel(source: BuiltinSource) {
  const parts = source.name.replaceAll("\\", "/").split("/").filter(Boolean);
  if (parts.length > 1 && (/^\d+\.\d+\.\d+/.test(parts[parts.length - 1]!) || /^[a-f\d]{8,}$/i.test(parts[parts.length - 1]!))) parts.pop();
  return parts[parts.length - 1] || source.name;
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
watch(() => props.initialProduct, value => { expandedProduct.value = value === "all" ? groups.value[0]?.name || "" : value || groups.value[0]?.name || ""; selectedSource.value = ""; });
watch(groups, value => {
  if (value.length && !expandedProduct.value && !selectedSource.value) expandedProduct.value = value[0]!.name;
}, { immediate: true });
watch(() => activeSource.value?.id, () => { if(activeSource.value) void openSource(activeSource.value); }, {immediate:true});
onMounted(() => { if(!discovery.inventory) void discoverMachine(); });
</script>
<template><section class="builtin-root">
<div v-if="!props.dialog" class="builtin-heading"><div><h2>工具已安装与自定义技能</h2><p>{{ skillCount }} 个技能目录 · {{ sources.length }} 处来源；可查看位置并复制到技能库。</p></div></div>
<div class="builtin-filters"><input class="input" v-model="keyword" placeholder="搜索工具、来源或目录…" aria-label="搜索工具技能来源" /><button class="btn" :aria-expanded="managing" @click="managing = !managing">配置来源目录</button><button class="btn" :disabled="discovery.loading || saving" @click="refresh">{{ discovery.loading ? '扫描中…' : '↻ 重新扫描' }}</button></div>
<section v-if="managing" class="card manual-sources">
  <h3>添加明确的技能来源目录</h3>
  <p>自动扫描用户技能目录与明确的系统技能目录，不深搜插件或安装包。可以手动添加直接包含「技能名/SKILL.md」的目录；来源只读，可复制到统一技能库。</p>
  <div v-for="source in configuredSources" :key="source.id" class="manual-source-row">
    <div><b>{{ source.tool_name }} · {{ source.name }}</b><code>{{ source.skills_dir }}</code><small v-if="!sources.some(s => s.id === source.id)" class="source-cache-note">当前未读到标准技能，请检查目录是否存在、层级是否正确。</small></div>
    <button class="btn" :disabled="saving" @click="editSource(source)">编辑</button><button class="btn" :disabled="saving || discovery.loading" @click="removeSource(source)">移除配置</button>
  </div>
  <form class="manual-source-form" @submit.prevent="saveSource">
    <label>所属工具<input v-model="sourceForm.tool_name" class="input" list="builtin-tool-names" placeholder="选择或输入，如 DeepSeek Harness" required :disabled="saving" /></label>
    <datalist id="builtin-tool-names"><option v-for="name in toolNames" :key="name" :value="name" /></datalist>
    <label>来源名称<input v-model="sourceForm.name" class="input" placeholder="如 Office 内置技能、Archify 插件" required :disabled="saving" /></label>
    <label class="directory-field">技能来源目录<input v-model="sourceForm.skills_dir" class="input" placeholder="绝对路径，如 D:/软件/dsh/resources/runtime/office-skills" required :disabled="saving" /></label>
    <p v-if="formError" class="discovery-alert error" role="alert">{{ formError }}</p>
    <div class="manual-source-actions"><span>{{ sourceForm.id ? '编辑来源' : '添加来源' }} · 配置会保存，移除配置只取消本工具的读取记录。</span><button v-if="sourceForm.id" type="button" class="btn" :disabled="saving" @click="editSource()">取消编辑</button><button class="btn primary" :disabled="saving || discovery.loading">{{ saving ? '保存中…' : '保存并扫描' }}</button></div>
  </form>
</section>
<div v-if="discovery.error" class="discovery-alert error">{{ discovery.error }}</div>
      <p v-if="!props.dialog" class="inventory-note">仅展示用户安装、自定义和明确的系统技能来源。插件内部资源与普通 Markdown 不自动纳入；目录中的文件不等于已被目标 Agent 启用。</p>
      <div v-if="visibleSources.length" class="builtin-browser card">
        <aside class="source-list"><div class="source-list-label">按工具浏览 <span>{{ groups.length }} 个工具</span></div>
          <div v-for="group in groups" :key="group.name" class="source-group">
            <button class="product-toggle" :class="{ open: isExpanded(group.name) }" :aria-expanded="isExpanded(group.name)" @click="toggleProduct(group.name)">
              <span class="product-avatar">{{ group.name.replace('OpenAI ', '').slice(0, 2).toUpperCase() }}</span>
              <span class="product-info"><b>{{ group.name }}</b><small>{{ group.items.length }} 处来源 · {{ group.skillCount }} 个目录</small><small v-if="group.systemCount || group.cacheOnlyCount || group.manualCount" class="product-breakdown">{{ group.systemCount }} 系统 · {{ group.skillCount - group.systemCount - group.cacheOnlyCount - group.manualCount }} 用户<span v-if="group.manualCount"> · {{ group.manualCount }} 手动</span><span v-if="group.cacheOnlyCount"> · {{ group.cacheOnlyCount }} 待确认缓存</span></small></span>
              <span class="product-chevron">⌄</span>
            </button>
            <div v-if="isExpanded(group.name)" class="source-children">
              <button v-for="source in group.items" :key="source.id" class="source-item" :class="{ active: activeSource?.id === source.id }" :title="source.name" @click="openSource(source)"><span class="source-name">{{ sourceLabel(source) }}</span><span v-if="source.cache_only" class="source-cache-note">待确认</span><span class="source-count">{{ source.skill_count }}</span></button>
            </div>
          </div>
        </aside>
        <section v-if="activeSource" class="source-detail">
          <header><div class="source-identity"><span class="eyebrow">{{ activeSource.group }}</span><h2>{{ sourceLabel(activeSource) }}</h2><p :title="activeSource.name">{{ activeSource.name }}</p></div><span class="badge" :class="activeSource.cache_only ? 'amber' : 'gray'">{{ activeSource.custom ? '手动来源 · 只读' : activeSource.cache_only ? '缓存待确认' : '只读来源' }}</span></header>
          <p v-if="activeSource.cache_only" class="cache-explanation">此插件目录有技能文件，但未找到安装标记或插件配置；缓存不代表当前可用。</p>
          <div class="source-location"><span>来源位置</span><code>{{ activeSource.skills_dir }}</code><button class="text-button" @click="openPath(activeSource.skills_dir)">打开目录 ↗</button></div>
          <div v-if="activeSource.skills_dir.toLowerCase().replaceAll('/', '\\') !== activeSource.resolved_dir.toLowerCase().replaceAll('/', '\\')" class="source-location"><span>路径联接 → 实际目录</span><code>{{ activeSource.resolved_dir }}</code></div>
          <div class="source-skill-toolbar"><b>技能清单 <span>{{ activeSource.skill_count }}</span></b><input v-model="skillsKeyword" class="input" placeholder="在此来源中搜索技能…" aria-label="搜索来源内技能" /></div>
          <div v-if="loadingSource === activeSource.id" class="empty">正在读取技能清单…</div>
          <div v-else-if="sourceErrors[activeSource.id]" class="discovery-alert error">{{ sourceErrors[activeSource.id] }}<button class="btn" @click="openSource(activeSource)">重试</button></div>
          <div v-else-if="!sourceLists[activeSource.id]" class="empty"><button class="btn" @click="openSource(activeSource)">读取此来源的技能</button></div>
          <div v-else class="builtin-skills"><article v-for="skill in visibleSkills" :key="skill.dir_name"><SkillIcon class="builtin-skill-icon" :name="skill.name" :identity="skill.dir_name" :description="skill.description" /><div><h3>{{ skill.name }} <span v-if="!skill.healthy" class="badge amber">元信息不完整</span></h3><p>{{ skill.description || '该技能暂未提供描述。' }}</p><code>{{ skill.path }}</code></div><button class="btn" :disabled="!!adopting || alreadyInLibrary(skill.dir_name)" @click="adopt(skill)">{{ alreadyInLibrary(skill.dir_name) ? "已在技能库" : adopting === skill.dir_name ? "复制中…" : "复制到技能库" }}</button><button class="icon-button" title="打开此技能目录" @click="openPath(skill.path)">↗</button></article><div v-if="!visibleSkills.length" class="empty">没有匹配的技能。</div></div>
        </section>
        <div v-else class="source-placeholder"><span>⌁</span><b>选择一个来源</b><p>从左侧展开工具，再选择技能来源查看清单与安装目录。</p></div>
      </div>
      <div v-else class="card empty"><div class="icon">◇</div>{{ discovery.loading ? '正在扫描工具技能目录…' : '当前范围未发现标准技能目录。工具版本、安装位置或尚未下载技能都可能影响结果。' }}<button class="btn" @click="refresh">重新扫描</button></div>
</section></template>
<style scoped>
.manual-sources { margin-bottom:18px; }
.manual-sources h3 { font-size:14px; }
.manual-sources > p { color:var(--muted); font-size:12px; line-height:1.7; margin:8px 0 14px; }
.manual-source-row { display:flex; align-items:center; gap:8px; padding:12px 0; border-bottom:1px solid var(--border); }
.manual-source-row > div { flex:1; min-width:0; display:grid; gap:5px; font-size:12px; }
.manual-source-row code { overflow-wrap:anywhere; color:var(--muted); }
.manual-source-form { display:grid; grid-template-columns:repeat(2,minmax(0,1fr)); gap:12px; margin-top:16px; }
.manual-source-form label { display:grid; gap:7px; font-size:12px; }
.manual-source-form .directory-field,.manual-source-form > p,.manual-source-actions { grid-column:1/-1; }
.manual-source-actions { display:flex; align-items:center; gap:10px; flex-wrap:wrap; }
.manual-source-actions span { flex:1; color:var(--muted); font-size:11px; }
@media(max-width:700px) { .manual-source-form { grid-template-columns:1fr; }.builtin-filters { flex-wrap:wrap; }.manual-source-row { flex-wrap:wrap; }.manual-source-row > div { flex-basis:100%; } }
.builtin-heading { display:flex; justify-content:space-between; gap:18px; align-items:center; margin-bottom:20px; }.builtin-heading h2 { font-size:17px; }.builtin-heading p,.inventory-note { color:var(--muted); font-size:11px; margin-top:8px; }.builtin-filters { display:flex; gap:12px; margin-bottom:14px; }.builtin-filters input { flex:1; }.inventory-note { margin-bottom:18px; line-height:1.8; }
.text-button,.icon-button { background:none; border:0; cursor:pointer; color:var(--accent); font:inherit; font-size:12px; }.icon-button { font-size:17px; flex-shrink:0; }
.builtin-browser { display:grid; grid-template-columns:270px minmax(0,1fr); padding:0; overflow:hidden; min-height:500px; }
.source-list { border-right:1px solid var(--border); background:var(--bg-soft); padding:16px 11px; max-height:650px; overflow:auto; }
.source-list-label { padding:2px 9px 13px; display:flex; justify-content:space-between; color:var(--faint); font-size:11px; }
.source-group + .source-group { margin-top:5px; }
.source-list button { width:100%; border:0; text-align:left; display:flex; align-items:center; cursor:pointer; color:var(--text); font:inherit; }
.product-toggle { gap:10px; padding:10px 9px; border-radius:9px; background:transparent; }
.product-toggle:hover,.product-toggle.open { background:var(--panel-2); }
.product-toggle.open .product-chevron { transform:rotate(180deg); color:var(--accent); }
.product-avatar { width:30px; height:30px; display:grid; place-items:center; flex:none; border:1px solid var(--border); border-radius:8px; background:var(--panel); color:var(--accent); font:600 10px "Bahnschrift",sans-serif; }
.product-info { min-width:0; flex:1; display:flex; flex-direction:column; line-height:1.4; }
.product-info b { font-size:12px; white-space:nowrap; overflow:hidden; text-overflow:ellipsis; }
.product-info small { color:var(--faint); font-size:10px; white-space:nowrap; }
.product-info .product-breakdown { color:var(--muted); white-space:normal; }
.product-chevron { color:var(--faint); font-size:17px; line-height:1; transition:transform .15s; }
.source-children { margin:3px 0 9px 23px; padding-left:11px; border-left:1px solid var(--border); }
.source-list .source-item { gap:8px; min-height:35px; padding:7px 9px; border-radius:6px; background:transparent; }
.source-list .source-item:hover { background:var(--panel-2); }
.source-list .source-item.active { background:var(--accent-soft); color:var(--accent); font-weight:600; }
.source-name { flex:1; min-width:0; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; font-size:11px; }
.source-count { color:var(--faint); font:10px "Consolas",monospace; }
.source-cache-note { color:var(--amber); font-size:9px; flex:none; }
.source-placeholder { display:flex; flex-direction:column; align-items:center; justify-content:center; text-align:center; padding:30px; color:var(--muted); }
.source-placeholder span { display:grid; place-items:center; width:45px; height:45px; margin-bottom:15px; background:var(--accent-soft); color:var(--accent); border-radius:12px; font-size:22px; }
.source-placeholder b { color:var(--text); font-size:14px; }
.source-placeholder p { font-size:11px; margin-top:6px; }
.source-detail { padding:24px; min-width:0; }
.source-detail > header { display:flex; align-items:center; justify-content:space-between; gap:15px; margin-bottom:18px; }
.source-detail h2 { font-size:18px; overflow-wrap:anywhere; }
.source-identity { min-width:0; }
.source-identity p { color:var(--faint); font-size:11px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; max-width:650px; margin-top:4px; }
.cache-explanation { margin:-6px 0 14px; color:var(--amber); font-size:11px; line-height:1.5; }
.source-location { padding:12px; display:flex; flex-wrap:wrap; gap:6px 12px; background:var(--bg-soft); border:1px solid var(--border-soft); border-radius:6px; margin-bottom:6px; }
.source-location > span { font-size:10px; color:var(--muted); width:100%; }
.source-location code { font-size:12px; overflow-wrap:anywhere; flex:1; min-width:0; }
.source-skill-toolbar { display:flex; justify-content:space-between; align-items:center; gap:10px; margin:24px 0 6px; }
.source-skill-toolbar b { font-size:12px; white-space:nowrap; }
.source-skill-toolbar b span { color:var(--faint); margin-left:6px; font-weight:400; }
.source-skill-toolbar input { max-width:230px; font-size:11px; }
.builtin-skills { max-height:470px; overflow:auto; }
.builtin-skills article { display:flex; gap:12px; padding:17px 0; border-bottom:1px solid var(--border-soft); align-items:flex-start; }
.builtin-skill-icon { width:29px; height:29px; border-radius:8px; }
.builtin-skills article > div:nth-child(2) { flex:1; min-width:0; }
.builtin-skills h3 { font-size:13px; font-weight:600; }
.builtin-skills p { font-size:12px; color:var(--muted); margin:5px 0 8px; overflow-wrap:anywhere; }
.builtin-skills code { font-size:11px; color:var(--faint); overflow-wrap:anywhere; }
.discovery-alert { padding:12px 16px; background:var(--amber-soft); border:1px solid #e9dabb; border-radius:8px; font-size:12px; margin-bottom:14px; display:flex; align-items:center; justify-content:space-between; gap:12px; }
.discovery-alert.error { background:var(--red-soft); color:var(--red); border-color:#e4c3bd; }
@media(max-width:1100px) { .builtin-browser { grid-template-columns:220px minmax(0,1fr); }.source-detail { padding:16px; }.builtin-skills article { flex-wrap:wrap; }.builtin-skills article .btn { margin-left:41px; font-size:10px; } }
</style>
