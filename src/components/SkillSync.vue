<script lang="ts">
import { computed, onMounted, reactive, ref } from "vue";
import * as api from "../api/maintenance";
import { loadAll, store, toast } from "../store";

// 保留同步状态，切换设置页签再回来也能看到当前阶段和最终结果。
const busy = ref(false);
const message = ref("");
const failed = ref(false);
const progress = ref<api.SkillSyncProgress>();
const phase = ref("");
const elapsed = ref(0);
</script>

<script setup lang="ts">
const settings = reactive<api.SkillSyncSettings>({ repository: "", branch: "main", whole_repository: false });
const ready = ref(false);
const statusLabel = computed(() => progress.value?.label ?? phase.value);
onMounted(async () => {
  try { Object.assign(settings, await api.skillSyncSettings()); ready.value = true; }
  catch (e) { message.value = String(e); failed.value = true; }
});
async function run(label: string, action: () => Promise<void>) {
  busy.value = true;
  failed.value = false;
  message.value = "";
  progress.value = undefined;
  phase.value = label;
  elapsed.value = 0;
  const start = performance.now();
  const tick = () => { elapsed.value = Math.floor((performance.now() - start) / 1000); };
  const timer = window.setInterval(tick, 1000);
  try { await action(); }
  catch (e) {
    message.value = `${statusLabel.value}失败：${String(e)}`;
    failed.value = true;
    toast(message.value, "error");
  } finally {
    window.clearInterval(timer);
    tick();
    busy.value = false;
  }
}
async function save() {
  await api.saveSkillSyncSettings({ ...settings, repository: settings.repository.trim(), branch: settings.branch.trim() });
}
async function saveSettings() {
  await run("保存仓库设置", async () => {
    await save();
    message.value = "仓库设置已保存。两台电脑使用同一仓库和分支即可共享技能。";
  });
}
async function detect(repository?: string) {
  await run(repository ? "读取远端分支" : "读取本地仓库", async () => {
    Object.assign(settings, await api.detectSkillGit(repository));
    message.value = `已读取分支 ${settings.branch}，可以直接推送或拉取。`;
  });
}
async function sync(operation: "connect" | "push" | "pull") {
  if (operation !== "connect") {
    const target = `${settings.repository}（${settings.branch}）`;
    const warning = settings.whole_repository
      ? operation === "push"
        ? `将按 Git 规则把 ${store.config?.skills_root} 的仓库改动推送到 ${target}。已跟踪的本机配置也会同步，请确认仓库访问权限和待推送内容。是否继续？`
        : `将从 ${target} 拉取整个仓库到 ${store.config?.skills_root}，已跟踪的配置也会随远端更新。遇到文件冲突或分支分叉时停止。是否继续？`
      : operation === "push"
        ? `将把 ${store.config?.skills_root} 中的技能提交并推送到 ${target}。请确认技能文件及 Git 历史中没有密钥或私人资料。是否继续？`
        : `将从 ${target} 拉取技能到 ${store.config?.skills_root}。遇到文件冲突或分支分叉时停止。是否继续？`;
    if (!confirm(warning)) return;
  }
  await run("保存仓库设置", async () => {
    await save();
    message.value = await api.skillSyncRun(operation, (update) => { progress.value = update; });
    toast(message.value, "success");
    if (operation !== "connect") void loadAll();
  });
}
</script>

<template>
  <section class="card sync-card">
    <h3>Git 仓库同步</h3>
    <p>支持 GitHub、GitLab、Gitea 和其他 HTTPS / SSH Git 服务器。已有仓库可以直接读取地址和分支；新电脑只需填写相同的仓库地址。</p>
    <label>仓库地址<input v-model="settings.repository" class="input" placeholder="owner/my-skills 或 HTTPS / SSH 地址" :disabled="busy || !ready" /></label>
    <div class="sync-actions">
      <button class="btn" :disabled="busy || !ready" @click="detect()">读取本地仓库</button>
      <button class="btn" :disabled="busy || !ready || !settings.repository.trim()" @click="detect(settings.repository)">读取远端分支</button>
    </div>
    <label>分支<input v-model="settings.branch" class="input" placeholder="main" :disabled="busy || !ready" /></label>
    <label class="sync-mode"><input v-model="settings.whole_repository" type="checkbox" :disabled="busy || !ready" />按 Git 同步整个仓库</label>
    <p v-if="settings.whole_repository">适合自己的私有仓库。使用现有 Git 忽略规则；已跟踪的配置照常同步，工具不再过滤文件。推送前会先拉取能安全应用的远端更新，遇到冲突或分叉会停止。</p>
    <p v-else>仅同步技能并检查常见凭证。已有仓库如果跟踪了其他文件，可以勾选「按 Git 同步整个仓库」继续使用。</p>
    <div class="sync-actions">
      <button class="btn" :disabled="busy || !ready" @click="saveSettings">保存仓库</button>
      <button class="btn" :disabled="busy || !ready || !settings.repository.trim()" @click="sync('connect')">连接 / 登录</button>
      <button class="btn primary" :disabled="busy || !ready || !settings.repository.trim()" @click="sync('push')">{{ settings.whole_repository ? '推送仓库' : '推送技能' }} ↑</button>
      <button class="btn" :disabled="busy || !ready || !settings.repository.trim()" @click="sync('pull')">{{ settings.whole_repository ? '拉取仓库' : '拉取技能' }} ↓</button>
    </div>
    <div v-if="busy" class="sync-progress">
      <div class="sync-progress-label">
        <span role="status">{{ progress ? `第 ${progress.step} / ${progress.total} 步 · ` : "" }}{{ statusLabel }}…</span>
        <span>已用 {{ elapsed }} 秒</span>
      </div>
      <progress v-if="progress && progress.total > 1" :value="progress.step - 1" :max="progress.total" aria-label="已完成的同步步骤" />
      <progress v-else aria-label="正在处理" />
      <p v-if="elapsed >= 15">仍在等待当前步骤完成。Git 单条命令等待超过 90 秒会报超时。</p>
    </div>
    <p v-if="!busy && message" class="sync-result" :class="{ failed }" role="status">{{ message }}<span v-if="elapsed"> · 用时 {{ elapsed }} 秒</span></p>
  </section>
</template>

<style scoped>
.sync-card { margin-bottom:18px; }.sync-card h3 { font-size:15px; }.sync-card p { font-size:12px; line-height:1.8; color:var(--muted); margin:10px 0; }.sync-card label { display:block; font-size:12px; margin:12px 0; }.sync-card input { margin-top:6px; }.sync-actions { display:flex; align-items:center; flex-wrap:wrap; gap:10px; }.sync-result { white-space:pre-wrap; overflow-wrap:anywhere; }.sync-result.failed { color:var(--red); }
.sync-progress { margin-top:14px; }
.sync-progress-label { display:flex; justify-content:space-between; flex-wrap:wrap; gap:6px 12px; font-size:12px; color:var(--muted); }
.sync-progress progress { display:block; width:100%; height:8px; margin-top:8px; accent-color:var(--accent); }
</style>
