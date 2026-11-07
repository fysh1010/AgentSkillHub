// 在 verify-mcp 的独立 validation 应用内运行；不连接真实远端、不使用真实 Agent 配置。
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { setTimeout as pause } from "node:timers/promises";

export async function verifyMaintenance({ invoke, root, checks, click, waitText, dom, setField, cdp, output }) {
  // 等启动检查读完 validation 缓存，再测试关闭开关，避免与应用初始化竞争。
  await waitText("发现新版本 v999.0.0");
  await invoke("save_skill_sync_settings", { settings: { repository: "fixture-user/fixture-skills", branch: "main" } });
  assert.deepEqual(await invoke("skill_sync_settings"), { repository: "fixture-user/fixture-skills", branch: "main", whole_repository: false });
  await assert.rejects(invoke("save_skill_sync_settings", { settings: { repository: "https://token@github.com/a/b", branch: "main" } }));
  await invoke("save_update_settings", { settings: { check_on_start: false } });
  assert.equal((await invoke("update_settings")).check_on_start, false);
  await assert.rejects(invoke("open_update_link", { url: "https://example.invalid/download.exe" }));
  checks.push("GitHub 仓库设置持久化、拒绝带凭证 URL、自动更新开关持久化、阻止非官方下载地址");

  assert(await dom(() => { const button = document.querySelector('.nav-settings'); if (!button) return false; button.click(); return true; }));
  await waitText("数据与备份"); await click("数据与备份"); await waitText("Git 仓库同步");
  await waitText("加密技能备份");
  const library = path.join(root, "home", "library");
  assert.equal((await invoke("get_sync_data")).config.skills_root, library);
  const skillPath = path.join(library, "fixture-backup");
  const backupPath = path.join(root, "backups");
  await Promise.all([mkdir(skillPath, { recursive: true }), mkdir(backupPath, { recursive: true })]);
  await writeFile(path.join(skillPath, "SKILL.md"), "backup fixture");
  if (!await dom(() => !!document.querySelector('#backup-location') && document.querySelector('.skill-backup button[aria-pressed="true"]').textContent.includes("本地"))) await click("本地 / 网盘文件夹");
  await setField("#backup-location", backupPath);
  await setField(".skill-backup input[type=password]", "desktop-fixture-passphrase");
  await click("创建加密备份");
  await waitText("已备份");
  for (let i = 0; i < 80; i++) {
    if (await dom(() => [...document.querySelectorAll('.skill-backup button')].some(b => b.textContent.includes('恢复到新目录') && !b.disabled))) break;
    await pause(100);
  }
  await click("恢复到新目录");
  await waitText("当前技能库未被覆盖");
  const restoredPath = await dom(() => document.querySelector('.skill-backup .restored p').textContent);
  assert.equal(await readFile(path.join(restoredPath, "fixture-backup", "SKILL.md"), "utf8"), "backup fixture");
  assert.equal((await invoke("get_sync_data")).config.skills_root, library);
  checks.push("桌面创建加密技能快照并恢复到新目录，保持原技能库");
  assert.equal(await dom(() => document.querySelector('.sync-card input').value), "fixture-user/fixture-skills");
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('连接 / 登录'))));
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('推送技能'))));
  assert(await dom(() => [...document.querySelectorAll('.sync-card button')].some(b => b.textContent.includes('拉取技能'))));
  assert(await dom(() => { const input = document.querySelector('.sync-mode input'); if (!input) return false; input.click(); return true; }));
  await waitText("推送仓库");
  await click("保存仓库");
  await waitText("仓库设置已保存");
  assert.equal((await invoke("skill_sync_settings")).whole_repository, true);
  // 用本地分支不匹配触发真实后端错误，在任何网络请求前停止。
  execFileSync("git", ["init", "-b", "fixture-local"], { cwd: library, windowsHide: true });
  assert(await dom(() => {
    window.confirm = () => true;
    window.sawGitProgress = false;
    const card = document.querySelector(".sync-card");
    const observer = new MutationObserver(() => {
      if (card.querySelector("progress") && card.textContent.includes("第 1 / 6 步")) {
        window.sawGitProgress = true;
        observer.disconnect();
      }
    });
    observer.observe(card, { childList: true, subtree: true, characterData: true });
    return true;
  }));
  await click("推送仓库 ↑");
  await waitText("检查本地仓库与文件失败");
  assert(await dom(() => window.sawGitProgress));
  assert(await dom(() => !document.querySelector(".sync-card button.primary").disabled));
  checks.push("Git 推送显示真实后端进度；本地分支不匹配时显示失败步骤并恢复按钮");
  const screenshot = await cdp("Page.captureScreenshot", { format: "png" });
  await writeFile(output.replace(/\.png$/, "-sharing.png"), Buffer.from(screenshot.data, "base64"));
  await click("通用设置"); await waitText("版本更新");
  assert(await dom(() => !![...document.querySelectorAll('button')].find(b => b.textContent.trim() === '检查新版本')));
  // 启动时读取独立 validation 缓存夹具，走真实查询与渲染链路，不请求网络。
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
