import type { BuiltinSource, MachineInventory } from "../types";

const productNames: [string, string][] = [
  ["WorkBuddy", "WorkBuddy"], ["Codex", "OpenAI Codex"],
  ["Claude", "Claude Code"], ["豆包", "豆包 Doubao"],
  ["Qoder CLI", "Qoder CLI"], ["Qoder CN", "Qoder CN"], ["Qoder", "Qoder"],
  ["Trae CN", "Trae CN"], ["Trae", "Trae"],
];

export function sourceProduct(group: string): string {
  return productNames.find(([prefix]) => group.startsWith(prefix))?.[1] || group.replace(/ 系统技能$/, "");
}

export function isSharedSkillSource(source: BuiltinSource): boolean {
  return source.skills_dir.replaceAll('\\','/').replace(/\/+$/, '').toLowerCase().endsWith('/.agents/skills');
}

export function installedBuiltinSources(inventory: MachineInventory | null): BuiltinSource[] {
  if (!inventory) return [];
  const managed = inventory.agents.filter(agent => agent.management_enabled && !agent.shared);
  const names = new Set(managed.flatMap(agent => [agent.name, sourceProduct(agent.name)]));
  const pathKey = (path: string) => path.replaceAll('\\','/').replace(/\/+$/, '').toLowerCase();
  return inventory.sources.filter(source => {
    if (isSharedSkillSource(source)) return managed.length > 0;
    if (source.group.includes('系统技能') || pathKey(source.skills_dir).split('/').includes('.system')) return false;
    if (!names.has(source.group) && !names.has(sourceProduct(source.group))) return false;
    return source.name === '用户安装 / 自定义技能'
      || source.custom && managed.some(agent => pathKey(agent.skills_dir) === pathKey(source.skills_dir));
  });
}
