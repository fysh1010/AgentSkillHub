import { computed, reactive } from "vue";
import { assignSkills, hasExternalSkill, isAssigned, store, toast } from "../store";
import type { SkillAssignment } from "../types";

export const assignmentDialog = reactive({ title: "", changes: [] as SkillAssignment[] });

export const assignmentAgents = computed(() => {
  return store.agents.map(a => ({ id:a.agent.id, name:a.agent.name, writable:true, installed:true }));
});

export function assignmentChanges(agentIds: string[], skillKeys: string[], mode: "all" | "none" | "invert"): SkillAssignment[] {
  return agentIds.flatMap(agent_id => skillKeys.flatMap(skill_key => {
    if (hasExternalSkill(agent_id, skill_key)) return [];
    const current = isAssigned(agent_id, skill_key);
    const enabled = mode === "invert" ? !current : mode === "all";
    return enabled === current ? [] : [{ agent_id, skill_key, enabled }];
  }));
}

export function previewAssignments(changes: SkillAssignment[], title: string) {
  if (store.assigning) return;
  if (!changes.length) { toast("当前分配已符合选择，无需变更"); return; }
  assignmentDialog.title = title;
  assignmentDialog.changes = changes;
}

export async function toggleAssignment(agentId: string, skillKey: string) {
  await assignSkills([{ agent_id: agentId, skill_key: skillKey, enabled: !isAssigned(agentId, skillKey) }]);
}
