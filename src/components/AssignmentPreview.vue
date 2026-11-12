<script setup lang="ts">
import { ref, watch } from "vue";
import { assignmentDialog } from "../composables/useSkillAssignments";
import { applySkillAssignments } from "../api/tauri";
import { assignSkills, store } from "../store";
import { useEsc } from "../composables/useEsc";
import type { DeployResult } from "../types";
const preview = ref<DeployResult>();
const loading = ref(false);
const error = ref("");
let request = 0;
function close() { if (!store.assigning) assignmentDialog.changes = []; }
useEsc(close);
watch(() => assignmentDialog.changes, async changes => {
  const id = ++request;
  preview.value = undefined;
  error.value = "";
  if (!changes.length) return;
  loading.value = true;
  try {
    const result = await applySkillAssignments(changes, true);
    if (id === request) preview.value = result;
  } catch (e) { if (id === request) error.value = String(e); }
  finally { if (id === request) loading.value = false; }
});
async function apply() {
  if (!preview.value || loading.value || error.value) return;
  if (await assignSkills([...assignmentDialog.changes])) close();
}
</script>

<template>
  <div v-if="assignmentDialog.changes.length" class="modal-mask" @click.self="close">
    <div class="modal" style="max-width:700px" role="dialog" aria-modal="true" aria-label="技能分配预览">
      <div class="modal-head"><b>{{ assignmentDialog.title }}</b><button class="btn ghost" :disabled="store.assigning" @click="close">✕</button></div>
      <div class="modal-body">
        <p class="preview-note">确认后应用以下技能分配，可在按 Agent 视图回滚最近一次操作。</p>
        <p v-if="loading">正在检查目录与同名冲突…</p>
        <p v-else-if="error" class="preview-error" role="alert">{{ error }}</p>
        <template v-else-if="preview">
          <p>{{ preview.summary }}</p>
          <div v-for="action in preview.actions" :key="action.link_path" class="preview-row">
            <span class="badge" :class="action.kind === 'create_link' ? 'green' : 'gray'">{{ action.kind === 'create_link' ? '启用' : '停用' }}</span>
            <div><b>{{ action.skill_key }}</b><span>{{ action.message }}</span><code>{{ action.link_path }}</code></div>
          </div>
        </template>
      </div>
      <div class="modal-foot"><button class="btn" :disabled="store.assigning" @click="close">取消</button><button class="btn primary" :disabled="loading || !!error || !preview || store.assigning" @click="apply">{{ store.assigning ? '应用中…' : '确认应用' }}</button></div>
    </div>
  </div>
</template>

<style scoped>
.preview-note { color:var(--muted); margin-bottom:16px; font-size:12px; }
.preview-error { color:var(--red); white-space:pre-wrap; }
.preview-row { display:flex; align-items:flex-start; gap:12px; padding:12px 0; border-bottom:1px solid var(--border-soft); }
.preview-row > div { display:grid; gap:3px; min-width:0; }
.preview-row span:not(.badge), .preview-row code { color:var(--muted); font-size:12px; overflow-wrap:anywhere; }
</style>
