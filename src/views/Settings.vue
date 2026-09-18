<script setup lang="ts">
import { onMounted, ref } from "vue";
import { store, loadAll, toast } from "../store";
import { setSkillsRoot, saveAgents, detectAgents, gitBackup } from "../api/tauri";
import type { AgentDef, DetectedAgent } from "../types";

// ---------- Git 备份 ----------

const backing = ref(false);
const backupMsg = ref("");
const backupMsgOut = ref("");
const backupOk = ref(false);

async function doBackup() {
  if (backing.value) return;
  backing.value = true;
  backupMsgOut.value = "";
  try {
    const out = await gitBackup(backupMsg.value);
    backupOk.value = true;
    backupMsgOut.value = `✅ ${out}`;
    backupMsg.value = "";
  } catch (e) {
    backupOk.value = false;
    backupMsgOut.value = String(e);
  } finally {
    backing.value = false;
  }
}

const root = ref(store.config?.skills_root || "");
const editing = ref(false);
const saveMsg = ref("");
const newAgent = ref<AgentDef>({ id: "", name: "", skills_dir: "", builtin: false });
const adding = ref(false);
const detected = ref<DetectedAgent[]>([]);

async function loadDetected() {
  try {
    detected.value = await detectAgents();
  } catch {
    detected.value = [];
  }
}

async function addDetected(d: DetectedAgent) {
  const list = store.config?.agents;
  if (!list) return;
  if (list.some((a) => a.id === d.id)) {
    toast(`Agent ID "${d.id}" 已存在`, "error");
    return;
  }
  list.push({ id: d.id, name: d.name, skills_dir: d.skills_dir, builtin: false });
  try {
    await saveAgents(list.map((a) => ({ ...a })));
    await loadAll();
    toast(`已添加 ${d.name}`, "success");
    await loadDetected();
  } catch (e) {
    toast(String(e), "error");
  }
}

onMounted(loadDetected);

async function saveRoot() {
  if (!root.value.trim()) {
    toast("技能库目录不能为空", "error");
    return;
  }
  try {
    await setSkillsRoot(root.value.trim());
    await loadAll();
    toast("技能库目录已更新", "success");
  } catch (e) {
    toast(String(e), "error");
  }
}

async function doSaveAgents() {
  try {
    const list = store.config?.agents || [];
    await saveAgents(list.map((a) => ({ ...a })));
    await loadAll();
    editing.value = false;
    saveMsg.value = "Agent 列表已保存";
    toast("Agent 列表已保存", "success");
  } catch (e) {
    toast(String(e), "error");
  }
}

function updateAgentField(idx: number, field: keyof AgentDef, val: string | boolean) {
  const list = store.config?.agents;
  if (!list) return;
  list[idx] = { ...list[idx], [field]: val };
}

function removeAgent(idx: number) {
  const list = store.config?.agents;
  if (!list) return;
  list.splice(idx, 1);
}

function addAgent() {
  if (!newAgent.value.id.trim() || !newAgent.value.skills_dir.trim()) {
    toast("ID 与目录必填", "error");
    return;
  }
  const list = store.config?.agents;
  if (!list) return;
  if (list.some((a) => a.id === newAgent.value.id.trim())) {
    toast("Agent ID 已存在", "error");
    return;
  }
  list.push({
    id: newAgent.value.id.trim(),
    name: newAgent.value.name.trim() || newAgent.value.id.trim(),
    skills_dir: newAgent.value.skills_dir.trim(),
    builtin: false,
  });
  newAgent.value = { id: "", name: "", skills_dir: "", builtin: false };
  adding.value = false;
}

function suggestId() {
  if (!newAgent.value.id && newAgent.value.name) {
    newAgent.value.id = newAgent.value.name.trim().toLowerCase().replace(/\s+/g, "-");
  }
}
</script>

<template>
  <div class="settings">
    <header class="page-head">
      <div>
        <h1>设置</h1>
        <p class="sub">技能库根目录与 AI Agent 工具的注册管理</p>
      </div>
    </header>

    <!-- 技能库 -->
    <section class="card block">
      <div class="block-head">
        <h3>技能库根目录</h3>
        <span class="badge blue">可自定义</span>
      </div>
      <p class="block-desc">所有技能的存放根目录。工具从这里扫描 SKILL.md，并按此目录部署链接。</p>
      <div class="row">
        <input v-model="root" class="input" placeholder="例如 E:\my-skills" />
        <button class="btn primary" @click="saveRoot()">保存并重新扫描</button>
      </div>
      <div class="hint">当前技能数：{{ store.skills.length }} ｜ 技能库：{{ store.config?.skills_root }}</div>
    </section>

    <!-- Agent 列表 -->
    <section class="card block">
      <div class="block-head">
        <h3>AI Agent 工具</h3>
        <div style="display: flex; gap: 8px">
          <button class="btn" @click="adding = !adding">{{ adding ? "取消" : "新增" }}</button>
          <button class="btn primary" @click="doSaveAgents()">保存变更</button>
        </div>
      </div>
      <p class="block-desc">管理每个 AI Agent 的 skills 目录。内置工具可修改路径，也可以添加你自己的 Agent。</p>

      <div v-if="adding" class="new-agent">
        <input v-model="newAgent.name" class="input" placeholder="名称（如 Trae）" @input="suggestId()" />
        <input v-model="newAgent.id" class="input" placeholder="ID（唯一，如 trae）" />
        <input v-model="newAgent.skills_dir" class="input wide" placeholder="skills 目录绝对路径" />
        <button class="btn primary" @click="addAgent()">添加</button>
      </div>

      <div class="agent-table">
        <div class="at-row at-head">
          <span>名称</span>
          <span>ID</span>
          <span>skills 目录</span>
          <span>来源</span>
          <span></span>
        </div>
        <div v-for="(a, i) in store.config?.agents || []" :key="a.id + i" class="at-row">
          <input class="input" :value="a.name" @input="updateAgentField(i, 'name', ($event.target as HTMLInputElement).value)" />
          <code class="at-id">{{ a.id }}</code>
          <input class="input" :value="a.skills_dir" @input="updateAgentField(i, 'skills_dir', ($event.target as HTMLInputElement).value)" />
          <span class="badge" :class="a.builtin ? 'blue' : 'gray'">{{ a.builtin ? "内置" : "自定义" }}</span>
          <button class="btn danger" style="padding: 4px 10px" @click="removeAgent(i)" :disabled="a.builtin && (store.config?.agents.length || 0) <= 1">
            删除
          </button>
        </div>
      </div>
      <div v-if="saveMsg" class="hint green">{{ saveMsg }}</div>
    </section>

    <!-- 自动检测 -->
    <section class="card block">
      <div class="block-head">
        <h3>检测到的未注册 Agent</h3>
        <button class="btn" @click="loadDetected()">重新检测</button>
      </div>
      <p class="block-desc">
        自动扫描用户目录下已存在的 skills 目录（含重定向到其他盘的 Junction，按最终路径识别）。点击即可一键注册。
      </p>
      <div v-if="detected.length" class="detected-list">
        <div v-for="d in detected" :key="d.id" class="detected-item">
          <div class="d-info">
            <b>{{ d.name }}</b>
            <code :title="d.skills_dir">{{ d.skills_dir }}</code>
          </div>
          <button class="btn primary" @click="addDetected(d)">一键添加</button>
        </div>
      </div>
      <div v-else class="hint">未检测到新的 Agent 工具 —— 已注册的不会重复出现。</div>
    </section>

    <!-- Git 备份 -->
    <section class="card block">
      <div class="block-head">
        <h3>技能库 Git 备份</h3>
        <button class="btn primary" :disabled="backing" @click="doBackup()">
          {{ backing ? "备份中…" : "立即备份快照" }}
        </button>
      </div>
      <p class="block-desc">
        把技能库当前状态提交到本地 Git 仓库（{{ store.config?.skills_root }}，无仓库时自动初始化）。
        删错 / 改坏技能后可随时用 git 回滚到任意快照。
      </p>
      <input
        v-model="backupMsg"
        class="input"
        style="margin-bottom: 8px"
        placeholder="快照说明（可选，如：更新 xxx 前备份）"
      />
      <div v-if="backupMsgOut" class="hint" :class="backupOk ? 'green' : ''">{{ backupMsgOut }}</div>
    </section>

    <section class="card block">
      <div class="block-head"><h3>关于</h3></div>
      <p class="block-desc">
        AgentSkillHub v0.1.0 · 本地桌面应用，技能统一管理。
        技能源文件永远只存放在技能库目录；本工具只负责在各 Agent 的 skills 目录创建 / 移除 Junction 链接。
      </p>
    </section>
  </div>
</template>

<style scoped>
.settings {
  max-width: 980px;
}
.page-head {
  margin-bottom: 16px;
}
h1 {
  font-size: 21px;
  font-weight: 700;
}
.sub {
  color: var(--muted);
  font-size: 13px;
  margin-top: 2px;
}
.block {
  margin-bottom: 16px;
}
.block-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 6px;
}
.block-head h3 {
  font-size: 15px;
}
.block-desc {
  color: var(--muted);
  font-size: 12.5px;
  margin-bottom: 12px;
}
.row {
  display: flex;
  gap: 10px;
}
.row .input {
  flex: 1;
}
.hint {
  margin-top: 10px;
  color: var(--faint);
  font-size: 12px;
}
.hint.green {
  color: var(--green);
}

.new-agent {
  display: grid;
  grid-template-columns: 1fr 1fr 2fr auto;
  gap: 8px;
  background: var(--bg-soft);
  border-radius: var(--radius-sm);
  padding: 10px;
  margin-bottom: 12px;
}

.agent-table {
  border: 1px solid var(--border-soft);
  border-radius: var(--radius);
  overflow: hidden;
}
.at-row {
  display: grid;
  grid-template-columns: 1.2fr 0.8fr 2.2fr 0.7fr 60px;
  gap: 8px;
  align-items: center;
  padding: 8px 12px;
  border-bottom: 1px solid var(--border-soft);
  font-size: 13px;
}
.at-row:last-child {
  border-bottom: none;
}
.at-head {
  background: var(--bg-soft);
  color: var(--faint);
  font-size: 12px;
}
.at-id {
  font-family: Consolas, monospace;
  font-size: 12px;
  color: var(--accent);
}

.detected-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}
.detected-item {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  padding: 10px 14px;
  border: 1px solid var(--border-soft);
  border-radius: var(--radius-sm);
  background: var(--bg-soft);
  transition: all 0.15s;
}
.detected-item:hover {
  border-color: var(--accent);
  background: var(--accent-soft);
}
.d-info {
  display: flex;
  flex-direction: column;
  min-width: 0;
}
.d-info b {
  font-size: 13.5px;
}
.d-info code {
  font-size: 11.5px;
  color: var(--faint);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
