// 仅用于隔离验证；不访问网络外部资源，不提供业务写入工具。
import http from 'node:http';
import readline from 'node:readline';
import {writeFileSync,appendFileSync} from 'node:fs';
import {spawn} from 'node:child_process';
const mode=process.argv[2]||'stdio';const pidFile=process.argv[3];
if(pidFile)writeFileSync(pidFile,String(process.pid));
const log=(s)=>{if(process.env.MCP_FIXTURE_LOG)appendFileSync(process.env.MCP_FIXTURE_LOG,JSON.stringify(s)+'\n');};
function reply(message,kind='normal'){
 if(!message.method)return;
 log({method:message.method,id:message.id});if(!('id' in message))return;
 if(kind==='timeout')return;
 if(kind==='auth')return {jsonrpc:'2.0',id:message.id,error:{code:-32001,message:'unauthorized '+process.env.MCP_FIXTURE_TOKEN}};
 if(message.method==='initialize')return {jsonrpc:'2.0',id:message.id,result:{protocolVersion:kind==='bad-version'?'2099-01-01':message.params.protocolVersion,capabilities:kind==='no-capability'?{}:{tools:{listChanged:false}},serverInfo:{name:'Isolated MCP Fixture',version:'1.0.0'}}};
 if(message.method==='tools/list'){
  if(kind==='bad-tools')return {jsonrpc:'2.0',id:message.id,result:{tools:'broken'}};
  const tool=name=>({name,description:kind==='tool-secret'?'Fixture '+process.env.MCP_FIXTURE_TOKEN:'Fixture '+name,inputSchema:{type:'object',properties:{}}});
  return {jsonrpc:'2.0',id:message.id,result:{tools:kind==='empty'?[]:message.params?.cursor?[tool('second_tool')]:[tool('first_tool')],...(kind!=='empty'&&!message.params?.cursor?{nextCursor:'page-2'}:{})}};
 }
 return {jsonrpc:'2.0',id:message.id,result:{}};
}
if(mode.startsWith('stdio')){
 if(mode==='stdio-tree'){
  const child=spawn(process.execPath,['-e','setInterval(()=>{},1000)'],{stdio:'ignore',windowsHide:true});
  if(pidFile)writeFileSync(pidFile+'.child',String(child.pid));
 }
 if(mode==='stdio-exit'){process.stderr.write('failed '+process.env.MCP_FIXTURE_TOKEN);process.exit(2);}
 readline.createInterface({input:process.stdin}).on('line',line=>{let m;try{m=JSON.parse(line);}catch{return;}if(mode==='stdio-ping'&&m.method==='initialize')process.stdout.write(JSON.stringify({jsonrpc:'2.0',id:1,method:'ping'})+'\n');const result=reply(m,mode==='stdio-timeout'||mode==='stdio-tree'?'timeout':mode==='stdio-auth'?'auth':mode==='stdio-empty'?'empty':mode==='stdio-bad-tools'?'bad-tools':mode==='stdio-version'?'bad-version':mode==='stdio-tool-secret'?'tool-secret':'normal');if(result)process.stdout.write(JSON.stringify(result)+'\n');});
}else{
 const streams=new Map(),pingPending=new Map();let counter=0;
 const server=http.createServer((req,res)=>{
  const url=new URL(req.url,'http://127.0.0.1');
  if(url.pathname==='/auth'){res.writeHead(401);res.end('token secret should never be copied');return;}
  if(url.pathname==='/html'){res.writeHead(200,{'Content-Type':'text/html'});res.end('<html>ordinary web page</html>');return;}
  if(url.pathname==='/sse'&&req.method==='GET'){
   const id=String(++counter);res.writeHead(200,{'Content-Type':'text/event-stream','Cache-Control':'no-cache'});res.write('event: endpoint\ndata: /messages?id='+id+'\n\n');streams.set(id,res);res.on('close',()=>streams.delete(id));return;
  }
  if(url.pathname==='/sse-cross') {res.writeHead(200,{'Content-Type':'text/event-stream'});res.write('event: endpoint\ndata: https://example.com/messages\n\n');return;}
  if(req.method==='DELETE'){log({deleteSession:req.headers['mcp-session-id']});res.writeHead(204);res.end();return;}
  let body='';req.on('data',bytes=>body+=bytes);req.on('end',()=>{
   let message;try{message=JSON.parse(body);}catch{res.writeHead(400);res.end();return;}
   log({httpPath:url.pathname,session:req.headers['mcp-session-id'],version:req.headers['mcp-protocol-version'],authorization:req.headers.authorization?'present':'absent'});
   if(!message.method&&message.id==='fixture-ping'){res.writeHead(202);res.end();const pending=pingPending.get(message.id);if(pending){pending.res.end('data: '+JSON.stringify(pending.response)+'\n\n');pingPending.delete(message.id);}return;}
   const response=reply(message,url.pathname==='/slow'?'timeout':url.pathname==='/empty'?'empty':'normal');
   if(!response&&message.id){return;}
   if(url.pathname==='/messages'){
    res.writeHead(202);res.end();if(response)streams.get(url.searchParams.get('id'))?.write('event: message\ndata: '+JSON.stringify(response)+'\n\n');return;
   }
   if(!response){res.writeHead(202);res.end();return;}
   if(url.pathname==='/http-ping'&&message.method==='initialize'){res.writeHead(200,{'Content-Type':'text/event-stream','Mcp-Session-Id':'fixture-session'});res.write('data: '+JSON.stringify({jsonrpc:'2.0',id:'fixture-ping',method:'ping'})+'\n\n');pingPending.set('fixture-ping',{res,response});return;}
   res.writeHead(200,{'Content-Type':url.pathname==='/http-sse'?'text/event-stream':'application/json','Mcp-Session-Id':'fixture-session'});
   if(url.pathname==='/http-sse')res.end('event: message\ndata: '+JSON.stringify(response)+'\n\n');else res.end(JSON.stringify(response));
  });
 });
 server.listen(0,'127.0.0.1',()=>{console.log(JSON.stringify({port:server.address().port}));});
}
