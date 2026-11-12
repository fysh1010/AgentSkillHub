<script setup lang="ts">
import { computed, ref, onMounted, onUnmounted } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import { mcpMarketSources, type McpServer, type McpMarketSource } from '../api/mcp';
import SkillIcon from './SkillIcon.vue';
import McpMarketSources from './McpMarketSources.vue';

interface Input { name?: string; description?: string; value?: string; default?: string; isRequired?: boolean; isSecret?: boolean; type?: string; variables?: Record<string, Input>; isRepeated?: boolean }
interface Endpoint { type: string; url: string; headers?: Input[]; variables?: Record<string, Input> }
interface Package { registryType: string; identifier: string; version?: string; transport?: { type: string }; environmentVariables?: Input[]; packageArguments?: Input[]; runtimeArguments?: Input[]; registryBaseUrl?: string }
interface Listing { name: string; title?: string; description?: string; version?: string; remotes?: Endpoint[]; packages?: Package[]; icons?: {src:string}[]; repository?: {url?:string}; config?: Record<string,unknown> }
interface Page { servers: { server: Listing; _meta?: Record<string, { status?: string }> }[]; metadata?: { nextCursor?: string } }
const emit = defineEmits<{ configure: [server: McpServer] }>();
const query = ref(''); const items = ref<Listing[]>([]); const busy = ref(false); const error = ref('');
const cursor = ref(''); const activeQuery = ref(''); const chosen = ref<Listing>(); const option = ref(0); const values = ref<Record<string,string>>({});
const formError = ref('');
const sources=ref<McpMarketSource[]>([]); const sourceId=ref('official'); const manageSources=ref(false);
const source=computed(()=>sources.value.find(s=>s.id===sourceId.value));
let request=0;
onUnmounted(()=>{++request;});
function icon(s:Listing) {
  for(const entry of s.icons || []) {
    try {const url=new URL(entry.src,source.value?.url);if(['https:','http:'].includes(url.protocol))return url.href;} catch { /* 继续尝试下一张图片 */ }
  }
  return '';
}
function publisherIcon(s:Listing) {
  try {const url=new URL(s.repository?.url || '');const owner=url.pathname.split('/')[1];if(url.hostname==='github.com' && /^[a-zA-Z0-9-]+$/.test(owner || ''))return `https://github.com/${owner}.png?size=80`;} catch { /* 无发布者图标 */ }
  return '';
}
async function savedSources(rows:McpMarketSource[]) {const added=rows.find(s=>!sources.value.some(old=>old.id===s.id));sources.value=rows;manageSources.value=false;if(added)sourceId.value=added.id;else if(!rows.some(s=>s.id===sourceId.value))sourceId.value='official';await search();}
const options = computed(() => {
  const s = chosen.value;
  return [
    ...(s?.remotes || []).filter(r => ['streamable-http','sse'].includes(r.type)).map(r => ({ label: r.type === 'sse' ? '远程 SSE' : '远程 HTTP', remote: r, pkg: undefined as Package | undefined })),
    ...(s?.packages || []).filter(p => p.registryType === 'npm' && p.transport?.type === 'stdio' && (!p.registryBaseUrl || p.registryBaseUrl === 'https://registry.npmjs.org') && (!p.runtimeArguments?.length || p.runtimeArguments.every(a=>a.value==='-y'||a.value==='--yes'))).map(p => ({ label: `本地 npm · ${p.identifier}`, remote: undefined as Endpoint | undefined, pkg: p })),
  ];
});
const fields = computed(() => {
  const o = options.value[option.value]; const out: {key:string; field:Input}[] = [];
  for (const [name,f] of Object.entries(o?.remote?.variables || {})) out.push({key:`url:${name}`,field:{...f,name}});
  for (const f of o?.remote?.headers || o?.pkg?.environmentVariables || []) {
    if (f.variables) { for (const [name,v] of Object.entries(f.variables)) out.push({key:`var:${f.name}:${name}`,field:{...v,name,description:f.description,isRequired:f.isRequired || v.isRequired}}); }
    else out.push({key:`input:${f.name}`,field:f});
  }
  for (const [i,f] of (o?.pkg?.packageArguments || []).entries()) {
    if (f.variables) { for (const [name,v] of Object.entries(f.variables)) out.push({key:`argvar:${i}:${name}`,field:{...v,name,isRequired:f.isRequired || v.isRequired}}); }
    else if (!f.value) out.push({key:`arg:${i}`,field:{...f,name:f.name || f.description || `参数 ${i+1}`}});
  }
  return out;
});
async function search(more = false) {
  if (more && busy.value) return;
  const id=++request;
  busy.value = true; error.value = '';
  try {localStorage.setItem('ash.mcpMarketSource',sourceId.value);} catch { /* 存储不可用时仍可使用 */ }
  if (!more) { activeQuery.value = query.value.trim(); cursor.value = ''; items.value = []; }
  try {
    const page = await invoke<Page>('mcp_market_search', {query:activeQuery.value,cursor:more ? cursor.value : null,sourceId:sourceId.value});
    if(id!==request)return;
    const rows = page.servers.filter(r => !r._meta?.['io.modelcontextprotocol.registry/official']?.status || r._meta['io.modelcontextprotocol.registry/official'].status === 'active').map(r=>r.server);
    const map = new Map((more ? items.value : []).map(s=>[s.name,s])); rows.forEach(s=>map.set(s.name,s));
    items.value = [...map.values()]; cursor.value = page.metadata?.nextCursor || '';
  } catch(e) { if(id===request)error.value = String(e); } finally { if(id===request)busy.value = false; }
}
function choose(s: Listing) {
  if(s.config && typeof s.config==='object' && !Array.isArray(s.config)) {emit('configure',{name:s.name.replace(/[^a-zA-Z0-9._-]/g,'-').slice(0,150),config:s.config});return;}
  chosen.value=s; option.value=0; values.value={}; formError.value='';
}
function fieldValue(key: string, f: Input) { return values.value[key] ?? f.default ?? f.value ?? ''; }
function configure() {
  try {
    for (const {key,field} of fields.value) if (field.isRequired && !fieldValue(key,field).trim()) throw new Error(`请填写 ${field.name}`);
    const o = options.value[option.value]; if (!o || !chosen.value) return;
    const interpolate = (text: string, prefix: string, vars?: Record<string,Input>) => text.replace(/\{([^{}]+)\}/g, (_,name:string)=>{
      const f=vars?.[name]; if (!f) throw new Error(`未定义的变量：${name}`);
      const v=fieldValue(`${prefix}${name}`,f); if (!v && f.isRequired) throw new Error(`请填写 ${name}`); return prefix==='url:' ? encodeURIComponent(v) : v;
    });
    let config: Record<string,unknown>;
    if (o.remote) {
      const url=interpolate(o.remote.url,'url:',o.remote.variables);
      if (!url.startsWith('https://')) throw new Error('市场远程服务必须使用 HTTPS');
      const headers: Record<string,string> = {};
      for (const h of o.remote.headers || []) { const v=h.variables ? interpolate(h.value || '',`var:${h.name}:`,h.variables) : fieldValue(`input:${h.name}`,h); if(v && h.name) headers[h.name]=v; }
      config={type:o.remote.type==='sse'?'sse':'http',url,...(Object.keys(headers).length?{headers}:{})};
    } else {
      const p=o.pkg!;
      if (!/^(?:@[a-z0-9._-]+\/)?[a-z0-9][a-z0-9._-]*$/.test(p.identifier) || !p.version || !/^[0-9][a-zA-Z0-9.+-]*$/.test(p.version)) throw new Error('包名或固定版本无效，请查看发布者说明');
      const args=['-y',`${p.identifier}@${p.version}`];
      for (const [i,a] of (p.packageArguments || []).entries()) {
        if(a.isRepeated) throw new Error('该服务需要重复参数，请在高级配置中按发布者说明设置');
        const v=a.variables?interpolate(a.value || '',`argvar:${i}:`,a.variables):a.value || fieldValue(`arg:${i}`,a);
        if(v) {if(a.type==='named' && a.name) args.push(a.name); args.push(v);}
      }
      const env: Record<string,string>={}; for(const f of p.environmentVariables || []) {const v=f.variables?interpolate(f.value || '',`var:${f.name}:`,f.variables):fieldValue(`input:${f.name}`,f); if(v && f.name)env[f.name]=v;}
      config={command:'npx',args,...(Object.keys(env).length?{env}:{})};
    }
    emit('configure',{name:chosen.value.name.replace(/[^a-zA-Z0-9._-]/g,'-').slice(0,150),config}); chosen.value=undefined;
  } catch(e) {formError.value=String(e instanceof Error?e.message:e);}
}
onMounted(async()=>{try{sources.value=await mcpMarketSources();const saved=localStorage.getItem('ash.mcpMarketSource');if(saved&&sources.value.some(s=>s.id===saved))sourceId.value=saved;await search();}catch(e){error.value=String(e);}});
</script>

<template>
  <section class="market">
    <div class="intro"><div><h2>MCP 市场</h2><p>从选定来源查找 MCP，配置后分配给已开启的 Agent。</p></div><button class="btn" @click="manageSources=true">管理市场源</button></div>
    <div class="source-toolbar"><label>当前来源<select v-model="sourceId" aria-label="MCP 市场来源" @change="search()"><option v-for="s in sources" :key="s.id" :value="s.id">{{s.name}}</option></select></label><a v-if="source" :href="source.url" target="_blank" rel="noopener noreferrer">查看来源 ↗</a></div>
    <form class="searchbar" @submit.prevent="search()"><input v-model="query" placeholder="搜索服务，例如 filesystem、github、browser" aria-label="搜索 MCP 市场"/><button class="btn primary" :disabled="busy">{{busy?'正在搜索…':'搜索市场'}}</button></form>
    <p class="hint">Registry 收录不代表官方审核。远程服务可能需要授权；本地 npm 服务需要 Node.js，首次由 Agent 加载时下载固定版本。</p>
    <div v-if="error" role="alert" class="error">{{error}} <button class="btn" :disabled="busy" @click="search()">重试</button></div>
    <div class="grid"><article v-for="s in items" :key="s.name"><div class="listing-heading"><SkillIcon :name="s.title||s.name" :identity="s.name" :src="icon(s)" :fallback-src="publisherIcon(s)" /><div><span class="version">{{s.version||'自定义清单'}}</span><h3>{{s.title || s.name}}</h3></div></div><code>{{s.name}}</code><p>{{s.description||'发布者未提供说明'}}</p><button class="btn" @click="choose(s)">配置安装 →</button></article></div>
    <p v-if="!busy && !error && !items.length">没有匹配的服务，请换个关键词。</p>
    <button v-if="cursor" class="btn more" :disabled="busy" @click="search(true)">{{busy?'正在加载…':'加载更多'}}</button>
    <McpMarketSources v-if="manageSources" :sources="sources" @close="manageSources=false" @saved="savedSources" />
    <div v-if="chosen" class="mask" @click.self="chosen=undefined"><section class="dialog" role="dialog" aria-modal="true" aria-labelledby="market-title">
      <h2 id="market-title">配置 {{chosen.title || chosen.name}}</h2><p class="hint">确认服务参数后，选择目标 Agent 并预览写入。</p>
      <template v-if="options.length"><label>安装方式<select v-model="option"><option v-for="(o,i) in options" :key="i" :value="i">{{o.label}}</option></select></label>
      <p v-if="options[option]?.remote" class="endpoint">{{options[option]?.remote?.url}}</p>
      <label v-for="{key,field} in fields" :key="key">{{field.name}} {{field.isRequired?'*':''}}<small>{{field.description}}</small><input :value="fieldValue(key,field)" :type="field.isSecret?'password':'text'" autocomplete="off" @input="values[key]=($event.target as HTMLInputElement).value"/></label></template>
      <p v-else>该服务的安装格式暂不支持自动生成。请按发布者文档在「添加 MCP」中配置。</p>
      <p v-if="formError" class="error" role="alert">{{formError}}</p><footer><button class="btn" @click="chosen=undefined">取消</button><button v-if="options.length" class="btn primary" @click="configure">选择 Agent →</button></footer>
    </section></div>
  </section>
</template>

<style scoped>
.source-toolbar{display:flex;align-items:center;gap:16px;margin-top:18px}.source-toolbar label{display:flex;gap:12px;align-items:center;font-size:12px;color:var(--muted)}.source-toolbar select{width:260px;padding:8px 10px}.listing-heading{display:flex;gap:12px;align-items:center;min-height:44px}.listing-heading>div{min-width:0}.listing-heading h3{margin-top:5px}
.intro,.searchbar{display:flex;align-items:center;justify-content:space-between;gap:14px}.intro p,.hint{font-size:12px;line-height:1.8;color:var(--muted)}a{font-size:12px;color:var(--accent)}.searchbar{margin:20px 0}.searchbar input{flex:1}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(260px,1fr));gap:16px}.grid article{border:1px solid var(--border);border-radius:10px;padding:20px;background:var(--panel);display:flex;flex-direction:column;align-items:start;gap:10px;min-width:0}.grid h3{font-size:14px;margin:0;overflow-wrap:anywhere}.grid code{font-size:10px;color:var(--faint);overflow-wrap:anywhere}.grid p{font-size:12px;line-height:1.8;color:var(--muted);flex:1;overflow-wrap:anywhere}.version{font-size:10px;color:var(--accent)}input,select{padding:10px 12px;background:var(--bg-soft);color:var(--text);border:1px solid var(--border);border-radius:6px;width:100%}.more{margin:20px auto;display:block}.error{color:var(--red);font-size:12px;margin:14px 0}.mask{position:fixed;inset:0;z-index:110;background:#0006;display:flex;align-items:center;justify-content:center;padding:24px}.dialog{background:var(--panel);border:1px solid var(--border);border-radius:12px;padding:26px;width:620px;max-width:100%;max-height:85vh;overflow:auto}.dialog label{display:block;margin:16px 0;font-size:12px}.dialog small{display:block;color:var(--muted);margin:6px 0;line-height:1.6}.dialog input,.dialog select{margin-top:8px}.dialog footer{display:flex;justify-content:flex-end;gap:12px;margin-top:24px}.endpoint{font:11px/1.8 monospace;overflow-wrap:anywhere;color:var(--muted)}
</style>
