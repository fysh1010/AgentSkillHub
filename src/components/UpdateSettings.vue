<script setup lang="ts">
import { useUpdates } from "../composables/useUpdates";
import { toast } from "../store";
const { info, busy, error, automatic, check, setAutomatic, open } = useUpdates();
async function toggle(event: Event) {
  try { await setAutomatic((event.target as HTMLInputElement).checked); }
  catch (e) { (event.target as HTMLInputElement).checked = automatic.value; toast(String(e), "error"); }
}
</script>
<template>
  <section class="card updates-card">
    <div class="updates-head"><h3>版本更新</h3><button class="btn" :disabled="busy" @click="check(true)">{{ busy ? '检查中…' : '检查新版本' }}</button></div>
    <p><label><input type="checkbox" :checked="automatic" @change="toggle" />启动时检查正式版本（每天最多一次）</label></p>
    <p>只访问本项目 GitHub Releases；由你下载并更新客户端。</p>
    <template v-if="info">
      <p>当前 {{ info.current_version }} · 最新 {{ info.latest_version }} · {{ info.available ? '发现新版本' : '当前无需更新' }}</p>
      <div class="updates-actions"><button class="btn" @click="open(info.release_url).catch(e => toast(String(e),'error'))">查看发布说明</button><button v-if="info.available && info.download_url" class="btn primary" @click="open(info.download_url).catch(e => toast(String(e),'error'))">下载更新</button></div>
      <details v-if="info.notes"><summary>更新内容</summary><pre>{{ info.notes }}</pre></details>
    </template>
    <p v-if="error" role="alert">{{ error }}</p>
  </section>
</template>
<style scoped>
.updates-card { margin-bottom:18px; }.updates-head,.updates-actions { display:flex; align-items:center; gap:10px; }.updates-head { justify-content:space-between; }.updates-card h3 { font-size:15px; }.updates-card p,details { margin:12px 0; font-size:12px; color:var(--muted); line-height:1.8; }.updates-card pre { white-space:pre-wrap; overflow-wrap:anywhere; font:inherit; max-height:260px; overflow:auto; }input { vertical-align:middle; margin-right:6px; }
</style>
