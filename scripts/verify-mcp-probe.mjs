import assert from 'node:assert/strict';
import {readFile,writeFile} from 'node:fs/promises';
import {spawn} from 'node:child_process';
import path from 'node:path';
import {setTimeout as pause} from 'node:timers/promises';

export async function verifyProbeBackend({invoke,root,checks}){
 const fixture=path.join(root,'MCP server with spaces.mjs');
 await writeFile(fixture,await readFile('scripts/mcp-probe-fixture.mjs'));
 const log=path.join(root,'probe-methods.jsonl');const token='fixture-secret-not-a-real-credential';let seq=0;
 const local=(mode='stdio',seconds=3)=>({name:'probe-'+mode,check_timeout_sec:seconds,config:{command:process.execPath,args:[fixture,mode,path.join(root,'pid-'+(++seq))],env:{MCP_FIXTURE_LOG:log,MCP_FIXTURE_TOKEN:token}}});
 const probe=(server,id='fixture-probe-'+(++seq))=>invoke('mcp_probe_check',{request:{id,server}});
 let result=await probe(local());
 assert.equal(result.state,'ok',JSON.stringify(result));assert.equal(result.tool_count,2);assert.deepEqual(result.tools.map(t=>t.name),['first_tool','second_tool']);
 result=await probe(local('stdio-ping'));assert.equal(result.state,'ok',JSON.stringify(result));
 result=await probe(local('stdio-empty'));assert.equal(result.state,'connected');assert.equal(result.tool_count,0);
 result=await probe(local('stdio-bad-tools'));assert.equal(result.code,'protocol_error');
 result=await probe(local('stdio-version'));assert.equal(result.code,'unsupported_protocol');
 result=await probe(local('stdio-auth'));assert.equal(result.state,'auth_required');assert(!JSON.stringify(result).includes(token),'RPC 错误暴露了凭证');
 result=await probe(local('stdio-exit'));assert.equal(result.code,'process_exit');assert(!JSON.stringify(result).includes(token),'stderr 暴露了凭证');
 const reflected=local('stdio-tool-secret');reflected.config.env.MCP_FIXTURE_TOKEN='fixture-sensitive-'+('z'.repeat(300));result=await probe(reflected);assert.equal(result.state,'ok');assert(!JSON.stringify(result).includes(reflected.config.env.MCP_FIXTURE_TOKEN.slice(0,80)),'工具描述截断前未遮蔽长凭证');
 result=await probe({name:'missing',config:{command:'agentskillhub-no-such-command-123'},check_timeout_sec:1});assert.equal(result.code,'missing_command');
 result=await probe({name:'missing-env',config:{command:process.execPath,env:{TOKEN:'${AGENTSKILLHUB_NONEXISTENT_ENV_638297}'}},check_timeout_sec:1});assert.equal(result.code,'missing_env');
 checks.push('真实 STDIO 握手、分页工具发现、双向 ping、空工具与协议错误');
 checks.push('缺少命令/变量、认证错误和 stderr 凭证遮蔽');

 const tree=local('stdio-tree',1);result=await probe(tree);assert.equal(result.code,'timeout');assert(result.elapsed_ms<3500,'超时后进程清理过慢');
 const rootPid=Number(await readFile(tree.config.args[2],'utf8')),childPid=Number(await readFile(tree.config.args[2]+'.child','utf8'));
 const alive=pid=>{try{process.kill(pid,0);return true;}catch{return false;}};
 for(let i=0;i<15&&(alive(rootPid)||alive(childPid));i++)await pause(100);
 assert(!alive(rootPid)&&!alive(childPid),'检测结束残留进程树');
 const cancelId='cancel-probe-'+(++seq),promise=probe(local('stdio-timeout',8),cancelId);await pause(200);await invoke('mcp_probe_cancel',{id:cancelId});result=await promise;assert.equal(result.state,'cancelled');
 checks.push('检测超时、主动停止以及 Windows 子进程树清理');

 const http=spawn(process.execPath,[fixture,'http'],{env:{...process.env,MCP_FIXTURE_LOG:log},windowsHide:true,stdio:['ignore','pipe','pipe']});
 try{
  const port=await new Promise((resolve,reject)=>{let text='';http.stdout.on('data',bytes=>{text+=bytes;const line=text.split('\n')[0];try{resolve(JSON.parse(line).port);}catch{}});http.once('error',reject);http.once('exit',code=>reject(new Error('HTTP fixture exited '+code)));});
  const remote=(pathname,type='http')=>({name:'remote-fixture',config:{type,url:'http://127.0.0.1:'+port+pathname,headers:{Authorization:'Bearer '+token}},check_timeout_sec:3});
  for(const endpoint of ['/mcp','/http-sse','/http-ping']){result=await probe(remote(endpoint));assert.equal(result.state,'ok',endpoint+' '+JSON.stringify(result));assert.equal(result.tool_count,2);}
  result=await probe(remote('/sse','sse'));assert.equal(result.state,'ok',JSON.stringify(result));assert.equal(result.tool_count,2);
  result=await probe(remote('/auth'));assert.equal(result.state,'auth_required');assert(!JSON.stringify(result).includes(token));
  result=await probe(remote('/html'));assert.equal(result.code,'protocol_error');
  result=await probe({...remote('/slow'),check_timeout_sec:1});assert.equal(result.code,'timeout',JSON.stringify(result));
  result=await probe(remote('/sse-cross','sse'));assert.equal(result.code,'protocol_error');assert(result.message.includes('跨域'));
  const events=(await readFile(log,'utf8')).trim().split('\n').map(s=>JSON.parse(s));
  assert(events.some(e=>e.httpPath==='/mcp'&&e.session==='fixture-session'&&e.version==='2025-06-18'&&e.authorization==='present'),'HTTP 未携带协商协议、会话或授权');
  assert(events.some(e=>e.deleteSession==='fixture-session'),'检测结束未释放 HTTP 会话');
  assert(!events.some(e=>e.method==='tools/call'),'检测不应调用业务工具');
  checks.push('真实 HTTP、HTTP 事件流及旧版 SSE，携带授权/会话/协议头');
  checks.push('HTTP 401、普通网页与跨域 SSE 拒绝，不调用业务工具');
 }finally{http.kill();}
 return {fixture,local,probe,log,token};
}
