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

export function installedBuiltinSources(inventory: MachineInventory | null): BuiltinSource[] {
  if (!inventory) return [];
  const installed = new Set(inventory.agents.filter(agent => agent.installed && !agent.shared).map(agent => agent.name));
  return inventory.sources.filter(source => source.custom || installed.has(source.group) || installed.has(sourceProduct(source.group)));
}
