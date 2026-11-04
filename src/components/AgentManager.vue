<script setup lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import { discovery, discoverMachine, loadAll, store, toast, ui } from "../store";
import { revealPath, saveCustomAgent, setAgentManagement } from "../api/tauri";
import { useEsc } from "../composables/useEsc";
import type { MachineAgent } from "../types";
import { mcpInventory, mcpSaveBinding, mcpFormats, type McpTarget } from "../api/mcp";

const list = ref<"installed" | "uninstalled">("installed");
const keyword = ref("");
const busy = ref("");
const editing = ref(false);
const originalId = ref("");
const saving = ref(false);
const form = reactive({ id:"", name:"", skills_dir:"", executable_path:"", enabled:true, mcp_path:"", mcp_format:"json" });
const mcpTargets = ref<McpTarget[]>([]);
const bindingLoading = ref(false);
useEsc(() => { if (!saving.value) editing.value = false; });

const candidates = computed(() => (discovery.inventory?.agents || []).filter(a => !a.shared));
const installed = computed(() => candidates.value.filter(a => a.installed));
const uninstalled = computed(() => candidates.value.filter(a => !a.installed));
const configured = computed(() => installed.value.filter(a => a.management_enabled).length);
const visible = computed(() => (list.value === "installed" ? installed.value : uninstalled.value).filter(a =>
  `${a.name} ${a.id} ${a.skills_dir}`.toLowerCase().includes(keyword.value.trim().toLowerCase())));

onMounted(() => {
  if (!discovery.inventory || Date.now()/1000 - discovery.inventory.scanned_at > 15) void discoverMachine();
  void mcpInventory().then(data => mcpTargets.value = data.targets).catch(e => toast(String(e), "error"));
});

async function toggle(agent: MachineAgent, value: boolean) {
  if (busy.value || discovery.loading) return;
  busy.value = agent.id;
  try {
    store.config = await setAgentManagement(agent.id, value);
    const updated = discovery.inventory?.agents.find(a => a.id === agent.id);
    if (updated) { updated.management_enabled = value; updated.registered ||= value; }
    await loadAll();
    toast(value ? `已允许配置 ${agent.name}` : `已关闭 ${agent.name} 的配置，原技能分配与链接保留`, "success");
  } catch (e) { toast(String(e),"error"); }
  finally { busy.value = ""; }
}

async function openEditor(agent?: MachineAgent) {
  if (bindingLoading.value) return;
  bindingLoading.value = true;
  try { mcpTargets.value = (await mcpInventory()).targets; }
  catch(e) { toast(`无法读取 MCP 配置位置：${e}`, "error"); return; }
  finally { bindingLoading.value = false; }
  originalId.value = agent?.id || "";
  Object.assign(form, { id:agent?.id || "", name:agent?.name || "", skills_dir:agent?.skills_dir || "",
    executable_path:agent?.executable_path || store.config?.agent_preferences?.[agent?.id || ""]?.executable_path || "",
    enabled:agent?.management_enabled ?? true,
    mcp_path:mcpTargets.value.find(t => t.agent_id === agent?.id)?.binding?.path || "",
    mcp_format:mcpTargets.value.find(t => t.agent_id === agent?.id)?.binding?.format || "json" });
  if (agent && !agent.installed) form.enabled = true;
  editing.value = true;
}

async function save() {
  if (saving.value || discovery.loading) return;
  const id = form.id.trim();
  if (!id || !form.name.trim() || !form.skills_dir.trim()) { toast("请填写名称、唯一 ID 和技能目录", "error"); return; }
  if (!originalId.value && (discovery.inventory?.agents.some(a => a.id === id) || store.config?.agents.some(a => a.id === id))) {
    toast("此 ID 已存在，请使用对应工具的「编辑路径」入口", "error"); return;
  }
  saving.value = true;
  try {
    const previousBinding = mcpTargets.value.find(t => t.agent_id === id)?.binding;
    const bindingChanged = !!form.mcp_path.trim() && (previousBinding?.path !== form.mcp_path.trim() || previousBinding?.format !== form.mcp_format);
    if (bindingChanged && !form.enabled) throw new Error("更改 MCP 配置位置前，请勾选允许配置此工具");
    store.config = await saveCustomAgent({ id, name:form.name.trim(), skills_dir:form.skills_dir.trim(), builtin:false }, form.executable_path.trim(), form.enabled);
    originalId.value = id;
    await discoverMachine();
    if (bindingChanged) {
      try { await mcpSaveBinding({agent_id:id, path:form.mcp_path.trim(), format:form.mcp_format}); }
      catch(e) { throw new Error(`Agent 与 Skills 路径已保存，MCP 位置未保存：${e}`); }
    }
    mcpTargets.value = (await mcpInventory()).targets;
    editing.value = false;
    list.value = "installed";
    toast("自定义路径已保存", "success");
  } catch(e) { toast(String(e),"error"); }
  finally { saving.value = false; }
}

function openPath(path: string) { revealPath(path).catch(e => toast(String(e),"error")); }
function configure() { ui.page = "agents"; }
function evidenceLabel(agent: MachineAgent) {
  if (agent.custom) return "手动确认";
  return agent.evidence.filter(e => ["PATH 命令","安装登记","运行进程","IDE 扩展","开始菜单","常见安装位置"].includes(e.kind))
    .map(e => e.kind).filter((v,i,a) => a.indexOf(v) === i).join(" · ");
}
</script>

<template>
  <section class="agent-manager">
    <div class="manager-heading">
      <div><h2>AI Agent 工具</h2><p>先确认本机已安装的工具，再选择允许本软件配置哪些 Agent。</p></div>
      <div class="manager-actions"><button class="btn" :disabled="discovery.loading || !!busy || saving" @click="discoverMachine()">{{ discovery.loading ? '扫描中…' : '↻ 重新扫描' }}</button><button class="btn primary" :disabled="discovery.loading || !!busy" @click="openEditor()">＋ 自定义工具</button></div>
    </div>

    <div class="manager-summary"><span><b>{{ installed.length }}</b> 个已安装</span><span><b>{{ configured }}</b> 个允许配置</span><span class="catalog-note">内置 {{ discovery.inventory?.catalog_count || '—' }} 条产品路径规则 · 扫描不会自动开启配置</span></div>
    <div v-if="discovery.error" class="manager-error" role="alert">{{ discovery.error }} <button class="btn" @click="discoverMachine()">重试</button></div>
    <p v-for="warning in discovery.inventory?.warnings" :key="warning" class="manager-warning">{{ warning }}</p>

    <div class="manager-toolbar">
      <div class="manager-tabs" role="tablist" aria-label="安装状态">
        <button role="tab" :aria-selected="list === 'installed'" :class="{active:list === 'installed'}" @click="list = 'installed'">已安装 <span>{{ installed.length }}</span></button>
        <button role="tab" :aria-selected="list === 'uninstalled'" :class="{active:list === 'uninstalled'}" @click="list = 'uninstalled'">未安装 / 待确认 <span>{{ uninstalled.length }}</span></button>
      </div>
      <input v-model="keyword" class="input manager-search" placeholder="搜索工具名称或路径…" aria-label="搜索 Agent 工具" />
    </div>
    <p class="manager-note">{{ list === 'installed' ? '关闭「允许配置」后，该工具会从总览、Agent 配置和技能分配中隐藏；原目录、链接与分配记录保留。' : '目录或缓存残留不视为已安装。安装后重新扫描即可识别；未知或便携工具可以编辑并确认自定义路径。' }}</p>

    <div class="agent-settings-list">
      <article v-for="agent in visible" :key="agent.id" class="agent-settings-row" :data-agent-id="agent.id">
        <div class="tool-avatar">{{ agent.name.replace('OpenAI ','').replace('腾讯 ','').slice(0,2).toUpperCase() }}</div>
        <div class="tool-content">
          <div class="tool-title"><h3>{{ agent.name }}</h3><span class="badge" :class="agent.installed ? agent.custom ? 'gray' : 'green' : 'gray'">{{ agent.installed ? agent.custom ? '手动确认' : '已安装' : agent.confidence === 'missing' ? '未安装' : '待确认' }}</span></div>
          <div class="tool-path"><span>Skills</span><code :title="agent.skills_dir">{{ agent.skills_dir }}</code><button v-if="agent.dir_exists" class="path-link" title="打开技能目录" @click="openPath(agent.skills_dir)">↗</button></div>
          <div class="tool-path"><span>MCP</span><code>{{ mcpTargets.find(t => t.agent_id === agent.id)?.binding?.path || '尚未绑定，可在编辑路径中指定' }}</code></div>
          <div v-for="link in agent.path_links.filter(item => item.scope === 'directory')" :key="link.path" class="tool-link-map"><span>路径联接</span><code :title="link.path">{{ link.path }}</code><span>→ 实际目录</span><code :title="link.target">{{ link.target }}</code></div>
          <details v-if="agent.path_links.some(item => item.scope === 'skill')" class="tool-skill-links"><summary>{{ agent.path_links.filter(item => item.scope === 'skill').length }} 个技能目录链接 · 查看实际目标</summary><div v-for="link in agent.path_links.filter(item => item.scope === 'skill')" :key="link.path" class="tool-link-map"><code :title="link.path">{{ link.path }}</code><span>→</span><code :title="link.target">{{ link.target }}</code></div></details>
          <p v-if="agent.installed" class="tool-evidence" :title="agent.executable_path">{{ evidenceLabel(agent) }}<span v-if="!agent.dir_exists"> · 技能目录将在部署时创建</span></p>
          <p v-else class="tool-evidence">{{ agent.confidence === 'missing' ? '没有检测到程序安装证据' : '配置目录可能是历史残留，尚未确认程序已安装' }}</p>
        </div>
        <div class="tool-actions">
          <button class="tool-edit" :disabled="discovery.loading || !!busy" @click="openEditor(agent)">{{ agent.installed ? '编辑路径' : '自定义确认' }}</button>
          <div v-if="agent.installed" class="management-control">
            <span>{{ agent.management_enabled ? '允许配置' : '未开启配置' }}</span>
            <button class="management-switch" role="switch" :aria-checked="agent.management_enabled" :aria-label="`允许配置 ${agent.name}`" :class="{on:agent.management_enabled}" :disabled="!!busy || discovery.loading" @click="toggle(agent,!agent.management_enabled)"><span></span></button>
          </div>
          <span v-else class="unavailable-label">不可配置</span>
        </div>
      </article>
      <div v-if="!visible.length" class="empty"><div class="icon">⌕</div>{{ discovery.loading ? '正在查找本机安装的工具…' : keyword ? '没有匹配的工具。' : list === 'installed' ? '尚未确认已安装的 Agent，可以重新扫描或自定义添加。' : '没有其他待确认的工具。' }}</div>
    </div>
    <div v-if="configured" class="manager-footer"><span>开关即时保存，无需额外点击保存。</span><button class="btn" @click="configure">配置已开启的 Agent →</button></div>

    <div v-if="editing" class="modal-mask" @click.self="!saving && (editing = false)">
      <form class="modal custom-agent-modal" @submit.prevent="save">
        <div class="modal-head"><b>{{ originalId ? '编辑并确认工具路径' : '添加自定义工具' }}</b><button type="button" class="btn ghost" :disabled="saving" aria-label="关闭" @click="editing = false">✕</button></div>
        <div class="modal-body">
          <p class="custom-help">自定义用于新工具、便携安装或自动识别失败的情况。请确认本机已经安装该工具；提供有效的技能目录或程序文件作为依据。</p>
          <label>工具名称<input v-model="form.name" class="input" required placeholder="例如 My Agent" /></label>
          <label>唯一 ID<input v-model="form.id" class="input" required :readonly="!!originalId" pattern="[a-zA-Z0-9_-]+" placeholder="例如 my-agent" /></label>
          <label>Skills 目录<input v-model="form.skills_dir" class="input" required placeholder="技能目录的绝对路径" /></label>
          <label>MCP 配置文件 <span class="optional">可选，填写完整文件路径</span><input v-model="form.mcp_path" class="input" placeholder="例如 D:/MyAgent/mcp.json" /></label>
          <label v-if="form.mcp_path">MCP 配置格式<select v-model="form.mcp_format" class="input"><option v-for="format in mcpFormats" :key="format.id" :value="format.id">{{ format.name }}</option></select></label>
          <p class="custom-help">Skills 目录只管理标准技能。MCP 请选原工具实际使用的用户配置文件和对应格式；未知格式不能保证写入后被工具识别。清空 MCP 路径保留既有绑定。</p>
          <label>程序文件路径 <span class="optional">技能目录不存在时必填</span><input v-model="form.executable_path" class="input" placeholder="本机可执行文件的绝对路径" /></label>
          <label class="custom-enable"><input v-model="form.enabled" type="checkbox" />保存后允许本软件配置此工具</label>
        </div>
        <div class="modal-foot"><button type="button" class="btn" :disabled="saving" @click="editing = false">取消</button><button class="btn primary" type="submit" :disabled="saving || discovery.loading">{{ saving ? '保存中…' : '确认并保存' }}</button></div>
      </form>
    </div>
  </section>
</template>

<style scoped>
.manager-heading,.manager-toolbar,.manager-footer,.manager-summary,.tool-title,.tool-path,.tool-actions,.management-control,.manager-actions { display:flex; align-items:center; }
.manager-heading,.manager-toolbar,.manager-footer { justify-content:space-between; gap:18px; }
.manager-heading h2 { font-size:17px; font-weight:600; }.manager-heading p { margin-top:5px; color:var(--muted); font-size:12px; }
.manager-actions { gap:9px; flex-shrink:0; }.manager-summary { gap:24px; padding:18px 0 22px; color:var(--muted); font-size:12px; }.manager-summary b { color:var(--text); font-weight:600; margin-right:4px; }
.catalog-note { margin-left:auto; color:var(--faint); font-size:11px; }.manager-tabs { display:flex; gap:4px; background:var(--panel-2); padding:4px; border-radius:7px; }
.manager-tabs button { border:1px solid transparent; border-radius:5px; background:none; color:var(--muted); font-size:12px; padding:7px 11px; cursor:pointer; white-space:nowrap; }.manager-tabs button.active { background:var(--panel); color:var(--text); border-color:var(--border); box-shadow:var(--shadow-sm); }.manager-tabs span { margin-left:5px; font-size:10px; color:var(--faint); }
.manager-search { width:260px; }.manager-note { margin:14px 0 18px; color:var(--muted); font-size:11px; line-height:1.8; }
.agent-settings-list { border:1px solid var(--border); border-radius:10px; overflow:hidden; }.agent-settings-row { display:flex; gap:14px; align-items:center; padding:20px; border-bottom:1px solid var(--border-soft); }.agent-settings-row:last-child { border-bottom:0; }.agent-settings-row:hover { background:var(--bg-soft); }
.tool-avatar { width:38px; height:38px; border:1px solid var(--border); border-radius:9px; background:var(--panel); display:grid; place-items:center; font:500 12px "Bahnschrift",sans-serif; flex-shrink:0; }.tool-content { flex:1; min-width:0; }.tool-title { gap:10px; margin-bottom:7px; }.tool-title h3 { font-size:13px; font-weight:600; }.tool-title .badge { font-size:10px; }
.tool-path { gap:8px; min-width:0; }.tool-path > span { font-size:10px; color:var(--faint); }.tool-path code { font-size:11px; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; color:var(--muted); }.path-link { border:0; background:none; color:var(--accent); cursor:pointer; font-size:14px; flex-shrink:0; }
.tool-link-map { display:flex; align-items:baseline; flex-wrap:wrap; gap:4px 8px; margin-top:7px; font-size:10px; color:var(--muted); }.tool-link-map code { font-size:10px; color:var(--text); overflow-wrap:anywhere; }.tool-skill-links { margin-top:8px; color:var(--muted); font-size:10px; }.tool-skill-links summary { cursor:pointer; width:max-content; max-width:100%; color:var(--accent); }.tool-skill-links .tool-link-map { padding:4px 0 4px 10px; border-left:1px solid var(--border); }
.tool-evidence { margin-top:5px; color:var(--faint); font-size:10px; }.tool-actions { gap:24px; flex-shrink:0; }.tool-edit { font-size:11px; border:0; background:none; cursor:pointer; color:var(--muted); }.tool-edit:hover { color:var(--accent); text-decoration:underline; }.management-control { min-width:121px; justify-content:flex-end; gap:10px; font-size:11px; color:var(--muted); }.management-switch { width:34px; height:20px; padding:2px; border:1px solid #d4d4d8; border-radius:12px; background:#d4d4d8; cursor:pointer; flex-shrink:0; }.management-switch span { display:block; width:14px; height:14px; background:#fff; border-radius:50%; box-shadow:0 1px 2px #0002; transition:transform .15s; }.management-switch.on { background:var(--accent); border-color:var(--accent); }.management-switch.on span { transform:translateX(14px); }.unavailable-label { font-size:11px; color:var(--faint); min-width:121px; text-align:right; }
.manager-footer { margin-top:18px; color:var(--faint); font-size:11px; }.manager-error { color:var(--red); background:var(--red-soft); padding:12px; margin-bottom:12px; border-radius:7px; display:flex; justify-content:space-between; }.manager-warning { color:var(--amber); background:var(--amber-soft); border-radius:6px; padding:10px; font-size:12px; margin-bottom:10px; }
.custom-agent-modal { max-width:560px; }.custom-help { color:var(--muted); font-size:12px; margin-bottom:18px; line-height:1.8; }.custom-agent-modal .modal-body > label { display:block; font-size:12px; margin-top:14px; }.custom-agent-modal .input { margin-top:6px; }.custom-agent-modal .custom-enable { display:flex !important; align-items:center; gap:8px; }.optional { margin-left:8px; color:var(--faint); font-size:10px; }
@media(max-width:1150px) { .catalog-note { display:none; }.manager-heading { align-items:flex-start; }.manager-actions { flex-wrap:wrap; justify-content:flex-end; }.tool-actions { flex-direction:column-reverse; align-items:flex-end; gap:10px; }.management-control,.unavailable-label { min-width:auto; }.manager-search { width:220px; } }
</style>
