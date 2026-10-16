<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { store, loadAll, toast, ui } from "../store";
import { setSkillsRoot, gitBackup, setTokenBudget, exportManifest, importManifest, latestManifest } from "../api/tauri";
import AgentManager from "../components/AgentManager.vue";
import BuiltinSkills from "../components/BuiltinSkills.vue";
import pkg from "../../package.json";
const tabs = [{id:"agents",name:"Agent 工具"},{id:"builtin",name:"内置技能"},{id:"general",name:"通用设置"},{id:"data",name:"数据与备份"}];
const root = ref(store.config?.skills_root || "");
watch(() => store.config?.skills_root, (value, previous) => {
  if (!root.value || root.value === previous) root.value = value || "";
});
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

const budget = ref<number>(store.config?.token_budget || 8000);

async function doSetBudget() {
  const v = Number(budget.value);
  if (!Number.isFinite(v) || v < 1000) {
    toast("预算不能低于 1000 tokens", "error");
    return;
  }
  try {
    store.config = await setTokenBudget(v);
    toast(`上下文预算已设为 ${v} tokens`, "success");
  } catch (e) {
    toast(String(e), "error");
  }
}

// ---------- 环境清单导出/导入 ----------

const exportMsg = ref("");
const manifestPath = ref("");
const importMsg = ref("");
const missingSkills = ref(0);
const exporting = ref(false);
const importing = ref(false);

async function doExport() {
  if (exporting.value) return;
  exporting.value = true;
  exportMsg.value = "";
  try {
    const path = await exportManifest();
    exportMsg.value = `清单已导出：${path}`;
    toast("环境清单已导出", "success");
  } catch (e) {
    toast(String(e), "error");
    exportMsg.value = String(e);
  } finally { exporting.value = false; }
}

async function doImport() {
  if (importing.value) return;
  if (!manifestPath.value.trim()) {
    toast("请输入清单文件路径", "error");
    return;
  }
  if (!confirm("导入清单将覆盖已允许配置的 Agent 的技能分配关系，是否继续？")) return;
  importMsg.value = "";
  importing.value = true;
  try {
    const r = await importManifest(manifestPath.value.trim());
    if (r.ok) {
      await loadAll();
      missingSkills.value = r.missing_skills;
      importMsg.value = `✅ ${r.message}${r.missing_skills > 0 ? `（${r.missing_skills} 条指向缺失技能，将显示为已失效）` : ""}`;
      toast("清单已导入", "success");
    } else {
      toast("导入失败：请确保路径正确", "error");
    }
  } catch (e) {
    toast(String(e), "error");
    importMsg.value = `❌ ${e}`;
  } finally { importing.value = false; }
}

async function loadLatestManifest() {
  try {
    const p = await latestManifest();
    if (p) manifestPath.value = p;
  } catch { /* 无清单是正常情况 */ }
}

onMounted(loadLatestManifest);
</script>
<template>
  <div class="settings">
    <header class="page-head"><div><h1>设置</h1><p class="sub">发现本机工具，决定哪些 Agent 可以由这里管理。</p></div></header>
    <nav class="settings-tabs" aria-label="设置分类"><button v-for="tab in tabs" :key="tab.id" :class="{active:ui.settingsTab === tab.id}" @click="ui.settingsTab = tab.id">{{ tab.name }}</button></nav>
    <AgentManager v-if="ui.settingsTab === 'agents'" />
    <BuiltinSkills v-else-if="ui.settingsTab === 'builtin'" />
    <div v-else-if="ui.settingsTab === 'general'" class="settings-section">
    <!-- 技能库 -->
    <section class="card block">
      <div class="block-head">
        <h3>技能库根目录</h3>
        <span class="badge blue">可自定义</span>
      </div>
      <p class="block-desc">所有技能的存放根目录。工具从这里扫描 SKILL.md，并按此目录部署链接。</p>
      <div class="row">
        <input v-model="root" class="input" placeholder="技能库的绝对路径，默认跟随当前用户目录" />
        <button class="btn primary" @click="saveRoot()">保存并重新扫描</button>
      </div>
      <div class="hint">当前技能数：{{ store.skills.length }} ｜ 技能库：{{ store.config?.skills_root }}</div>
    </section>

    <!-- 上下文预算 -->
    <section class="card block">
      <div class="block-head">
        <h3>上下文预算</h3>
        <button class="btn primary" :disabled="!budget" @click="doSetBudget()">保存</button>
      </div>
      <p class="block-desc">
        每个 Agent 启用技能的 SKILL.md frontmatter（name + description）会常驻上下文，建议控制在 8000 tokens 以内。
      </p>
      <div class="row">
        <input
          v-model.number="budget"
          type="number"
          min="1000"
          step="500"
          class="input"
          style="width: 160px"
          placeholder="8000"
        />
        <span style="color: var(--muted); font-size: 12.5px; align-self: center">
          当前 Agent 常驻上下文上限（tokens，≥1000）
        </span>
      </div>
    </section>

<section class="card block"><h3>关于 AgentSkillHub</h3><p class="block-desc">v{{ pkg.version }} · 本地技能管理。路径跟随当前用户，统一使用黑白或蓝白配色。</p></section>
    </div><div v-else class="settings-section">
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

    <!-- 环境清单 -->
    <section class="card block">
      <div class="block-head">
        <h3>环境清单</h3>
        <div style="display: flex; gap: 8px">
          <button class="btn primary" :disabled="exporting" @click="doExport">导出清单</button>
        </div>
      </div>
      <p class="block-desc">
        把当前「技能库配置 + Agent 分配 + 收藏标签」导出为 JSON 清单，可在新机器或重装后一键恢复。
        不含技能文件内容与审计日志；导入只覆盖已允许配置的 Agent；新工具需在本机确认并开启。
      </p>
      <div v-if="exportMsg" class="hint" :class="exportMsg.startsWith('✅') ? 'green' : ''" style="word-break: break-all">{{ exportMsg }}</div>
      <div class="row" style="margin-top: 12px">
        <input v-model="manifestPath" class="input" placeholder="清单文件路径" />
        <button class="btn"  @click="loadLatestManifest()">探测最新</button>
        <button class="btn primary" :disabled="importing" @click="doImport()">校验并导入</button>
      </div>
      <div v-if="importMsg" class="hint" :class="importMsg.startsWith('✅') ? 'green' : ''" style="word-break: break-all">{{ importMsg }}</div>
    </section>

</div></div>
</template>
<style scoped>
.settings { max-width:1200px; }.page-head { margin-bottom:24px; }.sub { color:var(--muted); font-size:13px; margin-top:6px; }
.settings-tabs { display:flex; gap:28px; border-bottom:1px solid var(--border); margin-bottom:28px; }.settings-tabs button { padding:0 0 14px; border:0; border-bottom:2px solid transparent; background:none; color:var(--muted); font-size:13px; cursor:pointer; }.settings-tabs button.active { color:var(--text); border-color:var(--accent); font-weight:600; }
.settings-section { max-width:980px; }.block { margin-bottom:18px; }.block-head { display:flex; align-items:center; justify-content:space-between; gap:16px; margin-bottom:8px; }.block h3 { font-size:15px; }.block-desc { color:var(--muted); font-size:12px; line-height:1.8; margin:8px 0 14px; }.row { display:flex; gap:10px; }.row .input { flex:1; min-width:0; }.hint { color:var(--faint); font-size:12px; margin-top:12px; }.hint.green { color:var(--green); }
</style>
