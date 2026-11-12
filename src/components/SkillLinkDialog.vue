<script setup lang="ts">
import { skillLinkDialog as state, applySkillLink } from '../composables/useSkillLinks';
import { useEsc } from '../composables/useEsc';
function close() { if (!state.loading && !state.saving) state.open=false; }
useEsc(close);
</script>
<template>
  <div v-if="state.open" class="modal-mask" @click.self="close"><section class="modal" style="max-width:700px" role="dialog" aria-modal="true" aria-label="实体技能与链接转换">
    <header class="modal-head"><b>{{state.restore?'恢复原实体技能':'备份并转为技能库链接'}}</b><button class="btn ghost" :disabled="state.loading||state.saving" @click="close">✕</button></header>
    <div class="modal-body">
      <p v-if="state.loading">正在检查完整技能内容与目录…</p>
      <template v-if="state.preview"><p>确认后使用技能库中的版本。原实体目录会完整移到同级的 <code>.agentskillhub-backups</code> 中，随后在 Agent 用户技能根目录创建链接。</p><dl><dt>原实体目录</dt><dd>{{state.preview.original_path}}</dd><dt>新链接位置</dt><dd>{{state.preview.link_path}}</dd><dt>链接目标 · 技能库版本</dt><dd>{{state.preview.target}}</dd></dl><p class="note">同名不代表内容相同。转换后由技能库统一更新；可在“Agent 自有技能 → 原目录备份”恢复。预览后内容变化会中止操作。</p></template>
      <template v-if="state.restore"><p>移除仍指向技能库的链接，恢复原目录。备份或原位置被外部改动时会停止。</p><dl><dt>原目录备份</dt><dd>{{state.restore.backup_path}}</dd><dt>恢复到</dt><dd>{{state.restore.original_path}}</dd></dl></template>
      <p v-if="state.error" class="error" role="alert">{{state.error}}</p>
    </div><footer class="modal-foot"><button class="btn" :disabled="state.loading||state.saving" @click="close">取消</button><button class="btn primary" :disabled="state.loading||state.saving||!!state.error||(!state.preview&&!state.restore)" @click="applySkillLink">{{state.saving?'正在处理…':state.restore?'确认恢复原目录':'确认备份并创建链接'}}</button></footer>
  </section></div>
</template>
<style scoped>
p{font-size:12px;line-height:1.9}dl{padding:15px;background:var(--bg-soft);border:1px solid var(--border);border-radius:8px}dt{font-size:11px;color:var(--muted);margin-bottom:6px}dd{font:11px/1.8 Consolas,monospace;overflow-wrap:anywhere;margin:0 0 16px}dd:last-child{margin-bottom:0}.note{color:var(--muted)}.error{color:var(--red);white-space:pre-wrap}
</style>
