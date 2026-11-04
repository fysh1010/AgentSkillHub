// Windows 桌面集成检查：Node 24 内置 WebSocket，无额外依赖。
// 测试二进制需以 TAURI_CONFIG 覆盖 identifier=com.agentskillhub.validation，
// 并通过 additionalBrowserArgs 设置调试端口 9237（新 WebView2 不接受提权进程的环境变量覆盖）。
// 使用临时目标文件与独立应用标识，不改动真实 Agent 配置。
import assert from "node:assert/strict";
import { mkdtemp, mkdir, writeFile, readFile } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { spawn } from "node:child_process";
import { setTimeout as pause } from "node:timers/promises";

const maintenanceOnly = process.argv.includes("--maintenance-only");
const args = process.argv.slice(2).filter(arg => !["--build", "--maintenance-only"].includes(arg));
const exe = path.resolve(args[0] || "src-tauri/target/debug/agentskillhub.exe");
const output = path.resolve(args[1] || path.join(tmpdir(), "agentskillhub-mcp-ui.png"));
const root = await mkdtemp(path.join(tmpdir(), "agentskillhub-mcp-test-"));
const home = path.join(root, "home");
const roaming = path.join(root, "roaming");
const codex = path.join(home, ".codex");
const app = path.join(process.env.APPDATA, "com.agentskillhub.validation");
const knownApp = path.join(home, "AppData", "Roaming", "com.agentskillhub.validation");
await Promise.all([mkdir(path.join(codex, "skills"), { recursive: true }), mkdir(app, { recursive: true }), mkdir(knownApp, { recursive: true }), mkdir(path.dirname(output), { recursive: true })]);
const configFile = path.join(codex, "config.toml");
const original = '# fixture comment\nmodel = "fixture-model"\n[mcp_servers.external]\ncommand = "do-not-run"\n';
await writeFile(configFile, original);
const fixtureConfig = JSON.stringify({
  skills_root: path.join(home, "library"),
  agents: [{ id: "mcp-fixture", name: "MCP 验证工具", skills_dir: path.join(codex, "skills"), builtin: true }, { id: "mcp-fixture-two", name: "MCP 验证工具 · 二", skills_dir: path.join(home,"second-agent","skills"), builtin: true }],
  agent_preferences: { "mcp-fixture": { enabled: true, executable_path: exe, manual_confirmed: true }, "mcp-fixture-two": { enabled: true, executable_path: exe, manual_confirmed: true } },
  agent_configs: { "mcp-fixture": { enabled_skills: [] } }, audit: [],
});
await Promise.all([writeFile(path.join(app, "config.json"), fixtureConfig), writeFile(path.join(knownApp, "config.json"), fixtureConfig)]);
await Promise.all([writeFile(path.join(app, "mcp-library.json"), "{}"), writeFile(path.join(knownApp, "mcp-library.json"), "{}")]);
await Promise.all([writeFile(path.join(app, "update-settings.json"), '{"check_on_start":true}'), writeFile(path.join(knownApp, "update-settings.json"), '{"check_on_start":true}')]);
const { version: fixtureVersion } = JSON.parse(await readFile("package.json", "utf8"));
const updateFixture = JSON.stringify({ current_version: fixtureVersion, latest_version: "v999.0.0", available: true,
  release_url: "https://github.com/fysh1010/AgentSkillHub/releases/tag/v999.0.0",
  download_url: "https://github.com/fysh1010/AgentSkillHub/releases/download/v999.0.0/fixture-portable.zip",
  notes: "Isolated update fixture", checked_at: Math.floor(Date.now() / 1000) });
await Promise.all([writeFile(path.join(app, "update-check.json"), updateFixture), writeFile(path.join(knownApp, "update-check.json"), updateFixture)]);
const port = 9237;
if (process.argv.includes("--build")) {
  const config = JSON.parse(await readFile("src-tauri/tauri.conf.json", "utf8"));
  const override = { identifier: "com.agentskillhub.validation", app: { windows: config.app.windows.map(w => ({ ...w, additionalBrowserArgs: `--remote-debugging-port=${port}`, dataDirectory: path.join(root, "webview") })) } };
  const build = spawn("cargo", ["build", "--manifest-path", "src-tauri/Cargo.toml"], { env: { ...process.env, CARGO_INCREMENTAL: "0", TAURI_CONFIG: JSON.stringify(override) }, stdio: "inherit", windowsHide: true });
  await new Promise((resolve, reject) => { build.once("error", reject); build.once("exit", code => code ? reject(new Error(`测试构建失败 ${code}`)) : resolve()); });
}
const childEnv = { ...process.env, MCP_TEST_EXE: exe };
const launcher = spawn("powershell.exe", ["-NoProfile", "-Command", 'Start-Process -FilePath $env:MCP_TEST_EXE -WindowStyle Hidden -PassThru | Select-Object -ExpandProperty Id'], { env: childEnv, windowsHide: true });
let pidText = "";
launcher.stdout.on("data", bytes => { pidText += bytes; });
await new Promise((resolve, reject) => { launcher.once("error", reject); launcher.once("exit", code => code ? reject(new Error(`启动失败 ${code}`)) : resolve()); });
const appPid = Number(pidText.trim());
assert(appPid > 0, "缺少测试进程 PID");
let socket;
const checks = [];
try {
  let page;
  let lastStatus = "";
  for (let attempt = 0; attempt < 40; attempt++) {
    try { const pages = await (await fetch(`http://127.0.0.1:${port}/json/list`, { signal: AbortSignal.timeout(1000) })).json(); lastStatus = JSON.stringify(pages.map(p => ({ type: p.type, hasSocket: !!p.webSocketDebuggerUrl }))); page = pages.find(p => p.webSocketDebuggerUrl && p.type !== "service_worker"); } catch (e) { lastStatus = String(e); }
    if (page) break;
    await pause(300);
  }
  assert(page, `WebView2 调试页面未就绪：${lastStatus}`);
  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => { socket.onopen = resolve; socket.onerror = reject; });
  let seq = 0;
  const pending = new Map();
  socket.onmessage = event => {
    const result = JSON.parse(event.data);
    if (!result.id) return;
    const request = pending.get(result.id); if (!request) return;
    pending.delete(result.id);
    if (result.error) request.reject(new Error(result.error.message)); else request.resolve(result.result);
  };
  const cdp = (method, params = {}) => new Promise((resolve, reject) => { const id = ++seq; pending.set(id, { resolve, reject }); socket.send(JSON.stringify({ id, method, params })); });
  const evaluate = async expression => {
    const r = await cdp("Runtime.evaluate", { expression, awaitPromise: true, returnByValue: true });
    if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description || r.exceptionDetails.exception?.value || r.exceptionDetails.text);
    return r.result?.value;
  };
  const invoke = (command, args = {}) => evaluate(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(command)},${JSON.stringify(args)})`);
  for(let i=0;i<100;i++){if(await evaluate('typeof window.__TAURI_INTERNALS__?.invoke === "function"'))break;await pause(100);}
  const {verifyProbeBackend}=await import("./verify-mcp-probe.mjs");
  const probeFixtures=maintenanceOnly ? null : await verifyProbeBackend({invoke,root,checks});
  const click = async text => {
    const ok = await evaluate(`(()=>{const b=[...document.querySelectorAll('button')].find(b=>(b.textContent.trim()=== ${JSON.stringify(text)} || b.getAttribute("aria-label")===${JSON.stringify(text)})); if(!b || b.disabled)return false; b.click();return true})()`);
    assert(ok, `按钮不可点击：${text}`); await pause(160);
  };
  const waitText = async text => {
    for (let i = 0; i < 160; i++) { if (await evaluate(`document.body.innerText.includes(${JSON.stringify(text)})`)) return; await pause(200); }
    throw new Error(`等待界面文字超时：${text}\n`+await evaluate('document.body.innerText'));
  };
  const dom = (fn, ...args) => evaluate("(" + fn.toString() + ")(" + args.map(a=>JSON.stringify(a)).join(",") + ")");
  const applyChanges=async()=>{await waitText("确认应用变更");await click("确认应用变更");for(let i=0;i<100;i++){if(!await dom(()=>!!document.querySelector("#preview-title")))return;await pause(100);}throw new Error("配置未完成："+await dom(()=>document.querySelector(".workspace-dialog")?.innerText));};
  const setField = async (selector,value) => {assert(await dom((selector,value)=>{const el=document.querySelector(selector);if(!el)return false;el.value=value;el.dispatchEvent(new Event('input',{bubbles:true}));return true;},selector,value),"找不到字段 "+selector);await pause(50);};
  const clickAgent = async name => {assert(await dom(name=>{const b=[...document.querySelectorAll('.agent-sidebar>button')].find(b=>b.textContent.includes(name));if(!b)return false;b.click();return true;},name));await pause(100);};
  if (!maintenanceOnly) {
  await waitText("MCP 管理");
  const sync = await invoke("get_sync_data");
  assert.equal(sync.config.skills_root,path.join(home,"library"),"未使用隔离配置，禁止写入");
  await invoke("refresh_discovery");
  await invoke("mcp_save_binding",{binding:{agent_id:"mcp-fixture",path:configFile,format:"codex"}});
  const secondDir=path.join(home,"second-agent");
  await mkdir(secondDir,{recursive:true});
  const secondFile=path.join(secondDir,"opencode.jsonc");
  const secondOriginal='// fixture comment\n{"theme":"dark","mcp":{}}\n';
  await writeFile(secondFile,secondOriginal);
  await invoke("mcp_save_binding",{binding:{agent_id:"mcp-fixture-two",path:secondFile,format:"opencode"}});
  await click("MCP 管理");
  await waitText("MCP 集成");
  await waitText("external");
  const inventory=await invoke("mcp_inventory");
  assert(inventory.guides.length>=64,"应逐项提供内置 Agent 的配置核查记录");
  const builtinNames=inventory.sources.filter(s=>s.kind==="builtin").flatMap(s=>s.servers.map(s=>s.name));
  assert(!await dom(()=>!!document.querySelector('.source-block')),"默认页面不能显示内置来源列表");
  assert(!await dom(names=>[...document.querySelectorAll('.service-row h3')].some(h=>names.includes(h.childNodes[0]?.textContent.trim())),builtinNames),"内置服务不能混入我的 MCP");
  assert.equal(await readFile(configFile,"utf8"),original);
  checks.push("默认仅展示用户服务，内置来源隔离，逐项配置指南");

  await click("按 Agent");await clickAgent("MCP 验证工具");
  await click("暂停 external");await waitText("暂停此 Agent 中的服务");
  assert.equal(await readFile(configFile,"utf8"),original,"暂停预览不得改配置");
  await applyChanges();await waitText("已暂停 · 定义已保留");
  assert(!(await readFile(configFile,"utf8")).includes("do-not-run"));
  let inv=await invoke("mcp_inventory");
  assert(inv.sources.some(s=>s.kind==="paused"&&s.agent_id==="mcp-fixture"&&s.servers.some(s=>s.name==="external")));
  await click("启用 external");await applyChanges();
  await waitText("本地进程 · do-not-run");
  let baseline=await readFile(configFile,"utf8");
  assert(baseline.includes("# fixture comment")&&baseline.includes("fixture-model")&&baseline.includes("do-not-run"));
  checks.push("用户自配服务暂停、定义保留、重新启用，预览不写入");

  assert(await dom(()=>{const row=[...document.querySelectorAll('.agent-service')].find(r=>r.textContent.includes('external'));[...row.querySelectorAll('button')].find(b=>b.textContent==='编辑').click();return true;}));
  await waitText("命令与参数写在一起");
  await setField("#mcp-command",'fixture-edited-command --fixture "value with spaces"');
  await click("预览修改");await waitText("确认修改服务配置");
  assert.equal(await readFile(configFile,"utf8"),baseline);
  await applyChanges();await waitText("本地进程 · fixture-edited-command");
  assert((await readFile(configFile,"utf8")).includes("value with spaces"));
  await dom(()=>document.querySelector('.config-details').open=true);
  await click("恢复上次修改");await click("确认");
  await waitText("本地进程 · do-not-run");
  assert.equal(await readFile(configFile,"utf8"),baseline);
  checks.push("表单编辑原生用户服务与恢复原始字节");

  await click("＋ 添加 MCP");await waitText("命令与参数写在一起");
  await setField("#mcp-name","fixture-service");await setField("#mcp-command",'fixture-never-executed --test "D:/path with spaces"');
  await dom(async()=>{for(const row of document.querySelectorAll('.mcp-editor .agent-chip'))if(row.textContent.includes('MCP 验证工具')&&!row.classList.contains('selected')){row.click();await new Promise(r=>setTimeout(r,30));}});
  await click("保存");await waitText("确认分配与启用");
  assert.equal(await readFile(configFile,"utf8"),baseline);assert.equal(await readFile(secondFile,"utf8"),secondOriginal);
  assert.equal(await dom(()=>document.querySelectorAll('.preview-agent').length),2,"应同时预览两个 Agent");
  await applyChanges();

  let deployed=await readFile(configFile,"utf8");
  let second=await readFile(secondFile,"utf8");
  assert(deployed.includes("fixture-never-executed")&&deployed.includes("do-not-run")&&deployed.includes("# fixture comment"));
  assert(second.includes("// fixture comment")&&second.includes('"theme":"dark"'));
  const secondDoc=JSON.parse(second.replace("// fixture comment",""));
  assert.deepEqual(secondDoc.mcp["fixture-service"].command,["fixture-never-executed","--test","D:/path with spaces"]);
  checks.push("一份定义分配两个 Agent，TOML 与 OpenCode JSONC 自动转换");

  await click("我的 MCP");
  assert(await dom(()=>{const row=[...document.querySelectorAll('.service-row')].find(r=>r.querySelector('h3')?.textContent.startsWith('fixture-service'));[...row.querySelectorAll('button')].find(b=>b.textContent==='编辑').click();return true;}));
  await setField("#mcp-command",'fixture-command-v2 --test "D:/path with spaces"');
  await click("保存");await waitText("确认分配与启用");
  await applyChanges();
  assert((await readFile(configFile,"utf8")).includes("fixture-command-v2"));
  assert((await readFile(secondFile,"utf8")).includes("fixture-command-v2"));
  checks.push("服务库编辑后同步到所有已分配 Agent");

  await click("按 Agent");await clickAgent("MCP 验证工具");
  await click("暂停 fixture-service");await applyChanges();await waitText("已暂停 · 定义已保留");
  await click("启用 fixture-service");await applyChanges();
  await waitText("本地进程 · fixture-command-v2");
  inv=await invoke("mcp_inventory");
  assert(inv.targets.find(t=>t.agent_id==="mcp-fixture").managed.includes("fixture-service"),"暂停再启用应保留管理归属");
  checks.push("已分配服务暂停再启用仍保留归属");

  const rejectCall=async(command,args,expected)=>{let failure="";try{await invoke(command,args);}catch(e){failure=String(e);}assert(failure.includes(expected),command+" 应拒绝 "+expected+"，实际 "+failure);};
  const directSource=inv.sources.find(s=>s.agent_id==="mcp-fixture"&&s.kind==="user");
  const external=directSource.servers.find(s=>s.name==="external");
  const changeArgs={agentId:"mcp-fixture",path:configFile,format:"codex",name:"external",operation:"pause",config:null,expected:external.config};
  const preview=await invoke("mcp_preview_change",{request:changeArgs});
  deployed=await readFile(configFile,"utf8");
  await writeFile(configFile,deployed+"\n# external edit\n");
  await rejectCall("mcp_apply",{ticket:preview.ticket},"配置已变化");
  await writeFile(configFile,deployed);
  const libPreview=await invoke("mcp_preview",{agentId:"mcp-fixture",names:["fixture-service"]});
  await invoke("mcp_save_server",{server:{name:"fixture-service",config:{command:"fixture-command-v3"}}});
  await rejectCall("mcp_apply",{ticket:libPreview.ticket},"统一库已变化");
  await invoke("mcp_save_server",{server:{name:"fixture-service",config:{command:"fixture-command-v2",args:["--test","D:/path with spaces"]}}});
  await invoke("set_agent_management",{agentId:"mcp-fixture",enabled:false});
  await rejectCall("mcp_preview_change",{request:changeArgs},"关闭配置");
  await invoke("set_agent_management",{agentId:"mcp-fixture",enabled:true});
  checks.push("文件外部变化、库变化及关闭管理时的写入保护");

  // 完整 JSON 导入，不要求用户拆开各服务再粘贴。
  await click("＋ 添加 MCP");await click("JSON 导入 / 编辑");
  await setField("#mcp-config",JSON.stringify({mcpServers:{"fixture-http":{type:"http",url:"https://example.com/mcp",headers:{Authorization:"Bearer fixture-token"}},"fixture-extra":{command:"fixture-extra-never-run"}}}));
  await dom(()=>{const c=document.querySelector(".auto-check input");if(c.checked)c.click();});
  await click("保存");await click("我的 MCP");await waitText("fixture-http");await waitText("fixture-extra");
  assert(!await dom(()=>document.body.innerText.includes('Bearer fixture-token')),"主列表不应显示凭证");
  checks.push("完整 mcpServers JSON 批量导入和默认凭证隐藏");

  await click("发现与导入");await waitText("发现本机配置");
  assert(!await dom(()=>!!document.querySelector('.discovery-switch')), "不应保留插件深扫入口");
  await waitText("不深搜插件与缓存");
  await click("配置指南");await waitText("核查 2026-10-25");
  assert(await dom(()=>document.querySelectorAll('.guide-grid article').length>=64));
  await click("我的 MCP");
  await evaluate("document.querySelectorAll('.toast-close').forEach(b=>b.click())");
  await pause(150);
  let screenshot=await cdp("Page.captureScreenshot",{format:"png"});await writeFile(output,Buffer.from(screenshot.data,"base64"));
  await cdp("Emulation.setDeviceMetricsOverride",{width:1080,height:760,deviceScaleFactor:1,mobile:false});
  assert(await dom(()=>document.documentElement.scrollWidth<=window.innerWidth+1),"窄窗口出现横向溢出");
  await dom(()=>{const s=document.querySelector('select[aria-label="界面配色"]');s.value="mocha";s.dispatchEvent(new Event("change",{bubbles:true}));});await pause(120);
  const darkOutput=output.replace(/\.png$/,"-dark-1080.png");screenshot=await cdp("Page.captureScreenshot",{format:"png"});await writeFile(darkOutput,Buffer.from(screenshot.data,"base64"));
  await click("按 Agent");await clickAgent("MCP 验证工具");
  assert(await dom(()=>document.documentElement.scrollWidth<=window.innerWidth+1),"Agent 视图出现横向溢出");
  screenshot=await cdp("Page.captureScreenshot",{format:"png"});await writeFile(output.replace(/\.png$/,"-agent.png"),Buffer.from(screenshot.data,"base64"));
  await click("＋ 添加 MCP");await waitText("命令与参数写在一起");
  assert(await dom(()=>{const r=document.querySelector(".mcp-editor").getBoundingClientRect();return r.left>=0&&r.right<=innerWidth&&r.bottom<=innerHeight;}),"编辑器在窄窗口超出可视区域");
  await click("关闭编辑器");
  await dom(()=>{const s=document.querySelector('select[aria-label="界面配色"]');s.value="monochrome";s.dispatchEvent(new Event("change",{bubbles:true}));});
  await cdp("Emulation.clearDeviceMetricsOverride");
  checks.push("明确用户配置发现、无插件深扫入口、配置指南与主界面视觉检查");

  const registry=await invoke("mcp_market_search",{query:"filesystem",cursor:null});
  assert(registry.servers.length,"官方市场应返回真实搜索结果");
  await click("MCP 市场");await waitText("配置安装 →");
  await setField(".searchbar input","io.github.Digital-Defiance/mcp-filesystem");await click("搜索市场");await waitText("io.github.Digital-Defiance/mcp-filesystem");
  await click("配置安装 →");await waitText("WORKSPACE_ROOT");await click("选择 Agent →");await waitText("请填写 WORKSPACE_ROOT");
  await setField(".dialog input",root);await click("选择 Agent →");await waitText("命令与参数写在一起");
  await setField("#mcp-name","fixture-market");
  await dom(()=>{const c=document.querySelector(".auto-check input");if(c.checked)c.click();const chips=[...document.querySelectorAll(".mcp-editor .agent-chip")];for(const c of chips)if(c.classList.contains("selected"))c.click();});await pause(80);
  await dom(()=>{const row=[...document.querySelectorAll(".mcp-editor .agent-chip")].find(r=>r.textContent.includes("MCP 验证工具 · 二"));row.click();});
  await click("保存");await applyChanges();

  second=JSON.parse((await readFile(secondFile,"utf8")).replace("// fixture comment",""));
  assert.equal(second.mcp["fixture-market"].command[0],"npx");
  assert.equal(second.mcp["fixture-market"].environment.WORKSPACE_ROOT,root);
  checks.push("真实市场搜索、必填校验、参数表单与安装到指定 Agent");

  // 暂停后从库中移除定义，再恢复为原 Agent 的独立用户服务，不能留下失效的库引用。
  inv=await invoke("mcp_inventory");
  let marketSource=inv.sources.find(s=>s.agent_id==='mcp-fixture-two'&&s.kind==='user');
  let marketServer=marketSource.servers.find(s=>s.name==='fixture-market');
  let marketChange={agentId:marketSource.agent_id,path:marketSource.path,format:marketSource.format,name:marketServer.name,operation:'pause',config:null,expected:marketServer.config,kind:'user'};
  let marketPlan=await invoke('mcp_preview_change',{request:marketChange});await invoke('mcp_apply',{ticket:marketPlan.ticket});
  await invoke('mcp_remove_server',{name:'fixture-market'});
  inv=await invoke('mcp_inventory');marketSource=inv.sources.find(s=>s.agent_id==='mcp-fixture-two'&&s.kind==='paused');marketServer=marketSource.servers.find(s=>s.name==='fixture-market');
  marketPlan=await invoke('mcp_preview_change',{request:{...marketChange,operation:'resume',kind:'paused',expected:marketServer.config}});await invoke('mcp_apply',{ticket:marketPlan.ticket});
  inv=await invoke('mcp_inventory');
  assert(!inv.targets.find(t=>t.agent_id==='mcp-fixture-two').managed.includes('fixture-market'));
  assert(inv.sources.some(s=>s.kind==='user'&&s.agent_id==='mcp-fixture-two'&&s.servers.some(s=>s.name==='fixture-market')));
  checks.push('暂停后移除库定义再恢复，不残留失效分配');

  // 新弹窗完成参数、Agent 选择、保存、真实自动检测，无需去其他客户端验证握手。
  await click('我的 MCP');await click('＋ 添加 MCP');
  await setField('#mcp-name','fixture-auto-check');
  const {joinMcpCommand}=await import('../src/utils/mcpCommand.ts');
  await setField('#mcp-command',joinMcpCommand(process.execPath,[probeFixtures.fixture,'stdio',path.join(root,'auto-check.pid')]));
  await setField('#mcp-timeout','3');await click('＋ 添加');
  await setField('input[aria-label="参数 1 名称"]','MCP_FIXTURE_TOKEN');await setField('input[aria-label="参数 1 值"]','fixture-ui-private-token');
  await dom(()=>{const chip=[...document.querySelectorAll('.mcp-editor .agent-chip')].find(c=>c.textContent.includes('MCP 验证工具 · 二'));if(!chip.classList.contains('selected'))chip.click();});
  await click('保存');await applyChanges();await click('我的 MCP');
  for(let i=0;i<120;i++){if(await dom(()=>[...document.querySelectorAll('.service-row')].some(r=>r.querySelector('h3')?.textContent.startsWith('fixture-auto-check')&&r.querySelector('.mcp-check.ok'))))break;await pause(100);}
  assert(await dom(()=>[...document.querySelectorAll('.service-row')].some(r=>r.querySelector('h3')?.textContent.startsWith('fixture-auto-check')&&r.textContent.includes('已发现 2 个工具'))),'保存后未自动完成真实检测');
  assert(!await dom(()=>document.body.innerText.includes('fixture-ui-private-token')));
  await dom(()=>{const row=[...document.querySelectorAll('.service-row')].find(r=>r.querySelector('h3')?.textContent.startsWith('fixture-auto-check'));[...row.querySelectorAll('button')].find(b=>b.textContent==='编辑').click();});
  assert.equal(await dom(()=>document.querySelector('#mcp-timeout').value),'3','检测超时设置未保存');
  assert(await dom(()=>document.querySelector('.mcp-editor .agent-chip.selected')?.textContent.includes('MCP 验证工具 · 二')),'Agent 选择未保留');
  await evaluate("document.querySelectorAll('.toast-close').forEach(b=>b.click())");await pause(120);
  screenshot=await cdp('Page.captureScreenshot',{format:'png'});await writeFile(output.replace(/\.png$/,'-editor.png'),Buffer.from(screenshot.data,'base64'));
  await dom(()=>document.querySelector('.mcp-editor .check-message').click());await waitText('first_tool');await waitText('second_tool');
  screenshot=await cdp('Page.captureScreenshot',{format:'png'});await writeFile(output.replace(/\.png$/,'-tools.png'),Buffer.from(screenshot.data,'base64'));
  await click('关闭检测结果');
  await click('关闭编辑器');
  checks.push('截图式配置弹窗、完整引号命令、凭证遮蔽、Agent 选择与保存后自动检测');
  checks.push('检测超时设置持久化，以及在应用内查看已发现工具');

  assert.equal(await readFile(configFile,"utf8"),deployed);
  }
  const { verifyMaintenance } = await import("./verify-maintenance.mjs");
  await verifyMaintenance({ invoke, root, checks, click, waitText, dom, setField, cdp, output });

  // 所有写入均限于独立 validation 配置与临时夹具。
  console.log(JSON.stringify({passed:checks.length,checks,screenshot:output,fixture:root},null,2));
} finally {
  socket?.close();
  try { process.kill(appPid); } catch { /* 测试进程已退出 */ }
}
