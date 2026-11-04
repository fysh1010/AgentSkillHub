<script setup lang="ts">
import { onMounted, reactive, ref } from "vue";
import * as api from "../api/maintenance";
import { loadAll, store, toast } from "../store";

const settings = reactive<api.SkillSyncSettings>({ repository: "", branch: "main" });
const busy = ref(false);
const ready = ref(false);
const message = ref("");
const failed = ref(false);
onMounted(async () => {
  try { Object.assign(settings, await api.skillSyncSettings()); ready.value = true; }
  catch (e) { message.value = String(e); failed.value = true; }
});
async function save() {
  await api.saveSkillSyncSettings({ repository: settings.repository.trim(), branch: settings.branch.trim() });
}
async function saveSettings() {
  busy.value = true; failed.value = false;
  try { await save(); message.value = "仓库设置已保存。两台电脑使用同一仓库和分支即可共享技能。"; }
  catch (e) { message.value = String(e); failed.value = true; }
  finally { busy.value = false; }
}
async function sync(operation: "push" | "pull") {
  const warning = operation === "push"
    ? `将把技能库 ${store.config?.skills_root} 中的技能提交并推送到 ${settings.repository}（${settings.branch}）。已有 Git 历史也会一并推送，请确认历史、技能正文及脚本中没有密钥或私人资料；目录名过滤无法识别正文中的秘密。是否继续？`
    : `将从 ${settings.repository}（${settings.branch}）拉取技能到 ${store.config?.skills_root}。仅允许 Git 快进更新；有本地修改或分叉时停止。新技能不会自动分配给 Agent，是否继续？`;
  if (!confirm(warning)) return;
  busy.value = true; failed.value = false; message.value = "";
  try {
    await save(); message.value = await api.skillSyncRun(operation);
    await loadAll(); toast(message.value, "success");
  } catch (e) { message.value = String(e); failed.value = true; }
  finally { busy.value = false; }
}
</script>

<template>
  <section class="card sync-card">
    <h3>GitHub 技能同步</h3>
    <p>指定自己的专用技能仓库，在多台电脑间手动推送或拉取。使用本机 Git 凭证管理器登录；本软件不保存密码或令牌。</p>
    <label>GitHub 仓库<input v-model="settings.repository" class="input" placeholder="owner/my-skills 或 https://github.com/owner/my-skills" :disabled="busy || !ready" /></label>
    <label>分支<input v-model="settings.branch" class="input" placeholder="main" :disabled="busy || !ready" /></label>
    <p>请先在 GitHub 创建仓库。新电脑使用空技能库先拉取；切换电脑前先推送，另一台先拉取再安装新技能。技能库必须是独立目录，现有 origin 与分支必须匹配。</p>
    <p>只同步标准技能目录；跳过隐藏文件、回收站、依赖、缓存及常见密钥文件。MCP 密钥、Agent 绝对路径和应用设置保留在各电脑本机。技能中的私人内容仍需自行检查；建议使用私有仓库。</p>
    <div class="sync-actions">
      <button class="btn" :disabled="busy || !ready" @click="saveSettings">保存仓库</button>
      <button class="btn primary" :disabled="busy || !ready" @click="sync('push')">推送技能 ↑</button>
      <button class="btn" :disabled="busy || !ready" @click="sync('pull')">拉取技能 ↓</button>
      <span v-if="busy">正在处理，请勿关闭应用…</span>
    </div>
    <p v-if="message" class="sync-result" :class="{ failed }" role="status">{{ message }}</p>
  </section>
</template>

<style scoped>
.sync-card { margin-bottom:18px; }.sync-card h3 { font-size:15px; }.sync-card p { font-size:12px; line-height:1.8; color:var(--muted); margin:10px 0; }.sync-card label { display:block; font-size:12px; margin:12px 0; }.sync-card input { margin-top:6px; }.sync-actions { display:flex; align-items:center; flex-wrap:wrap; gap:10px; }.sync-actions span { font-size:12px; color:var(--muted); }.sync-result { white-space:pre-wrap; overflow-wrap:anywhere; }.sync-result.failed { color:var(--red); }
</style>
