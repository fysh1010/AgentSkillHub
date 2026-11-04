import {reactive} from 'vue';
import {mcpProbeCheck,mcpProbeCancel} from '../api/mcp';
import type {McpProbeResult,McpServer} from '../api/mcp';
export interface McpCheck {id:string;state:string;result?:McpProbeResult;promise:Promise<McpProbeResult>;resolve:(r:McpProbeResult)=>void}
const checks=reactive<Record<string,McpCheck>>({});
const queue:{key:string;server:McpServer;item:McpCheck}[]=[];let active=0;let seq=0;
export function mcpCheckKey(server:McpServer){const c={...server.config};if(c.command&&(!c.type||c.type==='stdio'))delete c.type;if(c.url&&(!c.type||c.type==='http'||c.type==='streamable-http'))delete c.type;delete c.disabled;delete c.enabled;for(const k of ['env','headers'])if(c[k]&&Object.keys(c[k] as object).length===0)delete c[k];if(Array.isArray(c.args)&&!c.args.length)delete c.args;return JSON.stringify(c,(_k,v)=>v&&typeof v==='object'&&!Array.isArray(v)?Object.fromEntries(Object.entries(v).sort(([a],[b])=>a.localeCompare(b))):v);}
function failure(message:string,state='failed'):McpProbeResult{return {state,code:state,phase:'prepare',message,details:'',server_name:'',server_version:'',protocol:'',tool_count:0,tools:[],elapsed_ms:0,checked_at:Date.now()/1000,partial:false};}
function pump(){
 while(active<2&&queue.length){const next=queue.shift()!;if(next.item.state==='cancelled')continue;active++;next.item.state='checking';
  void mcpProbeCheck(next.item.id,next.server).catch(e=>failure(String(e))).then(result=>{next.item.state=result.state;next.item.result=result;next.item.resolve(result);}).finally(()=>{active--;pump();});
 }
}
export function useMcpChecks(){
 function check(server:McpServer,force=true){const key=mcpCheckKey(server),old=checks[key];if(old&&['queued','checking'].includes(old.state))return old.promise;if(!force&&old?.result&&Date.now()/1000-old.result.checked_at<30)return old.promise;
  let resolve!:(r:McpProbeResult)=>void;const promise=new Promise<McpProbeResult>(r=>resolve=r);const item=reactive<McpCheck>({id:'mcp-check-'+Date.now()+'-'+(++seq),state:'queued',promise,resolve});checks[key]=item;queue.push({key,server:JSON.parse(JSON.stringify(server)),item});pump();return promise;
 }
 function cancel(server:McpServer){const item=checks[mcpCheckKey(server)];if(!item)return;if(item.state==='queued'){item.state='cancelled';item.result=failure('已取消排队','cancelled');item.resolve(item.result);}else if(item.state==='checking')void mcpProbeCancel(item.id);}
 return {checks,check,cancel};
}
