//! 各家 AI Agent 产品的技能目录约定清单（内置，精选常用产品）
//!
//! 依据：各产品官方文档的 skill 加载路径、vercel-labs/skills 的跨 Agent 路径表，
//! 以及本机实测（用户目录下真实存在 skills 子目录的产品）。
//! 只收录「全局/用户级」位置；项目内目录（`.claude/skills` 等）随仓库走，不在此列。

use std::path::{Path, PathBuf};

/// 路径锚点
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Root {
    /// 用户主目录（Windows `%USERPROFILE%`，其他系统 `$HOME`）
    Home,
    /// 产品数据目录 `%LOCALAPPDATA%`（仅 Windows，如豆包桌面版）
    LocalAppData,
    AppData,
}

/// 一个已知产品的技能目录模板
#[derive(Debug, Clone, Copy)]
pub struct KnownAgent {
    pub id: &'static str,
    pub name: &'static str,
    pub root: Root,
    /// 相对锚点的路径段，可含 `*` 通配段（逐层展开，用于按市场/插件/版本或账号分目录的产品）
    pub rel: &'static [&'static str],
}

/// 解析结果：产品 + 实际路径 + 通配段命中的实例名
pub struct Hit {
    pub agent: &'static KnownAgent,
    pub path: PathBuf,
    /// `*` 命中的目录名（多段用 `/` 连接）；无通配时为 None
    pub instance: Option<String>,
}

impl Hit {
    /// 注册表 id：通配实例需区分，避免同一产品多账号撞 id
    pub fn agent_id(&self) -> String {
        match &self.instance {
            Some(seg) => {
                let safe = sanitize(seg);
                if safe == *seg {
                    format!("{}-{}", self.agent.id, safe)
                } else {
                    let hash = seg.bytes().fold(0x811c9dc5u32, |h, b| {
                        (h ^ b as u32).wrapping_mul(0x01000193)
                    });
                    format!("{}-{}-{hash:08x}", self.agent.id, safe)
                }
            }
            None => self.agent.id.to_string(),
        }
    }

    pub fn display_name(&self) -> String {
        match &self.instance {
            Some(seg) => format!("{} · {}", self.agent.name, seg),
            None => self.agent.name.to_string(),
        }
    }
}

fn sanitize(seg: &str) -> String {
    seg.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect()
}

/// 单个模板最多展开出多少个路径（防止通配命中海量目录）
const MAX_HITS_PER_TEMPLATE: usize = 128;

impl KnownAgent {
    fn resolve(&'static self, bases: &Bases) -> Vec<Hit> {
        let Some(root) = bases.anchor(self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        self.walk(root, self.rel, &mut Vec::new(), &mut out);
        out
    }

    /// 逐段下钻；`*` 段展开为该层实际存在的子目录（名称倒序，新版本/靠后的排前）
    fn walk(
        &'static self,
        cur: PathBuf,
        rest: &'static [&'static str],
        inst: &mut Vec<String>,
        out: &mut Vec<Hit>,
    ) {
        if out.len() >= MAX_HITS_PER_TEMPLATE {
            return;
        }
        let Some((seg, tail)) = rest.split_first() else {
            out.push(Hit {
                agent: self,
                path: cur,
                instance: if inst.is_empty() {
                    None
                } else {
                    Some(inst.join("/"))
                },
            });
            return;
        };
        if *seg != "*" {
            self.walk(cur.join(seg), tail, inst, out);
            return;
        }
        let Ok(rd) = std::fs::read_dir(&cur) else {
            return;
        };
        let mut subs: Vec<String> = Vec::new();
        for e in rd.flatten() {
            let s = e.file_name().to_string_lossy().to_string();
            if !s.starts_with('.') && e.path().is_dir() {
                subs.push(s);
            }
        }
        subs.sort();
        subs.reverse();
        for s in subs {
            inst.push(s.clone());
            self.walk(cur.join(&s), tail, inst, out);
            inst.pop();
            if out.len() >= MAX_HITS_PER_TEMPLATE {
                break;
            }
        }
    }
}

/// 各锚点的实际根目录（由调用方解析环境后构造，见 commands::agent_bases）
#[derive(Debug, Clone)]
pub struct Bases {
    pub home: PathBuf,
    pub local_appdata: Option<PathBuf>,
    pub appdata: Option<PathBuf>,
}

impl Bases {
    fn anchor(&self, root: Root) -> Option<PathBuf> {
        match root {
            Root::Home => Some(self.home.clone()),
            Root::LocalAppData => self.local_appdata.clone(),
            Root::AppData => self.appdata.clone(),
        }
    }
}

fn scan(list: &'static [KnownAgent], bases: &Bases, require_content: bool) -> Vec<Hit> {
    let mut out: Vec<Hit> = Vec::new();
    for a in list {
        for h in a.resolve(bases) {
            let dup = out.iter().any(|o: &Hit| eq_path(&o.path, &h.path));
            if dup || !h.path.is_dir() {
                continue;
            }
            if require_content && !has_entries(&h.path) {
                continue;
            }
            out.push(h);
        }
    }
    out
}

fn has_entries(dir: &Path) -> bool {
    match std::fs::read_dir(dir) {
        Ok(mut rd) => rd.next().is_some(),
        Err(_) => false,
    }
}

/// 本机已存在的「可部署 Agent」技能目录
pub fn detect_existing(bases: &Bases) -> Vec<Hit> {
    scan(KNOWN_AGENTS, bases, false)
}

/// 本机已存在的「工具内置技能来源」（只读，不作为可部署 Agent 注册）
pub fn detect_sources(bases: &Bases) -> Vec<Hit> {
    scan(KNOWN_SOURCES, bases, true)
}

/// 按 id 取某个内置技能来源的实际目录（仅在本机存在时可得）
#[cfg(test)]
pub fn source_dir(bases: &Bases, id: &str) -> Option<PathBuf> {
    detect_sources(bases)
        .into_iter()
        .find(|h| h.agent_id() == id)
        .map(|h| h.path)
}

pub fn eq_path(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
        == b.to_string_lossy()
            .replace('/', "\\")
            .trim_end_matches('\\')
            .to_lowercase()
}

/// 已知产品的模板路径（不检查是否存在），供默认注册表使用
pub fn template_path(bases: &Bases, id: &str) -> Option<String> {
    let a = KNOWN_AGENTS.iter().find(|a| a.id == id)?;
    if a.rel.contains(&"*") {
        return None;
    }
    let mut p = bases.anchor(a.root)?;
    for seg in a.rel {
        p = p.join(seg);
    }
    Some(p.to_string_lossy().to_string())
}

/// id → 友好显示名（供通用扫描兜底命名用）
pub fn name_for(id: &str) -> Option<&'static str> {
    KNOWN_AGENTS.iter().find(|a| a.id == id).map(|a| a.name)
}

/// 内置清单（精选常用产品，非穷举；未收录的由通用 ~\.xxx\skills 扫描兜底）。
/// id 与产品在自己主目录下的点目录名保持一致，便于和通用扫描结果对齐。
pub const KNOWN_AGENTS: &[KnownAgent] = &[
    // ---------- 通用共享约定 ----------
    KnownAgent {
        id: "agents",
        name: "Agents 通用目录",
        root: Root::Home,
        rel: &[".agents", "skills"],
    },
    KnownAgent {
        id: "config-agents",
        name: "Agents 通用目录 (.config)",
        root: Root::Home,
        rel: &[".config", "agents", "skills"],
    },
    // ---------- 国际 ----------
    KnownAgent {
        id: "cline",
        name: "Cline",
        root: Root::Home,
        rel: &[".cline", "skills"],
    },
    KnownAgent {
        id: "antigravity-cli",
        name: "Antigravity CLI",
        root: Root::Home,
        rel: &[".gemini", "antigravity-cli", "skills"],
    },
    KnownAgent {
        id: "autohand-code",
        name: "Autohand Code CLI",
        root: Root::Home,
        rel: &[".autohand", "skills"],
    },
    KnownAgent {
        id: "hermes-agent",
        name: "Hermes Agent",
        root: Root::Home,
        rel: &[".hermes", "skills"],
    },
    KnownAgent {
        id: "grok",
        name: "Grok Build",
        root: Root::Home,
        rel: &[".grok", "skills"],
    },
    KnownAgent {
        id: "codemaker",
        name: "Codemaker",
        root: Root::Home,
        rel: &[".codemaker", "skills"],
    },
    KnownAgent {
        id: "codestudio",
        name: "Code Studio",
        root: Root::Home,
        rel: &[".codestudio", "skills"],
    },
    KnownAgent {
        id: "firebender",
        name: "Firebender",
        root: Root::Home,
        rel: &[".firebender", "skills"],
    },
    KnownAgent {
        id: "jazz",
        name: "Jazz",
        root: Root::Home,
        rel: &[".jazz", "skills"],
    },
    KnownAgent {
        id: "inference-sh",
        name: "inference.sh",
        root: Root::Home,
        rel: &[".inferencesh", "skills"],
    },
    KnownAgent {
        id: "claude",
        name: "Claude Code",
        root: Root::Home,
        rel: &[".claude", "skills"],
    },
    KnownAgent {
        id: "codex",
        name: "OpenAI Codex",
        root: Root::Home,
        rel: &[".codex", "skills"],
    },
    KnownAgent {
        id: "cursor",
        name: "Cursor",
        root: Root::Home,
        rel: &[".cursor", "skills"],
    },
    KnownAgent {
        id: "windsurf",
        name: "Windsurf",
        root: Root::Home,
        rel: &[".codeium", "windsurf", "skills"],
    },
    KnownAgent {
        id: "gemini",
        name: "Gemini CLI",
        root: Root::Home,
        rel: &[".gemini", "skills"],
    },
    KnownAgent {
        id: "antigravity",
        name: "Google Antigravity",
        root: Root::Home,
        rel: &[".gemini", "antigravity", "skills"],
    },
    KnownAgent {
        id: "copilot",
        name: "GitHub Copilot",
        root: Root::Home,
        rel: &[".copilot", "skills"],
    },
    KnownAgent {
        id: "opencode",
        name: "OpenCode",
        root: Root::Home,
        rel: &[".config", "opencode", "skills"],
    },
    KnownAgent {
        id: "crush",
        name: "Charm Crush",
        root: Root::Home,
        rel: &[".config", "crush", "skills"],
    },
    KnownAgent {
        id: "goose",
        name: "Block Goose",
        root: Root::Home,
        rel: &[".config", "goose", "skills"],
    },
    KnownAgent {
        id: "devin",
        name: "Devin for Terminal",
        root: Root::Home,
        rel: &[".config", "devin", "skills"],
    },
    KnownAgent {
        id: "iflow",
        name: "iFlow CLI",
        root: Root::Home,
        rel: &[".iflow", "skills"],
    },
    KnownAgent {
        id: "qwen",
        name: "Qwen Code",
        root: Root::Home,
        rel: &[".qwen", "skills"],
    },
    KnownAgent {
        id: "roo",
        name: "Roo Code",
        root: Root::Home,
        rel: &[".roo", "skills"],
    },
    KnownAgent {
        id: "kilo",
        name: "Kilo Code",
        root: Root::Home,
        rel: &[".kilo", "skills"],
    },
    KnownAgent {
        id: "kiro",
        name: "Kiro CLI",
        root: Root::Home,
        rel: &[".kiro", "skills"],
    },
    KnownAgent {
        id: "continue",
        name: "Continue",
        root: Root::Home,
        rel: &[".continue", "skills"],
    },
    KnownAgent {
        id: "augment",
        name: "Augment Code",
        root: Root::Home,
        rel: &[".augment", "skills"],
    },
    KnownAgent {
        id: "openhands",
        name: "OpenHands",
        root: Root::Home,
        rel: &[".openhands", "skills"],
    },
    KnownAgent {
        id: "junie",
        name: "JetBrains Junie",
        root: Root::Home,
        rel: &[".junie", "skills"],
    },
    KnownAgent {
        id: "tabnine",
        name: "Tabnine CLI",
        root: Root::Home,
        rel: &[".tabnine", "agent", "skills"],
    },
    KnownAgent {
        id: "cortex",
        name: "Snowflake Cortex Code",
        root: Root::Home,
        rel: &[".snowflake", "cortex", "skills"],
    },
    KnownAgent {
        id: "zencoder",
        name: "Zencoder",
        root: Root::Home,
        rel: &[".zencoder", "skills"],
    },
    KnownAgent {
        id: "aider-desk",
        name: "AiderDesk",
        root: Root::Home,
        rel: &[".aider-desk", "skills"],
    },
    KnownAgent {
        id: "forge",
        name: "ForgeCode",
        root: Root::Home,
        rel: &[".forge", "skills"],
    },
    KnownAgent {
        id: "mcpjam",
        name: "MCPJam",
        root: Root::Home,
        rel: &[".mcpjam", "skills"],
    },
    KnownAgent {
        id: "mux",
        name: "Mux",
        root: Root::Home,
        rel: &[".mux", "skills"],
    },
    KnownAgent {
        id: "rovodev",
        name: "Rovo Dev",
        root: Root::Home,
        rel: &[".rovodev", "skills"],
    },
    KnownAgent {
        id: "neovate",
        name: "Neovate",
        root: Root::Home,
        rel: &[".neovate", "skills"],
    },
    KnownAgent {
        id: "pochi",
        name: "Pochi",
        root: Root::Home,
        rel: &[".pochi", "skills"],
    },
    KnownAgent {
        id: "deepagents",
        name: "Deep Agents",
        root: Root::Home,
        rel: &[".deepagents", "agent", "skills"],
    },
    KnownAgent {
        id: "pi",
        name: "Pi",
        root: Root::Home,
        rel: &[".pi", "agent", "skills"],
    },
    KnownAgent {
        id: "posit",
        name: "Posit Assistant",
        root: Root::Home,
        rel: &[".posit", "assistant", "skills"],
    },
    KnownAgent {
        id: "astrbot",
        name: "AstrBot",
        root: Root::Home,
        rel: &[".astrbot", "data", "skills"],
    },
    KnownAgent {
        id: "openclaw",
        name: "OpenClaw",
        root: Root::Home,
        rel: &[".openclaw", "skills"],
    },
    KnownAgent {
        id: "factory",
        name: "Factory Droid",
        root: Root::Home,
        rel: &[".factory", "skills"],
    },
    KnownAgent {
        id: "vibe",
        name: "Mistral Vibe",
        root: Root::Home,
        rel: &[".vibe", "skills"],
    },
    KnownAgent {
        id: "kimchi",
        name: "Kimchi",
        root: Root::Home,
        rel: &[".config", "kimchi", "harness", "skills"],
    },
    // ---------- 国内 ----------
    KnownAgent {
        id: "qoder",
        name: "Qoder",
        root: Root::Home,
        rel: &[".qoder", "skills"],
    },
    KnownAgent {
        id: "qoder-cn",
        name: "Qoder CN",
        root: Root::Home,
        rel: &[".qoder-cn", "skills"],
    },
    KnownAgent {
        id: "qoder-cli",
        name: "Qoder CLI",
        root: Root::Home,
        rel: &[".qoder", "skills"],
    },
    KnownAgent {
        id: "lingxi",
        name: "WPS 灵犀",
        root: Root::AppData,
        rel: &["WPS 灵犀", "serverdir", "user_skills"],
    },
    KnownAgent {
        id: "trae",
        name: "Trae",
        root: Root::Home,
        rel: &[".trae", "skills"],
    },
    KnownAgent {
        id: "trae-cn",
        name: "Trae CN",
        root: Root::Home,
        rel: &[".trae-cn", "skills"],
    },
    KnownAgent {
        id: "zcode",
        name: "Zcode",
        root: Root::Home,
        rel: &[".zcode", "skills"],
    },
    KnownAgent {
        id: "workbuddy",
        name: "WorkBuddy",
        root: Root::Home,
        rel: &[".workbuddy", "skills"],
    },
    KnownAgent {
        id: "lingma",
        name: "通义灵码 Lingma",
        root: Root::Home,
        rel: &[".lingma", "skills"],
    },
    KnownAgent {
        id: "codebuddy",
        name: "腾讯 CodeBuddy",
        root: Root::Home,
        rel: &[".codebuddy", "skills"],
    },
    KnownAgent {
        id: "minimax",
        name: "MiniMax Code",
        root: Root::Home,
        rel: &[".minimax", "skills"],
    },
    KnownAgent {
        id: "kimi-code",
        name: "Kimi Code",
        root: Root::Home,
        rel: &[".kimi-code", "skills"],
    },
    KnownAgent {
        id: "codeartsdoer",
        name: "华为 CodeArts Agent",
        root: Root::Home,
        rel: &[".codeartsdoer", "skills"],
    },
    KnownAgent {
        id: "doubao",
        name: "豆包 Doubao",
        root: Root::LocalAppData,
        rel: &[
            "Doubao",
            "User Data",
            "Default",
            ".doubao",
            "agent_mode",
            "workspace",
            ".user_skills",
        ],
    },
    KnownAgent {
        id: "meituan-catpaw",
        name: "Meituan CatPaw",
        root: Root::Home,
        rel: &[".meituan-catpaw", "*", "skills"],
    },
];

/// 工具「自带技能」的位置：插件市场缓存、IDE 内置技能包等。
/// 这些目录由对应工具自己写入与升级，本工具只读浏览 + 收编副本，绝不往里做链接部署。
pub const KNOWN_SOURCES: &[KnownAgent] = &[
    KnownAgent { id: "workbuddy-system", name: "WorkBuddy 系统技能", root: Root::Home, rel: &[".workbuddy", "skills", ".system"] },
    KnownAgent { id: "codex-system", name: "Codex 系统技能", root: Root::Home, rel: &[".codex", "skills", ".system"] },
    KnownAgent { id: "lingxi-builtin", name: "WPS 灵犀系统技能", root: Root::AppData, rel: &["WPS 灵犀", "serverdir", "official_skills"] },
];

#[cfg(test)]
mod tests {
    use super::*;

    use crate::commands::test_scratch;

    fn bases(home: &Path) -> Bases {
        Bases {
            home: home.to_path_buf(),
            local_appdata: None,
            appdata: None,
        }
    }

    fn mk(root: &Path, rel: &str) {
        std::fs::create_dir_all(root.join(rel)).unwrap();
    }

    fn ids(hits: &[Hit]) -> Vec<String> {
        hits.iter().map(|h| h.agent_id()).collect()
    }

    #[test]
    fn ids_unique_and_paths_wellformed() {
        let mut seen: Vec<&str> = Vec::new();
        let mut check = |list: &'static [KnownAgent]| {
            for a in list {
                assert!(!seen.contains(&a.id), "重复 id: {}", a.id);
                seen.push(a.id);
                assert!(
                    !a.rel.is_empty() && a.rel.iter().all(|s| !s.is_empty()),
                    "{} 路径模板为空段",
                    a.id
                );
                if a.root == Root::Home && !a.id.ends_with("system") {
                    assert_eq!(
                        a.rel.last().copied(),
                        Some("skills"),
                        "{} 末段应为 skills",
                        a.id
                    );
                }
            }
        };
        check(KNOWN_AGENTS);
        check(KNOWN_SOURCES);
        assert!(KNOWN_AGENTS.len() > 40);
        assert_eq!(KNOWN_SOURCES.len(), 3);
    }

    #[test]
    fn plugin_caches_are_not_automatic_skill_sources() {
        let root = test_scratch("catalog_no_plugin_sources");
        mk(&root, ".qoder-cn/plugins/cache/market/plugin/1.0.0/skills/example");
        mk(&root, ".workbuddy/plugins/cache/market/plugin/1.0.0/skills/example");
        mk(&root, ".workbuddy/skills/.system/example");
        let hits = detect_sources(&bases(&root));
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].agent.id, "workbuddy-system");
        assert!(source_dir(&bases(&root), "qoder-cn-plugins").is_none());
    }

    #[test]
    fn source_requires_non_empty_dir() {
        let root = test_scratch("catalog_src_empty");
        mk(
            &root,
            ".qoder-cn/plugins/cache/mkt/empty-plugin/1.0.0/skills",
        ); // 有目录但无技能
        assert!(
            detect_sources(&bases(&root)).is_empty(),
            "空技能目录不应作为来源报出"
        );
    }

    #[test]
    fn detects_only_existing_dirs() {
        let root = test_scratch("catalog");
        mk(&root, ".claude/skills");
        mk(&root, ".codex"); // 有产品目录但没有 skills，不应命中
        for acct in ["alice", "bob"] {
            mk(&root, &format!(".meituan-catpaw/{acct}/skills"));
        }
        mk(&root, ".meituan-catpaw/agent-host"); // 无 skills 子目录的实例

        let hits = detect_existing(&bases(&root));
        let got = ids(&hits);
        assert!(got.contains(&"claude".to_string()));
        assert!(!got.contains(&"codex".to_string()), "空目录不应命中");
        assert!(
            got.contains(&"meituan-catpaw-alice".to_string())
                && got.contains(&"meituan-catpaw-bob".to_string()),
            "通配应展开出两个账号，且 id 互不冲突: {got:?}"
        );
        assert!(
            !got.iter()
                .any(|i| i.starts_with("meituan-catpaw-agent-host")),
            "没有 skills 子目录的实例不应命中"
        );
        let mut sorted = got.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), got.len(), "展开后 id 仍需唯一");
    }

    #[test]
    fn local_appdata_anchor_is_optional() {
        let root = test_scratch("catalog_local");
        mk(&root, ".claude/skills");
        mk(
            &root,
            "Doubao/User Data/Default/.doubao/agent_mode/workspace/.user_skills",
        );

        // 没有 LOCALAPPDATA 锚点时不报豆包
        assert!(!ids(&detect_existing(&bases(&root))).contains(&"doubao".to_string()));

        let with_local = Bases {
            home: root.clone(),
            local_appdata: Some(root.clone()),
            appdata: None,
        };
        assert!(ids(&detect_existing(&with_local)).contains(&"doubao".to_string()));
    }

    #[test]
    fn template_path_skips_wildcard_entries() {
        let root = test_scratch("catalog_tpl");
        let b = bases(&root);
        assert_eq!(
            template_path(&b, "claude"),
            Some(
                root.join(".claude")
                    .join("skills")
                    .to_string_lossy()
                    .to_string()
            )
        );
        assert!(
            template_path(&b, "meituan-catpaw").is_none(),
            "通配模板不能当路径用"
        );
        assert!(template_path(&b, "nope").is_none());
        assert_eq!(name_for("claude"), Some("Claude Code"));
    }

    #[test]
    fn name_lookup_covers_catalog() {
        assert_eq!(name_for("trae-cn"), Some("Trae CN"));
        assert_eq!(name_for("qoder-cn"), Some("Qoder CN"));
        assert_eq!(name_for("definitely-not-an-agent"), None);
    }
    #[test]
    fn unicode_and_punctuation_instances_do_not_collide() {
        let make = |name: &str| {
            Hit {
                agent: &KNOWN_AGENTS[0],
                path: PathBuf::new(),
                instance: Some(name.into()),
            }
            .agent_id()
        };
        assert_ne!(make("张三"), make("李四"));
        assert_ne!(make("a.b"), make("a-b"));
        assert_ne!(make("market/plugin/1.0"), make("market-plugin-1-0"));
    }
}
