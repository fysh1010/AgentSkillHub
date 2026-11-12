<script setup lang="ts">
import {computed,onMounted,ref,watch} from 'vue';
import * as api from '../api/mcp';
import {refreshDiscovery} from '../api/tauri';
import type {McpInventory,McpServer,McpSource,McpTarget,McpPreview} from '../api/mcp';
import {toast,openAgentSettings,store} from '../store';
import {useEsc} from '../composables/useEsc';
import McpMarket from '../components/McpMarket.vue';
import McpEditor from '../components/McpEditor.vue';
import McpCheckStatus from '../components/McpCheckStatus.vue';
import {useMcpChecks} from '../composables/useMcpChecks';
import AgentAssignmentList from '../components/AgentAssignmentList.vue';
import AgentIcon from '../components/AgentIcon.vue';
import {assignmentAgents} from '../composables/useSkillAssignments';

interface Instance {source:McpSource;server:McpServer}
interface Entry {id:string;server:McpServer;library:boolean;instances:Instance[]}
interface Pending {agent:string;preview:McpPreview;done?:boolean;error?:string;probe?:McpServer}
const {check}=useMcpChecks();
const editorAgents=ref<string[]>([]);
const data=ref<McpInventory>({servers:[],sources:[],targets:[],warnings:[],guides:[]});
const tab=ref('mine');const busy=ref(false);const error=ref('');const search=ref('');const filter=ref('all');
const agentId=ref('');
const editorOpen=ref(false);const editServer=ref<McpServer>();const editInstance=ref<Instance>();const editorError=ref('');
const editorExisting=ref(false);const suggestedAgent=ref('');
const assignEntry=ref<Entry>();const selected=ref<string[]>([]);const assignmentError=ref('');
const quickAssign=ref(false);
const pending=ref<Pending[]>([]);const previewTitle=ref('');const removeEntry=ref<Entry>();const restoreTarget=ref<McpTarget>();
const enabledIds=computed(()=>new Set(assignmentAgents.value.map(a=>a.id)));
const query=(s:string)=>s.toLowerCase().includes(search.value.trim().toLowerCase());
const hosted=(s:McpServer)=>!!s.config._hub_hosted;
const paused=(i:Instance)=>i.source.kind==='paused';
const disabled=(s:McpServer)=>s.config.disabled===true||s.config.enabled===false;
const transport=(s:McpServer)=>hosted(s)?'托管':s.config.command?'STDIO':s.config.type==='sse'?'SSE':'HTTP';
const canonical=(config:Record<string,unknown>)=>{const c={...config};if(c.command&&(!c.type||c.type==='stdio'||c.type==='local')||c.url&&(!c.type||c.type==='http'||c.type==='streamable-http'))delete c.type;delete c.enabled;delete c.disabled;for(const key of ['env','headers'])if(c[key]&&Object.keys(c[key] as object).length===0)delete c[key];if(Array.isArray(c.args)&&!c.args.length)delete c.args;return JSON.stringify(c,(_k,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);};
const same=(a:McpServer,b:McpServer)=>a.name===b.name&&canonical(a.config)===canonical(b.config);
const summary=(s:McpServer)=>hosted(s)?'由原应用管理授权':s.config.command?'本地进程 · '+String(s.config.command).split(/[\\/]/).pop():'远程 MCP 服务';
const userSources=computed(()=>data.value.sources.filter(s=>enabledIds.value.has(s.agent_id)&&(s.kind==='user'||s.kind==='paused')));
const sourceErrors=computed(()=>userSources.value.filter(s=>s.error));
const entries=computed(()=>{
 const out:Entry[]=data.value.servers.map(s=>({id:'library:'+s.name,server:s,library:true,instances:[]}));
 for(const source of userSources.value)for(const server of source.servers){
  let row=out.find(e=>same(e.server,server));if(!row){row={id:source.id+':'+server.name,server,library:false,instances:[]};out.push(row);}
  if(!row.instances.some(i=>i.source.agent_id===source.agent_id&&i.source.path===source.path&&i.source.kind===source.kind))row.instances.push({source,server});
 }
 return out.sort((a,b)=>a.server.name.localeCompare(b.server.name));
});
const visibleEntries=computed(()=>entries.value.filter(e=>query(e.server.name+' '+e.instances.map(i=>i.source.agent_name).join(' '))&&(filter.value==='all'||filter.value==='active'&&e.instances.some(i=>!paused(i)&&!disabled(i.server))||filter.value==='saved'&&!e.instances.length||filter.value==='paused'&&e.instances.some(paused))));
const activeCount=computed(()=>entries.value.filter(e=>e.instances.some(i=>!paused(i)&&!disabled(i.server))).length);
const userCount=(id:string)=>userSources.value.filter(s=>s.agent_id===id&&s.kind==='user').reduce((n,s)=>n+s.servers.length,0);
const target=computed(()=>targets.value.find(t=>t.agent_id===agentId.value));
const targets=computed(()=>data.value.targets.filter(t=>enabledIds.value.has(t.agent_id)));
const toolInstances=computed(()=>userSources.value.filter(s=>s.agent_id===agentId.value).flatMap(source=>source.servers.map(server=>({source,server}))));
const assignTargets=computed(()=>targets.value);
const actionNames:Record<string,string>={add:'新增配置',update:'更新配置',remove:'移出配置',keep:'保留配置',pause:'暂停服务',resume:'重新启用',activate:'启用服务',edit:'编辑配置'};
const statusText=(t:McpTarget)=>!t.installed?'尚未确认安装':!t.binding?'在设置中查看配置位置':!t.writable?'未开启配置管理':userCount(t.agent_id)+' 个用户服务';
function instanceFor(e:Entry,t:McpTarget){
 const matching=e.instances.find(i=>i.source.agent_id===t.agent_id&&i.source.kind==='user')||e.instances.find(i=>i.source.agent_id===t.agent_id);
 if(matching)return matching;
 // 库定义更新后，已托管 Agent 中可能仍是旧版本。读取真实实例，不能只看历史归属。
 if(e.library&&t.managed.includes(e.server.name)){
  for(const source of [...userSources.value].sort((a,b)=>Number(a.kind==='paused')-Number(b.kind==='paused'))){if(source.agent_id!==t.agent_id)continue;const server=source.servers.find(s=>s.name===e.server.name);if(server)return {source,server};}
 }
}
function enabledOn(e:Entry,t:McpTarget){const i=instanceFor(e,t);return !!i&&!paused(i)&&!disabled(i.server);}
function canAssign(t:McpTarget){return t.writable&&!!t.binding&&!t.app_managed;}
function conflicting(e:Entry,t:McpTarget){return userSources.value.some(s=>s.agent_id===t.agent_id&&s.servers.some(server=>server.name===e.server.name&&!same(server,e.server)));}
function assignmentBlocked(e:Entry,t:McpTarget){
 if(hosted(e.server))return '此服务的授权由原应用管理';
 if(!canAssign(t))return t.blocked_reason||(!t.installed?'尚未确认安装':!t.binding?'请先绑定适配的配置文件':'请在设置中开启「允许配置」');
 if(disabled(e.server)&&!instanceFor(e,t))return '源定义已禁用，请先启用源 Agent 或编辑服务定义';
 if(conflicting(e,t)&&!(e.library&&t.managed.includes(e.server.name)))return '存在同名的不同配置，请先在按 Agent 视图确认';
 const saved=data.value.servers.find(s=>s.name===e.server.name);
 if(saved&&!same(saved,e.server))return '服务库存在同名的不同配置，请另存为新名称';
 return '';
}
function localTargets(_e:Entry){return targets.value;}
function assignmentIcons(e:Entry){return localTargets(e).map(t=>{
 const blocked=assignmentBlocked(e,t),on=enabledOn(e,t),conflict=conflicting(e,t);
 return {id:t.agent_id,name:t.name,on,external:conflict,disabled:!!blocked,title:t.name+' · '+(blocked||(conflict?(on?'已有不同版本，点击暂停；同步可使用分配与启用':'已有不同版本，点击预览启用'):on?'已启用，点击暂停':'未启用，点击分配与启用'))};
});}
async function quickAssignments(e:Entry,mode:'all'|'none'|'invert',agent?:string){
 if(busy.value||pending.value.length||editorOpen.value)return;
 const eligible=localTargets(e).filter(t=>(!agent||t.agent_id===agent)&&!assignmentBlocked(e,t));
 if(!eligible.length)return;
 openAssign(e);quickAssign.value=true;
 for(const t of eligible){const on=enabledOn(e,t),next=mode==='invert'?!on:mode==='all';selected.value=selected.value.filter(id=>id!==t.agent_id);if(next)selected.value.push(t.agent_id);}
 try{await prepareAssignments(false,false,eligible.map(t=>t.agent_id));if(assignmentError.value){toast(assignmentError.value,'info');assignEntry.value=undefined;}}
 finally{quickAssign.value=false;}
}
function selectAssignmentAgents(mode:'all'|'none'|'invert'){
 const e=assignEntry.value;if(!e)return;
 for(const t of assignTargets.value.filter(t=>!assignmentBlocked(e,t))){const on=selected.value.includes(t.agent_id);selected.value=selected.value.filter(id=>id!==t.agent_id);if(mode==='all'||mode==='invert'&&!on)selected.value.push(t.agent_id);}
}
async function refresh(rescan=false){
 busy.value=true;error.value='';
 try{if(rescan)await refreshDiscovery();data.value=await api.mcpInventory();if(!targets.value.some(t=>t.agent_id===agentId.value))agentId.value=targets.value[0]?.agent_id||'';}
 catch(e){error.value=String(e);}finally{busy.value=false;}
}
function openEditor(server?:McpServer,instance?:Instance,existing=false){editServer.value=server;editInstance.value=instance;editorExisting.value=existing;suggestedAgent.value=tab.value==='agents'&&target.value?.writable?agentId.value:'';const e=server?entries.value.find(e=>e.library&&e.server.name===server.name):undefined;editorAgents.value=instance?[instance.source.agent_id]:e?data.value.targets.filter(t=>enabledOn(e,t)).map(t=>t.agent_id):suggestedAgent.value?[suggestedAgent.value]:[];editorError.value='';editorOpen.value=true;}
function addFromMarket(server:McpServer){openEditor(server);}
function autoDetect(server:McpServer){if(!disabled(server)&&!hosted(server))void check(server,false);}
function checkAll(){for(const e of entries.value)if(!disabled(e.server)&&(!e.instances.length||e.instances.some(i=>!paused(i))))autoDetect(e.server);}
function removeFromEditor(){const i=editInstance.value;editorOpen.value=false;if(i)void directChange(i,'remove');else removeEntry.value=entries.value.find(e=>e.library&&e.server.name===editServer.value?.name);}
async function saveEditor(servers:McpServer[],assign:boolean,agentIds:string[],autoCheck:boolean){
 busy.value=true;editorError.value='';
 try{
  if(editInstance.value){const i=editInstance.value;const preview=await api.mcpPreviewChange(i.source,i.server,'edit',servers[0].config);editorOpen.value=false;pending.value=[{agent:i.source.agent_name,preview,probe:autoCheck&&!paused(i)?servers[0]:undefined}];previewTitle.value='确认修改服务配置';}
  else{
   for(const server of servers){
    const exists=data.value.servers.find(s=>s.name===server.name);
    if(exists&&!editorExisting.value)throw new Error('服务库已有同名 MCP：'+server.name+'，请修改名称后保存');
   }
   for(const server of servers)await api.mcpSaveServer(server);
   editorOpen.value=false;await refresh();toast('已保存 '+servers.length+' 个 MCP 服务','success');
   if(autoCheck)servers.forEach(autoDetect);
   if(assign&&servers.length===1){const e=entries.value.find(e=>e.library&&e.server.name===servers[0].name);if(e){openAssign(e);selected.value=[...agentIds];await prepareAssignments(true,autoCheck);}}
   else if(servers.length>1)tab.value='mine';
  }
 }catch(e){editorError.value=String(e);}finally{busy.value=false;}
}
function openAssign(e:Entry){assignEntry.value=e;selected.value=data.value.targets.filter(t=>enabledOn(e,t)).map(t=>t.agent_id);assignmentError.value='';}
function needsSync(e:Entry,t:McpTarget){return e.library&&t.managed.includes(e.server.name)&&userSources.value.some(s=>s.agent_id===t.agent_id&&s.kind==='user'&&s.servers.some(s=>s.name===e.server.name&&!same(s,e.server)));}
async function prepareAssignments(fromEditor=false,autoCheck=true,scope?:string[]){
 const e=assignEntry.value;if(!e)return;busy.value=true;assignmentError.value='';
 const plans:Pending[]=[];
 try{
  let saved=data.value.servers.find(s=>s.name===e.server.name);
  if(saved&&!same(saved,e.server))throw new Error('服务库中已有同名但不同配置的服务。请先另存为新名称，再分配。');
  const used=new Set<string>();
  for(const t of assignTargets.value){
   if(scope&&!scope.includes(t.agent_id))continue;
   const next=selected.value.includes(t.agent_id),current=enabledOn(e,t),i=instanceFor(e,t);
   if(next===current&&!(next&&needsSync(e,t)))continue;
   if(!canAssign(t))throw new Error(t.name+'：'+(t.blocked_reason||'请先绑定已适配的配置文件'));
   const path=i?.source.path||t.binding!.path;if(used.has(path))throw new Error('多个 Agent 共用 '+path+'，请分别应用以免重复修改');
   used.add(path);
   let preview:McpPreview;
   if(i&&(!next||paused(i)||disabled(i.server)))preview=await api.mcpPreviewChange(i.source,i.server,next?(disabled(i.server)&&!paused(i)?'activate':'resume'):'pause');
   else{
    if(!saved){await api.mcpSaveServer(e.server);saved=e.server;}
    const names=t.managed.filter(n=>n!==e.server.name);if(next)names.push(e.server.name);
    preview=await api.mcpPreview(t.agent_id,names);
   }
   plans.push({agent:t.name,preview,probe:autoCheck&&next?e.server:undefined});
  }
  if(!plans.length){if(fromEditor){assignEntry.value=undefined;tab.value='mine';}else assignmentError.value='尚未改变分配。已有服务需要更新时，请在按 Agent 视图中编辑配置。';return;}
  assignEntry.value=undefined;pending.value=plans;previewTitle.value='确认分配与启用';
 }catch(err){assignmentError.value=String(err);}finally{busy.value=false;}
}
async function directChange(i:Instance,operation:string){
 busy.value=true;
 try{const preview=await api.mcpPreviewChange(i.source,i.server,operation);pending.value=[{agent:i.source.agent_name,preview,probe:['resume','activate'].includes(operation)?{...i.server,config:{...i.server.config,disabled:false,enabled:true}}:undefined}];previewTitle.value=operation==='pause'?'暂停此 Agent 中的服务':operation==='resume'?'重新启用服务':operation==='activate'?'启用此 Agent 中的服务':'确认移除服务';}
 catch(e){toast(String(e),'error');}finally{busy.value=false;}
}
async function applyPlans(){
 busy.value=true;
 try{
  for(const p of pending.value){if(p.done)continue;try{await api.mcpApply(p.preview.ticket);p.done=true;p.error='';if(p.probe)autoDetect(p.probe);}catch(e){p.error=String(e);}}
  const failed=pending.value.filter(p=>!p.done);
  if(!failed.length){const detecting=pending.value.some(p=>p.probe);pending.value=[];toast(detecting?'配置已应用，已安排连接检测；目标 Agent 可能需要重新加载或授权':'配置已应用；目标 Agent 可能需要重新加载','success');}
  await refresh();
 }finally{busy.value=false;}
}
async function copyDefinition(i:Instance){const existing=data.value.servers.some(s=>s.name===i.server.name);openEditor({...i.server,name:existing?i.server.name+'-copy':i.server.name});}
async function removeLibrary(){if(!removeEntry.value)return;busy.value=true;try{await api.mcpRemoveServer(removeEntry.value.server.name);removeEntry.value=undefined;await refresh();toast('已从服务库移除','success');}catch(e){toast(String(e),'error');}finally{busy.value=false;}}
async function restore(){if(!restoreTarget.value)return;busy.value=true;try{toast(await api.mcpRestore(restoreTarget.value.agent_id),'success');restoreTarget.value=undefined;await refresh();}catch(e){toast(String(e),'error');}finally{busy.value=false;}}
useEsc(()=>{if(busy.value)return;editorOpen.value=false;assignEntry.value=undefined;pending.value=[];removeEntry.value=undefined;restoreTarget.value=undefined;});
onMounted(()=>refresh());
// 外部编辑后更新清单；编辑、差异确认期间保留用户正在审阅的状态。
watch(()=>store.revision,()=>{if(!busy.value&&!editorOpen.value&&!assignEntry.value&&!pending.value.length&&!removeEntry.value&&!restoreTarget.value)void refresh();});
</script>

<template>
<section class="mcp-workspace">
 <header class="workspace-heading">
  <div><div class="eyebrow">TOOLS, CONNECTED.</div><h1>MCP 集成</h1><p>自己的服务，一个地方管理。分配给需要它的 Agent。</p></div>
  <div class="heading-actions"><button class="btn scan-btn" :disabled="busy" @click="refresh(true)"><span :class="{spinning:busy}">↻</span> {{busy?'正在处理':'重新扫描'}}</button><button class="btn" :disabled="busy" @click="checkAll()">检测全部</button><button class="btn primary" :disabled="busy" @click="openEditor()">＋ 添加 MCP</button></div>
 </header>
 <div class="workspace-summary"><span><b>{{entries.length}}</b> 我的服务</span><i/><span><b>{{activeCount}}</b> 已写入 Agent</span><i/><span><b>{{targets.length}}</b> 已开启 Agent</span><span class="summary-note"><em/> 配置在本机保存</span></div>
 <nav class="workspace-tabs" aria-label="MCP 视图"><button v-for="item in [{id:'mine',name:'我的 MCP'},{id:'agents',name:'按 Agent'},{id:'market',name:'MCP 市场'}]" :key="item.id" :class="{active:tab===item.id}" :aria-label="item.name" @click="tab=item.id;search=''">{{item.name}}<span v-if="item.id==='mine'">{{entries.length}}</span></button></nav>
 <div v-if="error" class="workspace-error" role="alert">{{error}} <button @click="refresh()">重试</button></div>

 <template v-if="tab==='mine'">
  <details v-if="sourceErrors.length" class="info-card"><summary>{{sourceErrors.length}} 个用户配置读取失败 · 查看详情</summary><p v-for="s in sourceErrors" :key="s.id">{{s.agent_name}} · {{s.path}}<br/>{{s.error}}</p><button class="text-action" @click="openAgentSettings()">工具与路径设置 →</button></details>
  <div class="list-toolbar"><div class="filter-chips"><button v-for="item in [{id:'all',name:'全部服务'},{id:'active',name:'已启用'},{id:'saved',name:'未分配'},{id:'paused',name:'已暂停'}]" :key="item.id" :class="{active:filter===item.id}" @click="filter=item.id">{{item.name}}</button></div><div class="search-field"><span>⌕</span><input v-model="search" placeholder="搜索服务或 Agent…" aria-label="搜索我的 MCP"/></div></div>
  <div v-if="entries.length" class="services-table">
   <div class="table-labels"><span>服务 / 连接方式</span><span>分配与启用</span><span>管理</span></div>
   <article v-for="e in visibleEntries" :key="e.id" class="service-row">
    <div class="service-identity"><span class="service-symbol" :class="{remote:!e.server.config.command}"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6"><path v-if="e.server.config.command" d="m5 7 5 5-5 5m8 0h6"/><path v-else d="M8 12h8M12 8v8M5 6h14v12H5z"/></svg></span><div><h3>{{e.server.name}} <span class="transport-tag">{{transport(e.server)}}</span></h3><p>{{summary(e.server)}}<span class="origin-label">{{e.library?'统一管理':'来自用户配置'}}</span></p><McpCheckStatus :server="e.server" :disabled="disabled(e.server)||(!e.library&&e.instances.every(i=>paused(i)))"/></div></div>
    <AgentAssignmentList class="mcp-assignments" :agents="assignmentIcons(e)" :busy="busy" :label="'分配 MCP ' + e.server.name" @toggle="id=>quickAssignments(e,'invert',id)" @bulk="mode=>quickAssignments(e,mode)" />
    <div class="service-actions"><button class="btn compact" :disabled="busy" @click="openAssign(e)">分配与启用</button><button class="text-action" :disabled="busy" @click="e.library?openEditor(e.server,undefined,true):openEditor(e.server,e.instances[0])">编辑</button><button v-if="e.library" class="text-action danger" :disabled="busy" @click="removeEntry=e">移除</button></div>
   </article>
   <div v-if="!visibleEntries.length" class="empty-state small"><h3>没有符合条件的服务</h3><p>试试其他关键词或筛选条件。</p></div>
  </div>
  <div v-else-if="!busy" class="empty-state"><div class="empty-plug">⌘</div><h2>把常用工具连接起来</h2><p>已开启 Agent 的用户服务会自动显示在这里，也可以添加自己的 MCP。</p><div class="heading-actions"><button class="btn primary" @click="openEditor()">＋ 添加第一个 MCP</button><button class="btn" @click="openAgentSettings()">管理 Agent 工具</button></div><div class="empty-examples"><span>本地文件</span><span>代码仓库</span><span>知识库</span><span>内部服务</span></div></div>
  <div class="subtle-note"><span>ⓘ</span> 图标亮起表示配置已启用；点击图标或批量选择后预览确认。琥珀色表示同名配置不同。目标 Agent 可能需要重载或授权。</div>
 </template>

 <div v-if="tab==='agents'" class="agent-workspace">
  <aside class="agent-sidebar"><div class="sidebar-label">已开启的 AGENT</div><button v-for="t in targets" :key="t.agent_id" :class="{active:agentId===t.agent_id}" @click="agentId=t.agent_id"><span class="agent-avatar"><AgentIcon :id="t.agent_id" :name="t.name" :size="20" /></span><div><b>{{t.name}}</b><small>{{statusText(t)}}</small></div><span class="agent-count">{{userCount(t.agent_id)}}</span></button></aside>
  <div v-if="target" class="agent-panel">
   <header><div><span class="eyebrow">AGENT INTEGRATION</span><h2>{{target.name}}</h2><p>管理这个 Agent 中你自己配置的服务。</p></div><button class="btn compact" :disabled="busy||!target.writable" @click="openEditor()">＋ 添加服务</button></header>
   <p class="info-card">{{target.reload_notice}}</p>
   <div class="agent-config-actions"><button class="text-action" @click="openAgentSettings()">工具与路径设置 ↗</button><button v-if="target.can_restore" class="text-action" :disabled="busy||!target.writable" @click="restoreTarget=target">恢复上次修改</button></div>
   <div v-for="s in sourceErrors.filter(s=>s.agent_id===agentId)" :key="s.id" class="workspace-error" role="alert">{{s.error}} · {{s.path}}</div>
   <div class="agent-service-heading"><h3>我的服务 <span>{{toolInstances.length}}</span></h3><span>开关立即保存配置；运行中连接需在原工具确认</span></div>
   <article v-for="i in toolInstances" :key="i.source.id+':'+i.server.name" class="agent-service">
    <div class="service-identity"><span class="service-symbol mini">⌘</span><div><h3>{{i.server.name}} <span class="transport-tag">{{transport(i.server)}}</span></h3><p>{{paused(i)?'已暂停 · 定义已保留':disabled(i.server)?'原配置已禁用':summary(i.server)}}</p><McpCheckStatus :server="i.server" :disabled="paused(i)||disabled(i.server)"/></div></div>
    <div class="agent-service-actions"><button class="text-action" :disabled="busy||!target.writable" @click="openEditor(i.server,i)">编辑</button><button class="text-action" :disabled="busy||hosted(i.server)" @click="copyDefinition(i)">另存到服务库</button><button class="switch" :class="{on:!paused(i)&&!disabled(i.server)}" role="switch" :aria-checked="!paused(i)&&!disabled(i.server)" :aria-label="(paused(i)||disabled(i.server)?'启用 ':'暂停 ')+i.server.name" :disabled="busy||!target.writable" @click="directChange(i,paused(i)?'resume':disabled(i.server)?'activate':'pause')"><i/></button><button class="text-action danger" :disabled="busy||!target.writable" @click="directChange(i,'remove')">移除</button></div>
   </article>
   <div v-if="!toolInstances.length" class="empty-state small"><h3>这个 Agent 还没有用户 MCP</h3><p>从服务库选择服务，或添加自己的配置。</p><button class="btn" @click="tab='mine'">前往我的 MCP →</button></div>
   <div class="subtle-note">配置已启用不等同于服务在线。授权、信任与运行状态请在 {{target.name}} 中确认。</div>
  </div><div v-else class="empty-state"><h3>选择一个 Agent</h3><p>在设置中开启 Agent 后，即可在这里分配 MCP。</p></div>
 </div>

 <McpMarket v-if="tab==='market'" @configure="addFromMarket"/>

 <McpEditor v-if="editorOpen" :key="(editInstance?.source.id||'library')+':'+(editServer?.name||'new')" :server="editServer" :direct="!!editInstance" :lock-name="editorExisting" :targets="targets" :initial-agents="editorAgents" :busy="busy" :error="editorError" @close="editorOpen=false" @remove="removeFromEditor" @save="saveEditor"/>
 <div v-if="assignEntry && !quickAssign" class="mcp-overlay" @click.self="!busy&&(assignEntry=undefined)"><section class="workspace-dialog assignment-dialog" role="dialog" aria-modal="true" aria-labelledby="assignment-title"><header><div><span class="eyebrow">ASSIGN TO AGENTS</span><h2 id="assignment-title">分配 {{assignEntry.server.name}}</h2><p>勾选要启用的 Agent。取消勾选会暂停已有服务，并保留定义。</p></div><button class="icon-close" :disabled="busy" aria-label="关闭分配" @click="assignEntry=undefined">×</button></header><div class="dialog-body"><div class="assignment-selection"><span>选择分配的 Agent</span><button class="text-action" :disabled="busy" @click="selectAssignmentAgents('all')">全选</button><button class="text-action" :disabled="busy" @click="selectAssignmentAgents('none')">全不选</button><button class="text-action" :disabled="busy" @click="selectAssignmentAgents('invert')">反选</button></div><label v-for="t in assignTargets" :key="t.agent_id" class="assignment-row"><span class="agent-avatar"><AgentIcon :id="t.agent_id" :name="t.name" :size="20" /></span><div><b>{{t.name}}</b><small>{{canAssign(t)?t.binding?.path:t.blocked_reason||'尚未适配，请在设置中查看配置位置'}}</small></div><input v-model="selected" type="checkbox" :value="t.agent_id" :disabled="busy||!!assignmentBlocked(assignEntry,t)"/></label><p v-if="!assignTargets.length" class="info-card">请先在设置的 Agent 工具中开启管理。</p><p v-if="assignmentError" class="workspace-error" role="alert">{{assignmentError}}</p></div><footer><span>{{selected.length}} 个 Agent 已选</span><button class="btn" :disabled="busy" @click="assignEntry=undefined">取消</button><button class="btn primary" :disabled="busy" @click="prepareAssignments()">预览变更 →</button></footer></section></div>
 <div v-if="pending.length" class="mcp-overlay" @click.self="!busy&&(pending=[])"><section class="workspace-dialog" role="dialog" aria-modal="true" aria-labelledby="preview-title"><header><div><span class="eyebrow">REVIEW CHANGES</span><h2 id="preview-title">{{previewTitle}}</h2><p>确认后写入对应配置。其他服务保持原样，修改前自动备份。</p></div><button class="icon-close" :disabled="busy" aria-label="关闭预览" @click="pending=[]">×</button></header><div class="dialog-body"><section v-for="p in pending" :key="p.preview.ticket" class="preview-agent"><h3>{{p.agent}} <span v-if="p.done" class="success-text">已应用</span></h3><code>{{p.preview.path}}</code><div v-for="a in p.preview.actions" :key="a.name" class="preview-action"><span :class="a.kind">{{actionNames[a.kind]||a.kind}}</span><b>{{a.name}}</b></div><p v-if="p.error" class="workspace-error">{{p.error}}</p></section><p v-if="pending.some(p=>p.error)" class="info-card">部分操作未应用。已成功的配置会保留，请关闭预览、重新扫描后处理其余服务。</p></div><footer><span>仅修改上方列出的配置</span><button class="btn" :disabled="busy" @click="pending=[]">{{pending.some(p=>p.error)?'关闭':'取消'}}</button><button class="btn primary" :disabled="busy||pending.some(p=>p.error)" @click="applyPlans()">确认应用变更</button></footer></section></div>
 <div v-if="removeEntry||restoreTarget" class="mcp-overlay"><section class="workspace-dialog compact-dialog" role="dialog" aria-modal="true"><header><div><h2>{{restoreTarget?'恢复上次修改':'移除服务库定义'}}</h2><p>{{restoreTarget?'恢复 '+restoreTarget.name+' 上次修改前的完整配置。文件有外部变化时会停止。':'从服务库移除 '+removeEntry?.server.name+'。已有分配需先暂停，原 Agent 的外部配置不会被删除。'}}</p></div></header><footer><button class="btn" :disabled="busy" @click="removeEntry=undefined;restoreTarget=undefined">取消</button><button class="btn primary" :disabled="busy" @click="restoreTarget?restore():removeLibrary()">确认</button></footer></section></div>
</section>
</template>

<style scoped>
.mcp-workspace{max-width:1500px;margin:0 auto;padding:0 0 35px}.workspace-heading{display:flex;align-items:center;justify-content:space-between;gap:24px;margin-bottom:22px}.eyebrow{font-size:9px;font-weight:600;letter-spacing:2.1px;color:var(--faint)}h1{font-size:29px;font-weight:650;letter-spacing:-.8px;margin:9px 0 8px;line-height:1.2}.workspace-heading p{font-size:12px;line-height:1.8;color:var(--muted);margin:0}.heading-actions{display:flex;align-items:center;gap:10px}.scan-btn>span{font-size:17px;display:inline-block;margin-right:5px}.spinning{animation:spin 1s linear infinite}@keyframes spin{to{transform:rotate(360deg)}}.workspace-summary{display:flex;gap:17px;align-items:center;font-size:11px;color:var(--muted);padding:15px 18px;background:var(--bg-soft);border:1px solid var(--border-soft);border-radius:9px;margin-bottom:25px}.workspace-summary b{font-family:Consolas,monospace;color:var(--text);font-size:15px;font-weight:500;margin-right:6px}.workspace-summary>i{height:12px;border-left:1px solid var(--border)}.summary-note{margin-left:auto;display:flex;gap:7px;align-items:center;font-size:10px;color:var(--faint)}.summary-note em{height:5px;width:5px;border-radius:50%;background:var(--green)}.workspace-tabs{display:flex;gap:25px;border-bottom:1px solid var(--border);margin-bottom:23px}.workspace-tabs button{display:flex;align-items:center;gap:7px;background:none;border:0;border-bottom:2px solid transparent;color:var(--muted);padding:0 2px 14px;font-size:12px;cursor:pointer}.workspace-tabs button.active{font-weight:600;color:var(--text);border-color:var(--accent)}.workspace-tabs button>span{background:var(--panel-2);font:10px Consolas,monospace;padding:2px 5px;border-radius:4px}.list-toolbar{display:flex;align-items:center;justify-content:space-between;gap:15px;margin-bottom:17px}.filter-chips{display:flex;gap:5px}.filter-chips button{background:none;border:1px solid transparent;border-radius:6px;color:var(--muted);font-size:10px;padding:7px 10px;cursor:pointer}.filter-chips button.active{background:var(--accent-soft);color:var(--text);border-color:var(--border-soft)}.search-field{display:flex;gap:8px;align-items:center;border:1px solid var(--border);border-radius:7px;background:var(--panel);padding:0 10px;width:250px}.search-field>span{color:var(--faint);font-size:21px}.search-field input{min-width:0;width:100%;background:none;border:0;padding:9px 0;color:var(--text);outline:none;font-size:11px}.services-table{border:1px solid var(--border);border-radius:11px;overflow:hidden;background:var(--panel)}.table-labels,.service-row{display:grid;grid-template-columns:minmax(220px,1.1fr) minmax(180px,1fr) 210px;gap:15px;align-items:center}.table-labels{padding:12px 20px;color:var(--faint);font-size:9px;background:var(--bg-soft);border-bottom:1px solid var(--border)}.table-labels>span:last-child{text-align:right}.service-row{padding:21px 20px;border-bottom:1px solid var(--border-soft);transition:background .15s}.service-row:last-child{border-bottom:0}.service-row:hover{background:var(--bg-soft)}.service-identity{display:flex;gap:13px;align-items:center;min-width:0}.service-identity>div{min-width:0}.service-symbol{display:flex;align-items:center;justify-content:center;flex-shrink:0;width:41px;height:41px;border-radius:11px;background:var(--accent-soft);border:1px solid var(--border-soft);color:var(--text)}.service-symbol.remote{background:var(--green-soft);color:var(--green)}.service-symbol svg{width:22px;height:22px}.service-symbol.mini{width:34px;height:34px;font-size:20px}.service-identity h3{display:flex;align-items:center;gap:8px;flex-wrap:wrap;font-size:12px;font-weight:600;margin:0 0 7px;overflow-wrap:anywhere}.service-identity p{font-size:10px;line-height:1.6;color:var(--faint);margin:0;display:flex;align-items:center;gap:7px;flex-wrap:wrap}.transport-tag{font:8px Consolas,monospace;padding:3px 5px;border:1px solid var(--border-soft);border-radius:4px;color:var(--muted);white-space:nowrap}.origin-label{font-size:9px;color:var(--faint);padding-left:7px;border-left:1px solid var(--border)}.service-actions{display:flex;gap:12px;justify-content:flex-end;align-items:center}.btn.compact{font-size:10px;padding:7px 10px;white-space:nowrap}.text-action{font-size:10px;border:0;padding:0;background:none;color:var(--muted);cursor:pointer;white-space:nowrap}.text-action:hover{color:var(--text)}.text-action.danger:hover{color:var(--red)}.subtle-note{display:flex;gap:8px;font-size:10px;line-height:1.8;color:var(--faint);padding:18px 2px}.empty-state{border:1px dashed var(--border);border-radius:11px;min-height:335px;display:flex;flex-direction:column;justify-content:center;align-items:center;text-align:center;padding:35px 24px}.empty-state h2{font-size:19px;margin:18px 0 10px;font-weight:550}.empty-state h3{font-size:14px;margin:0 0 9px}.empty-state p{color:var(--muted);font-size:12px;line-height:1.8;margin:0 0 21px}.empty-plug{font-size:35px;border:1px solid var(--border);border-radius:16px;background:var(--bg-soft);width:66px;height:66px;display:grid;place-items:center;color:var(--muted)}.empty-examples{display:flex;gap:18px;margin-top:24px;font-size:9px;color:var(--faint)}.empty-state.small{min-height:180px;border:0;padding:30px 20px}.agent-workspace{display:grid;grid-template-columns:210px minmax(0,1fr);border:1px solid var(--border);border-radius:11px;overflow:hidden;min-height:450px}.agent-sidebar{background:var(--bg-soft);border-right:1px solid var(--border);padding:17px 10px;max-height:700px;overflow:auto}.sidebar-label{display:flex;align-items:center;justify-content:space-between;padding:0 6px 14px;font-size:9px;color:var(--faint);letter-spacing:1px}.sidebar-label label{display:flex;align-items:center;gap:3px;font-size:9px;letter-spacing:0}.sidebar-label input{accent-color:var(--accent)}.agent-sidebar>button{display:flex;gap:9px;align-items:center;width:100%;text-align:left;background:none;border:1px solid transparent;border-radius:7px;padding:11px 8px;cursor:pointer;color:var(--text);margin-bottom:3px}.agent-sidebar>button.active{background:var(--panel);border-color:var(--border);box-shadow:var(--shadow-sm)}.agent-avatar{width:28px;height:28px;display:grid;place-items:center;border:1px solid var(--border);border-radius:7px;background:var(--panel);font:12px Georgia,serif;flex-shrink:0}.agent-sidebar b{font-size:10px;font-weight:500}.agent-sidebar small{display:block;color:var(--faint);font-size:8px;margin-top:5px}.agent-sidebar>button>div{min-width:0;flex:1}.agent-count{font:10px Consolas,monospace;color:var(--muted)}.agent-panel{padding:24px;min-width:0}.agent-panel>header{display:flex;align-items:center;justify-content:space-between;gap:16px;margin-bottom:18px}.agent-panel h2{font-size:21px;margin:6px 0 7px;font-weight:550}.agent-panel>header p{font-size:11px;color:var(--muted);margin:0}.permission-card,.info-card{font-size:11px;color:var(--muted);line-height:1.8;padding:13px 15px;border:1px solid var(--border);background:var(--bg-soft);border-radius:7px;margin:15px 0}.permission-card{display:flex;justify-content:space-between;align-items:center;gap:13px;background:var(--accent-soft)}.config-details{border:1px solid var(--border);border-radius:7px;margin-bottom:26px}.config-details summary{display:flex;align-items:center;gap:12px;padding:12px;color:var(--muted);cursor:pointer;font-size:10px;list-style:none}.config-details summary>span:first-child{flex-shrink:0}.config-details summary>span:last-child{margin-left:auto}.config-details code{font:9px Consolas,monospace;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;color:var(--faint)}.config-details>div{padding:0 14px 14px}.config-details p{font-size:11px;line-height:1.8;color:var(--muted)}a{color:var(--accent);text-decoration:none;font-size:10px}.config-details .heading-actions{gap:16px;flex-wrap:wrap}.agent-service-heading{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:0 0 13px;border-bottom:1px solid var(--border)}.agent-service-heading h3{font-size:12px;font-weight:550;margin:0}.agent-service-heading h3>span{margin-left:7px;font:10px Consolas,monospace;color:var(--faint)}.agent-service-heading>span{font-size:9px;color:var(--faint)}.agent-service{display:flex;align-items:center;justify-content:space-between;gap:12px;padding:20px 0;border-bottom:1px solid var(--border-soft)}.agent-service-actions{display:flex;gap:13px;align-items:center}.switch{height:20px;width:35px;padding:2px;border:0;border-radius:11px;background:var(--border);cursor:pointer;display:flex;align-items:center;flex-shrink:0}.switch i{height:16px;width:16px;border-radius:50%;background:#fff;box-shadow:0 1px 3px #0002;transition:transform .15s}.switch.on{background:var(--green)}.switch.on i{transform:translateX(15px)}button:disabled{opacity:.45;cursor:not-allowed}.discover-heading{display:flex;align-items:center;justify-content:space-between;gap:18px;margin:4px 0 22px}.discover-heading h2{font-size:18px;margin:0 0 8px;font-weight:550}.discover-heading p{font-size:11px;color:var(--muted);margin:0;line-height:1.7}.discovery-switch{display:flex;border:1px solid var(--border);border-radius:7px;padding:3px;background:var(--bg-soft);flex-shrink:0}.discovery-switch button{border:0;border-radius:5px;background:none;color:var(--muted);padding:7px 12px;font-size:10px;cursor:pointer}.discovery-switch button.active{background:var(--panel);color:var(--text);box-shadow:var(--shadow-sm)}select{border:1px solid var(--border);border-radius:6px;background:var(--panel);color:var(--text);font-size:11px;padding:9px 12px}.source-block{border:1px solid var(--border);border-radius:9px;margin-bottom:17px;overflow:hidden}.source-block>header{display:flex;justify-content:space-between;align-items:center;padding:16px 19px;background:var(--bg-soft);gap:16px}.source-block h3{font-size:12px;margin:0 0 8px}.source-block code{font:9px/1.6 Consolas,monospace;overflow-wrap:anywhere;color:var(--faint)}.source-item{display:flex;align-items:center;justify-content:space-between;padding:14px 19px;border-top:1px solid var(--border-soft);gap:15px}.source-item b{font-size:11px;font-weight:550;overflow-wrap:anywhere}.source-item span{display:block;font-size:9px;color:var(--faint);margin-top:6px}.mcp-overlay{position:fixed;inset:0;z-index:100;background:rgba(17,24,39,.32);backdrop-filter:blur(3px);display:flex;align-items:center;justify-content:center;padding:24px}.workspace-dialog{width:660px;max-width:100%;max-height:85vh;display:flex;flex-direction:column;background:var(--panel);border:1px solid var(--border);border-radius:14px;box-shadow:0 24px 80px #0002}.workspace-dialog>header{display:flex;justify-content:space-between;align-items:flex-start;gap:15px;padding:25px 26px 20px}.workspace-dialog h2{font-size:20px;margin:8px 0 9px;font-weight:550;overflow-wrap:anywhere}.workspace-dialog header p{font-size:11px;line-height:1.8;color:var(--muted);margin:0}.icon-close{border:0;background:none;font-size:24px;color:var(--muted);cursor:pointer}.dialog-body{padding:0 26px 20px;overflow:auto}.workspace-dialog>footer{border-top:1px solid var(--border);padding:17px 26px;display:flex;justify-content:flex-end;align-items:center;gap:9px}.workspace-dialog>footer>span{margin-right:auto;font-size:9px;color:var(--faint)}.assignment-row{display:flex;align-items:center;gap:12px;padding:16px 0;border-bottom:1px solid var(--border-soft);cursor:pointer}.assignment-row>div{flex:1;min-width:0}.assignment-row b{font-size:12px;font-weight:550}.assignment-row small{display:block;font:9px/1.7 Consolas,monospace;color:var(--faint);margin-top:5px;overflow-wrap:anywhere}.assignment-row input{accent-color:var(--accent);width:16px;height:16px}.preview-agent{padding:16px 0;border-top:1px solid var(--border-soft)}.preview-agent h3{font-size:12px;margin:0 0 8px}.preview-agent>code{display:block;font:9px/1.7 Consolas,monospace;color:var(--faint);overflow-wrap:anywhere;margin-bottom:10px}.preview-action{display:flex;gap:12px;align-items:center;padding:9px 0;font-size:11px}.preview-action>span{border-radius:4px;font-size:9px;background:var(--bg-soft);padding:4px 7px;color:var(--muted)}.preview-action .add,.preview-action .resume{color:var(--green);background:var(--green-soft)}.preview-action .pause,.preview-action .remove{color:var(--amber);background:var(--amber-soft)}.preview-action b{font-weight:500;overflow-wrap:anywhere}.success-text{color:var(--green);font-size:10px;font-weight:400;margin-left:8px}.workspace-error{background:var(--red-soft);color:var(--red);font-size:11px;line-height:1.8;padding:12px 14px;border-radius:7px;margin:12px 0}.workspace-error button{border:0;background:none;color:var(--red);cursor:pointer}.compact-dialog{width:500px}button:focus-visible,input:focus-visible,select:focus-visible,summary:focus-visible,a:focus-visible{outline:2px solid var(--accent);outline-offset:3px}@media(max-width:1150px){.table-labels,.service-row{grid-template-columns:minmax(200px,1.1fr) minmax(110px,.7fr) 185px;gap:10px}.service-actions{gap:9px}.agent-workspace{grid-template-columns:185px minmax(0,1fr)}.agent-panel{padding:18px}.agent-service-actions{gap:9px}.workspace-tabs{gap:19px}.workspace-summary{gap:12px}}@media(max-width:950px){.service-row{grid-template-columns:1fr auto}.service-actions{grid-column:2;grid-row:1/3}.table-labels>span:nth-child(2){display:none}.table-labels{grid-template-columns:1fr auto}.agent-service{flex-wrap:wrap}.agent-service-actions{margin-left:47px}.summary-note{display:none}.workspace-heading{gap:10px}.heading-actions{gap:7px}}
.services-table .table-labels,.services-table .service-row { grid-template-columns:minmax(200px,1fr) minmax(340px,1.5fr) 142px; gap:18px; }
.services-table .service-actions { flex-wrap:wrap; gap:10px; }
.mcp-assignments { padding:2px 0; }
.assignment-selection { display:flex; align-items:center; gap:14px; padding:10px 0; border-bottom:1px solid var(--border-soft); }
.assignment-selection > span { margin-right:auto; color:var(--muted); font-size:11px; }
@media(max-width:1150px) { .services-table .table-labels,.services-table .service-row { grid-template-columns:minmax(180px,1fr) minmax(300px,1.4fr) 112px; gap:12px; } }
.agent-config-actions { display:flex; align-items:center; gap:20px; margin-bottom:20px; }
</style>
