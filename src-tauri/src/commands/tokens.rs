//! Token 上下文预算：估算每个 Agent 已启用技能的常驻上下文开销。
//!
//! Agent Skills 的约定是 SKILL.md 的 frontmatter（name + description）会常驻
//! Agent 上下文，正文按需加载。这里用经验系数估算：CJK 字符约 1.5 字符/token，
//! 其余（拉丁/空白/符号）约 4 字符/token，纯整数运算，零依赖。

use crate::models::{AgentDef, AgentTokenStats, AppConfig, SkillTokens};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// 单文件 token 估算缓存：SKILL.md 路径 → (文件 mtime, (resident, body))
fn token_cache() -> &'static crate::commands::cache::MtimeCache<(u64, u64)> {
    static CACHE: std::sync::OnceLock<crate::commands::cache::MtimeCache<(u64, u64)>> =
        std::sync::OnceLock::new();
    CACHE.get_or_init(|| std::sync::Mutex::new(HashMap::new()))
}

/// 估算文本的 token 数（中英混排）。
/// CJK 区间（含全角标点）按 2/3 字符每 token（≈1.5 字符/token），
/// 其余按 4 字符/token。
pub fn estimate_tokens(text: &str) -> u64 {
    let mut cjk = 0u64;
    let mut other = 0u64;
    for c in text.chars() {
        let u = c as u32;
        let is_cjk = (0x4e00..=0x9fff).contains(&u)
            || (0x3000..=0x303f).contains(&u)
            || (0xff00..=0xffef).contains(&u)
            || (0x3400..=0x4dbf).contains(&u);
        if is_cjk {
            cjk += 1;
        } else {
            other += 1;
        }
    }
    cjk * 2 / 3 + other / 4
}

/// frontmatter 常驻部分之外的结构开销（工具名、加载机制说明等，经验值）
const RESIDENT_OVERHEAD: u64 = 20;

/// 单技能的 token 估算（带文件级 mtime 缓存）
fn skill_tokens(path: &Path) -> (u64, u64) {
    let md_path = path.join("SKILL.md");
    let fingerprint = fs::metadata(&md_path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
        .unwrap_or(0);
    crate::commands::cache::cached(token_cache(), md_path.clone(), fingerprint, move || {
        let content = fs::read_to_string(&md_path).unwrap_or_default();
        let (name, display, desc, _, _) = crate::commands::scan::read_skill_md_pub(&md_path);
        let resident_meta = format!("{name} {display} {desc}");
        let resident = estimate_tokens(&resident_meta) + RESIDENT_OVERHEAD;
        (resident, estimate_tokens(&content))
    })
}

/// 计算单个 Agent 的 token 统计
pub fn agent_token_stats(agent: &AgentDef, cfg: &AppConfig) -> AgentTokenStats {
    let enabled: Vec<String> = cfg
        .agent_configs
        .get(&agent.id)
        .map(|c| c.enabled_skills.clone())
        .unwrap_or_default();
    let mut skills: Vec<SkillTokens> = Vec::new();
    for key in &enabled {
        let Some(path) = crate::commands::scan::resolve_skill_target(&cfg.skills_root, key) else {
            continue; // 失效技能不计入
        };
        let (resident, body) = skill_tokens(&path);
        let display = crate::commands::scan::read_skill_md_pub(&path.join("SKILL.md"));
        let name = if !display.1.is_empty() {
            display.1.clone()
        } else if !display.0.is_empty() {
            display.0.clone()
        } else {
            key.rsplit('/').next().unwrap_or(key).to_string()
        };
        skills.push(SkillTokens {
            skill_key: key.clone(),
            name,
            resident_tokens: resident,
            body_tokens: body,
        });
    }
    skills.sort_by_key(|item| std::cmp::Reverse(item.resident_tokens));
    let resident_tokens = skills.iter().map(|s| s.resident_tokens).sum();
    let body_tokens = skills.iter().map(|s| s.body_tokens).sum();
    let over_budget = cfg.token_budget > 0 && resident_tokens > cfg.token_budget;
    AgentTokenStats {
        agent_id: agent.id.clone(),
        agent_name: agent.name.clone(),
        enabled_count: enabled.len(),
        resident_tokens,
        body_tokens,
        budget: cfg.token_budget,
        over_budget,
        skills,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_english() {
        // 40 个英文字符 ≈ 10 tokens
        assert_eq!(estimate_tokens("abcdefghijabcdefghijabcdefghijabcdefghij"), 10);
    }

    #[test]
    fn estimate_chinese() {
        // 6 个汉字 → 6*2/3 = 4 tokens
        assert_eq!(estimate_tokens("技能管理工具"), 4);
    }

    #[test]
    fn estimate_mixed_and_empty() {
        assert_eq!(estimate_tokens(""), 0);
        // 3 汉字 + 4 字母 → 2 + 1 = 3
        assert_eq!(estimate_tokens("技能测试test"), 3);
    }

    #[test]
    fn over_budget_flag() {
        let cfg = AppConfig {
            skills_root: std::env::temp_dir().to_string_lossy().to_string(),
            token_budget: 8000,
            ..AppConfig::default()
        };
        let agent = AgentDef {
            id: "t".into(),
            name: "T".into(),
            skills_dir: std::env::temp_dir().to_string_lossy().to_string(),
            builtin: false,
        };
        // 空 enabled 列表，不超预算
        let st = agent_token_stats(&agent, &cfg);
        assert!(!st.over_budget);
        assert_eq!(st.resident_tokens, 0);
    }
}
