<script setup lang="ts">
import { onMounted, reactive, ref, watch } from "vue";
import * as api from "../api/maintenance";
import { revealPath, setSkillsRoot } from "../api/tauri";
import { loadAll, store } from "../store";

const target = reactive<api.SkillBackupTarget>({ location: "", username: "", webdav: false });
const password = ref("");
const passphrase = ref("");
const busy = ref(false);
const message = ref("");
const failed = ref(false);
const snapshots = ref<api.SkillBackupItem[]>([]);
const selected = ref("");
const restored = ref("");

async function run(task: () => Promise<void>) {
  if (busy.value) return;
  busy.value = true; failed.value = false; message.value = "";
  try { await task(); }
  catch (e) { failed.value = true; message.value = String(e); }
  finally { busy.value = false; }
}
onMounted(() => run(async () => { Object.assign(target, await api.skillBackupTarget()); }));
watch(() => [target.location, target.username, target.webdav, password.value], () => {
  snapshots.value = []; selected.value = "";
});
function setProvider(webdav: boolean) {
  if (target.webdav === webdav) return;
  target.webdav = webdav; target.location = ""; target.username = ""; password.value = "";
  message.value = "";
}
async function refresh() {
  snapshots.value = await api.skillBackupList({ ...target }, password.value);
  selected.value = snapshots.value[0]?.name || "";
}
function load() {
  return run(async () => {
    await refresh();
    message.value = snapshots.value.length ? `找到 ${snapshots.value.length} 份备份。` : "连接成功，还没有备份。";
  });
}
function chooseFolder() {
  return run(async () => {
    const path = await api.pickBackupFolder();
    if (path) target.location = path;
  });
}
function create() {
  if (target.webdav && !confirm("将向这个 WebDAV 文件夹上传加密技能快照，是否继续？")) return;
  return run(async () => {
    const result = await api.skillBackupCreate({ ...target }, password.value, passphrase.value);
    const item = { name: result.name, created: Number(result.name.split("-")[1]) };
    snapshots.value = [item, ...snapshots.value.filter(snapshot => snapshot.name !== item.name)];
    selected.value = item.name;
    message.value = `已备份 ${result.files} 个文件。`;
    try { await refresh(); } catch { message.value += " 列表暂未刷新，可稍后重试。"; }
  });
}
function restore() {
  return run(async () => {
    const result = await api.skillBackupRestore({ ...target }, password.value, passphrase.value, selected.value);
    restored.value = result.path;
    message.value = `已将 ${result.files} 个文件恢复到新目录，当前技能库未被覆盖。`;
  });
}
function activate() {
  if (!confirm("将恢复目录设为当前技能库。原目录仍保留，需要时可在通用设置切回。是否继续？")) return;
  return run(async () => { await setSkillsRoot(restored.value); await loadAll(); message.value = "已切换到恢复的技能库。"; });
}
const label = (item: api.SkillBackupItem) => `${new Date(item.created * 1000).toLocaleString()} · ${item.name.split("-")[2]?.slice(0, 6)}`;
</script>

<template>
  <section class="card skill-backup">
    <h3>加密技能备份</h3>
    <p>在一台电脑创建快照，在另一台电脑选同一位置和口令恢复。每次生成独立版本，无需 Git。</p>
    <div class="backup-actions" role="group" aria-label="备份位置">
      <button class="btn" :class="{ primary: !target.webdav }" :disabled="busy" :aria-pressed="!target.webdav" @click="setProvider(false)">本地 / 网盘文件夹</button>
      <button class="btn" :class="{ primary: target.webdav }" :disabled="busy" :aria-pressed="target.webdav" @click="setProvider(true)">WebDAV 服务器</button>
    </div>
    <label for="backup-location">{{ target.webdav ? "WebDAV 文件夹地址" : "备份文件夹" }}</label>
    <div class="backup-field">
      <input id="backup-location" v-model.trim="target.location" class="input" :disabled="busy" :placeholder="target.webdav ? 'https://nas.example.com/dav/skills/' : 'D:/SkillBackups'" />
      <button v-if="!target.webdav" class="btn" :disabled="busy" @click="chooseFolder">选择文件夹</button>
    </div>
    <div v-if="target.webdav" class="backup-credentials">
      <label>账号<input v-model="target.username" class="input" :disabled="busy" autocomplete="off" /></label>
      <label>应用密码<input v-model="password" type="password" class="input" :disabled="busy" autocomplete="off" /></label>
    </div>
    <p v-if="target.webdav">填写服务商提供的 HTTPS 地址，目录需已创建。成功读取后记住地址和账号，密码只在当前页面使用。</p>
    <label>备份口令<input v-model="passphrase" type="password" class="input" :disabled="busy" autocomplete="off" placeholder="至少 12 个字符；恢复时输入同一口令" /></label>
    <p>只备份技能文件，跳过隐藏文件、常见凭证文件和缓存；不包含 Git 历史、MCP 库或 Agent 配置。服务器只收到加密快照，口令遗失无法恢复。</p>
    <div class="backup-actions">
      <button class="btn" :disabled="busy || !target.location" @click="load">读取备份列表</button>
      <button class="btn primary" :disabled="busy || !target.location || passphrase.length < 12" @click="create">创建加密备份</button>
      <span v-if="busy" role="status">正在处理…</span>
    </div>
    <div v-if="snapshots.length" class="backup-field backup-restore">
      <select v-model="selected" class="input" aria-label="选择备份版本" :disabled="busy">
        <option v-for="item in snapshots" :key="item.name" :value="item.name">{{ label(item) }}</option>
      </select>
      <button class="btn" :disabled="busy || !selected || passphrase.length < 12" @click="restore">恢复到新目录</button>
    </div>
    <p v-if="message" role="status" :class="{ failed }">{{ message }}</p>
    <div v-if="restored" class="restored">
      <p>{{ restored }}</p>
      <div class="backup-actions">
        <button class="btn" :disabled="busy" @click="run(() => revealPath(restored))">打开恢复目录</button>
        <button class="btn" :disabled="busy || restored === store.config?.skills_root" @click="activate">设为当前技能库</button>
      </div>
    </div>
  </section>
</template>

<style scoped>
.skill-backup { margin-bottom:18px; }
h3 { font-size:15px; }
p { font-size:12px; line-height:1.8; color:var(--muted); margin:10px 0; overflow-wrap:anywhere; }
label { display:block; font-size:12px; margin:12px 0; }
.input { margin-top:6px; }
.backup-actions,.backup-field { display:flex; gap:8px; align-items:center; flex-wrap:wrap; }
.backup-field .input { flex:1; min-width:180px; }
.backup-field .btn { white-space:nowrap; }
.backup-credentials { display:grid; grid-template-columns:repeat(auto-fit,minmax(160px,1fr)); gap:12px; }
.backup-restore,.restored { border-top:1px solid var(--border); margin-top:12px; padding-top:10px; }
.failed { color:var(--red); }
</style>
