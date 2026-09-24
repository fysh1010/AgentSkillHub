<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { store, ui, toast, loadAll } from "../store";
import { doctorScan, doctorFix } from "../api/tauri";
import type { DoctorIssue, DoctorFixResult } from "../types";

const KIND_LABEL: Record<string, string> = {
  broken_link: "失效链接",
  wrong_link: "指向未启用的技能",
  stale_config: "配置引用失效技能",
  drift: "待重新部署",
  orphan_dir: "孤儿实体目录",
  unhealthy_skill: "技能库异常",
};
const KIND_ICON: Record<string, string> = {
  broken_link: "🔗",
  wrong_link: "🔀",
  stale_config: "📋",
  drift: "🔄",
  orphan_dir: "📁",
  unhealthy_skill: "⚠️",
};

const issues = ref<DoctorIssue[]>([]);
const scanning = ref(false);
const fixing = ref(false);

onMounted(() => {
  scan();
});

async function scan() {
  scanning.value = true;
  try {
    issues.value = await doctorScan();
    store.doctorCount = issues.value.length;
  } catch (e) {
    toast(String(e), "error");
  } finally {
    scanning.value = false;
  }
}

const fixableIds = computed(() => issues.value.filter((i) => i.fixable).map((i) => i.id));

async function fixAll() {
  const ids = fixableIds.value;
  if (!ids.length) return;
  fixing.value = true;
  try {
    const r: DoctorFixResult = await doctorFix(ids);
    toast(`体检修复完成：成功 ${r.fixed}，失败 ${r.failed}`, r.failed > 0 ? "error" : "success");
    await Promise.all([scan(), loadAll()]);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    fixing.value = false;
  }
}

async function fixOne(id: string) {
  fixing.value = true;
  try {
    const r: DoctorFixResult = await doctorFix([id]);
    toast(r.messages[0] || "修复完成", r.failed > 0 ? "error" : "success");
    await Promise.all([scan(), loadAll()]);
  } catch (e) {
    toast(String(e), "error");
  } finally {
    fixing.value = false;
  }
}

function gotoExternal(id: string) {
  ui.page = "agents";
  toast("请到 Agent 配置页找到该 Agent，查看外部技能并收编", "info");
}
</script>

<template>
  <div>
    <header class="page-head">
      <div>
        <h1>体检</h1>
        <p class="sub">扫描技能库与所有 Agent 链接状态，发现并修复问题</p>
      </div>
      <div style="display: flex; gap: 10px">
        <button class="btn" @click="scan" :disabled="scanning || fixing">
          {{ scanning ? "扫描中…" : "重新体检" }}
        </button>
        <button
          class="btn primary"
          @click="fixAll"
          :disabled="scanning || fixing || !fixableIds.length"
        >
          {{ fixing ? "修复中…" : `全部修复（${fixableIds.length}）` }}
        </button>
      </div>
    </header>

    <div v-if="scanning" class="empty" style="padding: 48px">
      <div class="icon">🔍</div>
      正在扫描，请稍候…
    </div>
    <div v-else-if="!issues.length" class="empty" style="padding: 48px">
      <div class="icon">✅</div>
      <b>技能库健康，未发现问题</b>
    </div>
    <div v-else>
      <!-- 汇总卡 -->
      <div class="card summary-bar" style="margin-bottom: 20px">
        <span style="font-weight: 600; font-size: 15px">
          发现 <b style="color: var(--amber)">{{ issues.length }}</b> 个问题
        </span>
        <span style="color: var(--muted); font-size: 12px; margin-left: 12px">
          {{ issues.filter((i) => i.fixable).length }} 个可自动修复，
          {{ issues.length - issues.filter((i) => i.fixable).length }} 个需手动处理
        </span>
      </div>

      <!-- 按 kind 分组 -->
      <template v-for="kind in [...new Set(issues.map((i) => i.kind))]" :key="kind">
        <div class="card issue-group" style="margin-bottom: 18px">
          <div class="issue-group-head">
            <span class="issue-kind-icon">{{ KIND_ICON[kind] || "❓" }}</span>
            <b style="font-size: 14px">{{ KIND_LABEL[kind] || kind }}</b>
            <span class="badge" style="margin-left: 8px; font-size: 11px">
              {{ issues.filter((i) => i.kind === kind).length }} 个
            </span>
          </div>
          <div
            v-for="i in issues.filter((x) => x.kind === kind)"
            :key="i.id"
            class="issue-row"
          >
            <span class="badge" :class="i.fixable ? 'amber' : 'gray'" style="flex-shrink: 0">
              {{ i.agent_name || "技能库" }}
            </span>
            <code class="issue-path" :title="i.path">{{ i.path }}</code>
            <span class="issue-detail">{{ i.detail }}</span>
            <button
              v-if="i.fixable"
              class="btn"
              style="flex-shrink: 0; padding: 3px 12px; font-size: 12px"
              :disabled="fixing"
              @click="fixOne(i.id)"
            >
              修复
            </button>
          </div>
        </div>
      </template>
    </div>
  </div>
</template>

<style scoped>
.summary-bar {
  display: flex;
  align-items: center;
  padding: 14px 18px;
}
.issue-group {
  overflow: hidden;
}
.issue-group-head {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 16px;
  border-bottom: 1px solid var(--border-soft);
  background: var(--bg-soft);
}
.issue-kind-icon {
  font-size: 16px;
}
.issue-row {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 16px;
  border-bottom: 1px solid var(--border-soft);
  font-size: 13px;
  transition: background 0.12s;
}
.issue-row:last-child {
  border-bottom: none;
}
.issue-row:hover {
  background: var(--bg-soft);
}
.issue-path {
  flex: 1;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: var(--faint);
  font-size: 12px;
}
.issue-detail {
  flex: 2;
  min-width: 0;
  color: var(--muted);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
</style>
