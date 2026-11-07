// 在 verify-mcp 的独立 validation 应用内运行；不连接真实远端、不使用真实 Agent 配置。
import assert from "node:assert/strict";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { setTimeout as pause } from "node:timers/promises";

export async function verifyMaintenance({ invoke, root, checks, click, waitText, dom, setField, cdp, output }) {
  await invoke("save_skill_sync_settings", { settings: { repository: "fixture-user/fixture-skills", branch: "main" } });
  assert.deepEqual(await invoke("skill_sync_settings"), { repository: "fixture-user/fixture-skills", branch: "main" });
  await assert.rejects(invoke("save_skill_sync_settings", { settings: { repository: "https://token@github.com/a/b", branch: "main" } }));
  await invoke("save_update_settings", { settings: { check_on_start: false } });
  assert.equal((await invoke("update_settings")).check_on_start, false);
  await assert.rejects(invoke("open_update_link", { url: "https://example.invalid/download.exe" }));
  checks.push("GitHub 仓库设置持久化、拒绝带凭证 URL、自动更新开关持久化、阻止非官方下载地址");

  assert(await dom(() => { const button = document.querySelector('.nav-settings'); if (!button) return false; button.click(); return true; }));
  await waitText("数据与备份"); await click("数据与备份"); await waitText("GitHub 技能同步");
  assert.equal(await dom(() => document.querySelector('.sync-card input').value), "fixture-user/fixture-skills");
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('连接 / 登录'))));
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('推送技能'))));
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('拉取技能'))));
  const screenshot = await cdp("Page.captureScreenshot", { format: "png" });
  await writeFile(output.replace(/\.png$/, "-sharing.png"), Buffer.from(screenshot.data, "base64"));
  await click("通用设置"); await waitText("版本更新");
  assert(await dom(() => !![...document.querySelectorAll('button')].find(b => b.textContent.trim() === '检查新版本')));
  // 启动时读取独立 validation 缓存夹具，走真实查询与渲染链路，不请求网络。
  await waitText("发现新版本 v999.0.0");
  assert(await dom(() => !!document.querySelector('.update-banner')));
  assert(await dom(() => [...document.querySelectorAll('.update-banner button')].some(b => b.textContent.includes('下载更新'))));
  const updateScreenshot = await cdp("Page.captureScreenshot", { format: "png" });
  await writeFile(output.replace(/\.png$/, "-updates.png"), Buffer.from(updateScreenshot.data, "base64"));
  checks.push("设置中的技能推送 / 拉取及手动版本检查入口可见");
  checks.push("模拟新正式版本时显示全局通知、发布说明和下载更新按钮");

  const skills = path.join(root, "custom-agent-skills");
  await mkdir(path.join(skills, "example"), { recursive: true });
  await writeFile(path.join(skills, "example/SKILL.md"), "---\nname: example\ndescription: fixture-only skill\n---\n");
  const file = path.join(root, "custom-agent-mcp.json");
  const original = '{"theme":"fixture","mcpServers":{}}\n';
  await writeFile(file, original);
  await click("Agent 工具"); await waitText("＋ 自定义工具");
  for (let attempt = 0; attempt < 100; attempt++) {
    if (await dom(() => [...document.querySelectorAll('button')].some(b => b.textContent.trim() === '＋ 自定义工具' && !b.disabled))) break;
    await pause(100);
  }
  await click("＋ 自定义工具");
  await waitText("添加自定义工具");
  await setField('.custom-agent-modal input[placeholder="例如 My Agent"]', "路径验证工具");
  await setField('.custom-agent-modal input[placeholder="例如 my-agent"]', "fixture-custom-paths");
  await setField('.custom-agent-modal input[placeholder="技能目录的绝对路径"]', skills);
  await setField('.custom-agent-modal input[placeholder="例如 D:/MyAgent/mcp.json"]', file);
  const customScreenshot = await cdp("Page.captureScreenshot", { format: "png" });
  await writeFile(output.replace(/\.png$/, "-custom-agent.png"), Buffer.from(customScreenshot.data, "base64"));
  await click("确认并保存"); await waitText("自定义路径已保存");
  const inventory = await invoke("mcp_inventory");
  const target = inventory.targets.find(t => t.agent_id === "fixture-custom-paths");
  assert(target.installed && target.writable);
  assert.equal(target.binding.path, file); assert.equal(target.binding.format, "json");
  assert(target.reload_notice.includes("目标 Agent"));
  assert.equal(await readFile(file, "utf8"), original, "绑定操作不应部署 MCP 或修改原文件");
  checks.push("自定义 Agent 弹窗同时保存 Skills 和 MCP 路径，保持原配置内容，返回真实重载限制");
}
