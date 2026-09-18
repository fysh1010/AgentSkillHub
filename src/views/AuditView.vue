<script setup lang="ts">
import { computed, ref } from "vue";
import { store, loadAll, toast } from "../store";
import { clearAudit, formatTs } from "../api/tauri";
import type { AuditEntry } from "../types";

const expanded = ref<string | null>(null);
const audit = computed<AuditEntry[]>(() => [...(store.config?.audit || [])].reverse());

function agentName(id: string) {
  if (id === "-") return "技能库";
  return store.agents.find((a) => a.agent.id === id)?.agent.name || id;
}

function tone(e: AuditEntry) {
  if (e.action === "rollback") return "amber";
  return e.actions.every((a) => a.ok) ? "green" : "red";
}

function actionLabel(kind: string) {
  const map: Record<string, string> = {
    create_link: "创建链接",
    remove_link: "移除链接",
    remove_parent_link: "迁移整目录",
    recycle_dir: "移入回收站",
  };
  return map[kind] || kind;
}

async function doClear() {
  if (!confirm("确定清空全部部署记录？此操作只删除日志，不影响已部署的链接。")) return;
  try {
    await clearAudit();
    await loadAll();
    toast("审计日志已清空", "success");
  } catch (e) {
    toast(String(e), "error");
  }
}
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>部署记录</h1>
        <p class="sub">每次部署 / 回滚 / 删除的完整操作明细，可追溯到单条链接</p>
      </div>
      <button class="btn danger" @click="doClear()">清空记录</button>
    </header>

    <div class="timeline">
      <div v-for="e in audit" :key="e.id" class="card entry">
        <div class="entry-head" @click="expanded = expanded === e.id ? null : e.id">
          <span class="badge" :class="tone(e)">
            {{ e.action === "deploy" ? "部署" : e.action === "delete_skill" ? "删除" : "回滚" }}
          </span>
          <div class="entry-info">
            <b>{{ agentName(e.agent_id) }}</b>
            <span class="entry-detail">{{ e.detail }}</span>
          </div>
          <span class="entry-ts">{{ formatTs(e.ts) }}</span>
        </div>
        <div v-if="expanded === e.id" class="entry-body">
          <div v-for="(a, i) in e.actions" :key="i" class="act-row">
            <span class="badge" :class="a.ok ? 'green' : 'red'">
              {{ a.ok ? "成功" : "失败" }}
            </span>
            <span class="act-kind">{{ actionLabel(a.kind) }}</span>
            <code class="act-path">{{ a.link_path }}</code>
            <span class="act-target" v-if="a.target" :title="a.target">→ {{ a.target }}</span>
          </div>
        </div>
      </div>
      <div v-if="!audit.length" class="card empty">
        <div class="icon">◫</div>
        还没有部署记录。在「Agent 配置」中部署后，这里会展示每次操作的明细。
      </div>
    </div>
  </div>
</template>

<style scoped>
.page-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
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

.timeline {
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.entry {
  padding: 12px 16px;
}
.entry-head {
  display: flex;
  align-items: center;
  gap: 12px;
  cursor: pointer;
}
.entry-info {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}
.entry-info b {
  font-size: 13.5px;
}
.entry-detail {
  color: var(--muted);
  font-size: 12.5px;
}
.entry-ts {
  color: var(--faint);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.entry-body {
  margin-top: 12px;
  border-top: 1px dashed var(--border-soft);
  padding-top: 10px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.act-row {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
  flex-wrap: wrap;
}
.act-kind {
  color: var(--text);
  min-width: 64px;
}
.act-path {
  font-family: Consolas, monospace;
  font-size: 11.5px;
  color: var(--accent);
  background: var(--bg-soft);
  padding: 1px 7px;
  border-radius: 4px;
  word-break: break-all;
}
.act-target {
  color: var(--faint);
  font-size: 11.5px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  max-width: 320px;
}
</style>
