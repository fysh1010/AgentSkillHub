import assert from "node:assert/strict";
import { categoryLabel, inSkillCategory, ROOT_CATEGORY, skillCategories } from "../src/utils/skillCategories.ts";

const skills = [
  { category: "包一层/01-开发工程/前端" },
  { category: "包一层/01-开发工程/后端" },
  { category: "包一层/01-开发工程工具" },
  { category: ROOT_CATEGORY },
];
assert.deepEqual(skillCategories(skills), [
  "包一层", "包一层/01-开发工程", "包一层/01-开发工程/后端",
  "包一层/01-开发工程/前端", "包一层/01-开发工程工具",
]);
assert.equal(categoryLabel("包一层/01-开发工程/前端"), "包一层 / 开发工程 / 前端");
assert.equal(categoryLabel(ROOT_CATEGORY), "未分类");
assert.equal(inSkillCategory(skills[0].category, "包一层"), true);
assert.equal(inSkillCategory(skills[0].category, "包一层/01-开发工程"), true);
assert.equal(inSkillCategory(skills[2].category, "包一层/01-开发工程"), false);
assert.equal(inSkillCategory(ROOT_CATEGORY, "包一层"), false);
assert.equal(inSkillCategory(ROOT_CATEGORY, ROOT_CATEGORY), true);
assert.equal(inSkillCategory(skills[0].category, ""), true);
console.log("技能分类层级、父目录筛选与路径边界检查通过");
