<script setup lang="ts">
import { computed, ref } from "vue";
import { store, loadAll, toast, discoverMachine } from "../store";
import { adoptAgentSkill, revealPath } from "../api/tauri";
import type { AgentOwnedSkill } from "../types";
import AgentIcon from "./AgentIcon.vue";
import SkillIcon from "./SkillIcon.vue";
import { previewSkillLink, previewRestoreSkill } from '../composables/useSkillLinks';
const backups = computed(() => (store.config?.skill_link_backups || []).filter(b=>store.agents.some(a=>a.agent.id===b.agent_id)));
const keyword = ref("");
const agent = ref("");
const adopting = ref("");
const agents = computed(() => [...new Map(store.ownedSkills.map(s => [s.agent_id, s.agent_name])).entries()]);
const visible = computed(() => store.ownedSkills.filter(s => (!agent.value || s.agent_id === agent.value) && `${s.skill.name} ${s.skill.description} ${s.agent_name}`.toLowerCase().includes(keyword.value.trim().toLowerCase())));
async function adopt(row: AgentOwnedSkill) {
  if (adopting.value) return;
  adopting.value = row.agent_id + ":" + row.skill.key;
  try {
    const key = await adoptAgentSkill(row.agent_id, row.skill.key);
    await loadAll(true);
    toast(`已导入「${key}」，原 Agent 目录保留，可分配给其他 Agent`, "success");
  } catch (e) { toast(String(e), "error"); }
  finally { adopting.value = ""; }
}
async function open(path: string) {
  try { await revealPath(path); } catch (e) { toast(String(e), "error"); }
}
</script>

<template>
  <section class="owned-skills">
    <header class="owned-header"><div class="owned-heading"><h2>Agent 自有技能</h2><b>{{ store.ownedSkills.length }}</b></div><button class="btn ghost" @click="discoverMachine()">重新扫描</button></header>
      <p class="owned-note">识别其他工具安装的实体文件夹和库外链接。导入会复制到技能库，原 Agent 的文件保留；同名内容单独列出，避免覆盖。</p>
      <div v-if="store.ownedSkills.length" class="owned-filters"><input v-model="keyword" class="input" placeholder="搜索 Agent 自有技能…" aria-label="搜索 Agent 自有技能" /><select v-model="agent" class="input" aria-label="筛选技能所属 Agent"><option value="">全部 Agent</option><option v-for="[id,name] in agents" :key="id" :value="id">{{ name }}</option></select></div>
      <div class="card owned-list">
        <div v-for="row in visible" :key="row.agent_id + ':' + row.skill.key" class="owned-row">
          <SkillIcon :name="row.skill.name" :identity="row.skill.path" :src="row.skill.icon_b64" />
          <div class="owned-info"><b>{{ row.skill.name }}</b><span :title="row.skill.description">{{ row.skill.description || '暂无描述' }}</span><button class="owned-path" :title="row.skill.path" @click="open(row.skill.path)">{{ row.skill.path }} ↗</button></div>
          <span class="owner" :title="row.agent_name"><AgentIcon :id="row.agent_id" :name="row.agent_name" :size="17" /><span>{{ row.agent_name }}</span></span>
          <span class="badge gray">{{ row.linked ? '外部链接' : '实体目录' }}</span>
          <button v-if="row.library_key && !row.linked" class="btn" @click="previewSkillLink(row.agent_id,row.library_key,row.skill.key)">备份并转为链接</button>
          <button v-if="!row.library_key" class="btn" :disabled="!!adopting" title="复制到统一技能库，保留原目录" @click="adopt(row)">{{ adopting === row.agent_id + ':' + row.skill.key ? '导入中…' : '导入技能库' }}</button>
          <span v-else-if="row.linked" class="badge amber" title="库中有同名条目；外部链接请在原工具中管理">库中有同名</span>
        </div>
        <div v-if="!visible.length" class="empty">{{ store.ownedSkills.length ? '没有匹配的自有技能' : '未发现技能库之外的用户技能。其他工具安装技能后，这里会自动更新。' }}</div>
      </div>
      <details v-if="backups.length" class="backup-list"><summary>原目录备份 · {{backups.length}}</summary><div v-for="b in backups" :key="b.id" class="owned-row"><div class="owned-info"><b>{{b.skill_key}} · {{store.agents.find(a=>a.agent.id===b.agent_id)?.agent.name}}</b><button class="owned-path" @click="open(b.backup_path)">{{b.backup_path}} ↗</button></div><button class="btn" @click="previewRestoreSkill(b)">恢复原目录</button></div></details>
  </section>
</template>

<style scoped>
.owned-skills { min-width:0; }
.owned-header { display:flex; justify-content:space-between; align-items:center; }
.owned-heading { display:flex; gap:10px; align-items:center; color:var(--text); }
.owned-heading h2 { font-size:15px; font-weight:600; }
.owned-heading b { font-size:11px; padding:1px 7px; background:var(--panel-2); color:var(--muted); border-radius:5px; }
.owned-note { color:var(--muted); font-size:12px; margin:8px 0 16px; }
.owned-filters { display:flex; gap:10px; margin-bottom:12px; }
.owned-filters input { max-width:340px; }
.owned-filters select { width:auto; }
.owned-list { padding:0; }
.owned-row { display:flex; gap:12px; align-items:center; padding:14px 18px; border-bottom:1px solid var(--border-soft); }
.owned-row:last-child { border-bottom:0; }
.owned-info { display:grid; gap:2px; flex:1; min-width:100px; }
.owned-info b { font-size:13px; font-weight:600; }
.owned-info > span { font-size:11px; color:var(--muted); overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
.owned-path { padding:0; width:100%; text-align:left; overflow:hidden; text-overflow:ellipsis; white-space:nowrap; border:0; background:none; color:var(--faint); cursor:pointer; font-family:inherit; font-size:10px; }
.owner { display:flex; align-items:center; gap:6px; color:var(--muted); font-size:11px; max-width:150px; flex-shrink:0; }
.owner > span { overflow:hidden; text-overflow:ellipsis; white-space:nowrap; }
</style>
