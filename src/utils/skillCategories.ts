import type { SkillInfo } from "../types";

export const ROOT_CATEGORY = "(根)";

export function categoryLabel(category: string): string {
  return category === ROOT_CATEGORY ? "未分类" : category.split("/").map(part => part.replace(/^\d+-/, "")).join(" / ");
}

export function skillCategories(skills: SkillInfo[]): string[] {
  const categories = new Set<string>();
  for (const skill of skills) {
    if (skill.category === ROOT_CATEGORY) continue;
    const parts = skill.category.split("/");
    for (let depth = 1; depth <= parts.length; depth++) categories.add(parts.slice(0, depth).join("/"));
  }
  return [...categories]
    .sort((a, b) => a.localeCompare(b, "zh", { numeric: true }));
}

export function inSkillCategory(category: string, selected: string): boolean {
  return !selected || category === selected || selected !== ROOT_CATEGORY && category.startsWith(selected + "/");
}
