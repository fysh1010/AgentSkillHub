import { reactive } from 'vue';
import { invoke } from '@tauri-apps/api/core';
import type { SkillLinkBackup, SkillLinkPreview } from '../types';
import { loadAll, toast } from '../store';

export const skillLinkDialog = reactive({ open:false, loading:false, saving:false, error:'', preview:undefined as SkillLinkPreview | undefined, restore:undefined as SkillLinkBackup | undefined });
export async function previewSkillLink(agentId: string, skillKey: string, sourceKey?: string) {
  if (skillLinkDialog.loading || skillLinkDialog.saving) return;
  Object.assign(skillLinkDialog, {open:true,loading:true,error:'',preview:undefined,restore:undefined});
  try { skillLinkDialog.preview = await invoke<SkillLinkPreview>('preview_skill_link',{agentId,skillKey,sourceKey:sourceKey||null}); }
  catch(e) { skillLinkDialog.error=String(e); }
  finally { skillLinkDialog.loading=false; }
}
export function previewRestoreSkill(row: SkillLinkBackup) {
  if (skillLinkDialog.saving || skillLinkDialog.loading) return;
  Object.assign(skillLinkDialog, {open:true,error:'',preview:undefined,restore:row});
}
export async function applySkillLink() {
  const p=skillLinkDialog.preview; const r=skillLinkDialog.restore;
  if (skillLinkDialog.saving || skillLinkDialog.loading || (!p && !r)) return;
  skillLinkDialog.saving=true; skillLinkDialog.error='';
  try {
    const message = r ? await invoke<string>('restore_skill_link',{backupId:r.id}) : await invoke<string>('convert_skill_link',{agentId:p!.agent_id,skillKey:p!.skill_key,sourceKey:p!.source_key,fingerprint:p!.fingerprint});
    await loadAll(true); skillLinkDialog.open=false; toast(message,'success');
  } catch(e) { skillLinkDialog.error=String(e); }
  finally { skillLinkDialog.saving=false; }
}
