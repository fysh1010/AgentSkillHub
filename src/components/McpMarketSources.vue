<script setup lang="ts">
import { ref } from 'vue';
import { mcpSaveMarketSources, type McpMarketSource } from '../api/mcp';
import { useEsc } from '../composables/useEsc';
const props=defineProps<{sources:McpMarketSource[]}>();
const emit=defineEmits<{close:[]; saved:[sources:McpMarketSource[]]}>();
const drafts=ref(props.sources.filter(s=>s.id!=='official').map(s=>({...s})));
const busy=ref(false); const error=ref('');
const sample=JSON.stringify({servers:[{name:'my-service',title:'我的 MCP',description:'服务说明',icons:[{src:'https://example.com/icon.png'}],config:{type:'http',url:'https://example.com/mcp'}}]},null,2);
function close(){if(!busy.value)emit('close');}
useEsc(close);
function add(){drafts.value.push({id:`source-${Date.now()}-${Math.random().toString(36).slice(2,7)}`,name:'',url:'',kind:'registry'});}
async function save(){busy.value=true;error.value='';try{emit('saved',await mcpSaveMarketSources(drafts.value));}catch(e){error.value=String(e);}finally{busy.value=false;}}
</script>
<template>
  <div class="modal-mask" @click.self="close"><section class="modal source-dialog" role="dialog" aria-modal="true" aria-label="管理 MCP 市场源"><header class="modal-head"><b>管理 MCP 市场源</b><button class="btn ghost" :disabled="busy" @click="close">✕</button></header>
    <div class="modal-body"><p>可添加兼容 MCP Registry 的地址，或公开 JSON 清单地址。保存后在市场切换来源；服务仍需配置和预览后才会写入 Agent。</p><div class="official"><b>MCP 官方 Registry</b><code>registry.modelcontextprotocol.io</code><span class="badge gray">默认保留</span></div>
      <fieldset v-for="(s,index) in drafts" :key="s.id" :disabled="busy"><div class="source-fields"><label>源名称<input v-model="s.name" class="input" placeholder="例如：团队 MCP" maxlength="80" aria-label="MCP 源名称" /></label><label>源格式<select v-model="s.kind" class="input" aria-label="MCP 源格式"><option value="registry">MCP Registry API</option><option value="json">JSON 清单</option></select></label></div><label>地址<input v-model="s.url" class="input" type="url" :placeholder="s.kind==='registry'?'https://registry.example.com/v0.1/servers':'https://example.com/mcp.json'" aria-label="MCP 源地址" /></label><button class="btn ghost danger" @click="drafts.splice(index,1)">移除此源</button></fieldset>
      <button class="btn" :disabled="busy||drafts.length>=20" @click="add">＋ 添加 MCP 源</button><details><summary>JSON 清单格式示例</summary><p>支持 servers 列表、官方 Registry 响应，以及 { mcpServers: { 服务名: 配置 } }。图标使用 icons[].src，推荐 HTTPS 图片。</p><pre>{{sample}}</pre></details><p v-if="error" class="error" role="alert">{{error}}</p>
    </div><footer class="modal-foot"><button class="btn" :disabled="busy" @click="close">取消</button><button class="btn primary" :disabled="busy" @click="save">{{busy?'正在保存…':'保存来源'}}</button></footer>
  </section></div>
</template>
<style scoped>
.source-dialog{max-width:750px}p{font-size:12px;line-height:1.8;color:var(--muted)}fieldset{border:1px solid var(--border);border-radius:8px;margin:14px 0;padding:14px}.source-fields{display:grid;grid-template-columns:1fr 190px;gap:14px}label{display:block;font-size:11px;color:var(--muted);margin-bottom:12px}.input{margin-top:6px;width:100%}.official{display:flex;gap:12px;align-items:center;padding:13px;background:var(--bg-soft);font-size:12px}.official code{font-size:11px;flex:1;color:var(--muted)}details{margin-top:20px;font-size:12px}pre{font:11px/1.6 Consolas,monospace;white-space:pre-wrap;padding:12px;background:var(--bg-soft);border-radius:8px}.error{color:var(--red)}
</style>
