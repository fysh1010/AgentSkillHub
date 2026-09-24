use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 一个技能的元信息（来自扫描）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInfo {
    /// 技能目录名
    pub name: String,
    /// 分类（父目录名）
    pub category: String,
    /// 唯一标识："分类/技能名"
    pub key: String,
    /// 绝对路径
    pub path: String,
    /// frontmatter 中的 description
    pub description: String,
    /// 是否有 scripts 目录
    pub has_scripts: bool,
    pub size_bytes: u64,
    pub modified_ts: u64,
    /// SKILL.md 存在且 frontmatter 合法
    pub healthy: bool,
    /// 技能自带的官方图标（data URL，如 data:image/png;base64,…），空表示没有
    #[serde(default)]
    pub icon_b64: String,
    /// 版本号（frontmatter version / metadata.version），空表示未知
    #[serde(default)]
    pub version: String,
}

/// 用户对某个技能的个性化标注（收藏 / 标签）
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillUserMeta {
    #[serde(default)]
    pub starred: bool,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// 技能目录里的一个文件条目
#[derive(Debug, Clone, Serialize)]
pub struct FileEntry {
    pub name: String,
    pub size_bytes: u64,
    pub is_dir: bool,
    /// 可执行脚本 / 钩子等需要留意的文件
    pub risky: bool,
}

/// 技能详情（SKILL.md 全文 + 文件清单）
#[derive(Debug, Clone, Serialize)]
pub struct SkillDetail {
    pub key: String,
    /// SKILL.md 全文（超长截断到 200KB）
    pub content: String,
    pub files: Vec<FileEntry>,
    /// 风险提示（如「包含可执行脚本 scripts/」）
    pub risks: Vec<String>,
}

/// 应用内回收站里的一条记录
#[derive(Debug, Clone, Serialize)]
pub struct TrashItem {
    /// 回收站里的目录名（还原时用）
    pub name: String,
    /// 原技能 key
    pub key: String,
    pub size_bytes: u64,
    /// 删除时间（秒级时间戳）
    pub deleted_ts: u64,
    pub path: String,
}

/// Agent 定义（注册表）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentDef {
    pub id: String,
    pub name: String,
    pub skills_dir: String,
    pub builtin: bool,
}

/// 本机检测到、但尚未注册的 Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedAgent {
    pub id: String,
    pub name: String,
    pub skills_dir: String,
}

/// 技能市场源（clawhub 社区市场 / 任意 GitHub 技能仓库）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketSource {
    /// 唯一 ID："clawhub" 或 "github:{owner}/{repo}"
    pub id: String,
    /// "clawhub" | "github"
    pub kind: String,
    /// 显示名
    pub name: String,
    /// github 源专用："owner/repo"
    #[serde(default)]
    pub repo: String,
    /// github 源专用：仓库内技能所在子路径（根为空）
    #[serde(default)]
    pub subpath: String,
}

/// 市场里的一个技能（列表项）
#[derive(Debug, Clone, Serialize)]
pub struct MarketSkill {
    /// 技能 slug / 目录名
    pub name: String,
    pub display_name: String,
    pub description: String,
    /// clawhub：安装量；github/skillhub：0
    pub downloads: u64,
    /// 更新时间（秒级时间戳，0 表示未知）
    pub updated_ts: u64,
    /// 技能库已存在同名技能
    pub installed: bool,
    /// 安装引用：clawhub 为 slug；github 为仓库内路径；skillhub 为 publicSlug
    pub reference: String,
    /// clawhub：ownerHandle（可为空）；github：owner；skillhub：namespace handle
    pub owner: String,
    /// 图标：lucide 图标名（如 "lucide:FileText"）或空
    #[serde(default)]
    pub icon: String,
    /// 版本号（如 "v1.0.0"），空表示未知
    #[serde(default)]
    pub version: String,
    /// 分类 / 主题标签
    #[serde(default)]
    pub topics: Vec<String>,
    /// 是否社区精选
    #[serde(default)]
    pub featured: bool,
    /// 作者头像 URL
    #[serde(default)]
    pub owner_image: String,
    /// 来源类型：clawhub / github / skillhub
    #[serde(default)]
    pub source_kind: String,
    /// 安全扫描状态：pass / warn / ""（未知）
    #[serde(default)]
    pub security: String,
    /// 官方技能图标 URL（腾讯 SkillHub 提供；ClawHub 该字段为空）
    #[serde(default)]
    pub icon_url: String,
    /// 官方分类 key（如 "office-efficiency"），由前端映射为中文标签
    #[serde(default)]
    pub category: String,
    /// 收藏 / star 数
    #[serde(default)]
    pub stars: u64,
}

/// Agent 目录里的外部（实体）技能
#[derive(Debug, Clone, Serialize)]
pub struct ExternalSkill {
    pub dir_name: String,
    pub name: String,
    pub description: String,
    pub size_bytes: u64,
    pub healthy: bool,
    pub path: String,
    /// 点开头目录（.system 等）——工具自带的系统技能，仅展示与收编，不做链接部署
    #[serde(default)]
    pub system: bool,
}

/// 单个 Agent 的技能配置
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentConfig {
    /// 启用的技能 key 列表（"分类/技能名"）
    pub enabled_skills: Vec<String>,
}

/// 一次部署动作（实际执行或将要执行）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployAction {
    /// create_link / remove_link / remove_parent_link / create_dir / skip
    pub kind: String,
    pub skill_key: String,
    pub link_path: String,
    pub target: String,
    pub ok: bool,
    pub message: String,
}

/// 差异条目（期望 vs 实际）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffItem {
    /// to_create / to_remove / keep / external / expand_parent_link
    pub kind: String,
    pub skill_key: String,
    pub link_path: String,
    pub current_target: Option<String>,
    pub expected_target: Option<String>,
}

/// 审计条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub id: String,
    pub ts: u64,
    pub action: String,
    pub agent_id: String,
    pub detail: String,
    pub actions: Vec<DeployAction>,
}

/// Agent 当前状态（用于总览红绿灯）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent: AgentDef,
    pub dir_ok: bool,
    pub is_parent_link: bool,
    pub enabled_count: usize,
    pub linked_count: usize,
    /// 整目录链接（父链接）下可见的技能子目录数
    pub parent_visible: usize,
    pub external_count: usize,
    pub drift: bool,
    pub message: String,
}

/// 部署结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployResult {
    pub ok: bool,
    pub actions: Vec<DeployAction>,
    pub summary: String,
}

/// 从技能库删除一个技能的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeleteResult {
    /// 被删除的技能 key
    pub skill_key: String,
    /// 被删除的技能目录绝对路径
    pub path: String,
    /// 是否已成功移入系统回收站
    pub recycled: bool,
    /// 已清理的 Agent 链接
    pub removed_links: Vec<DeployAction>,
    /// 已从配置中摘除该技能的 Agent 名称
    pub unassigned_agents: Vec<String>,
    /// 未能清理的链接 / 告警
    pub warnings: Vec<String>,
    /// 结果说明
    pub message: String,
}

// ================= 安全扫描 =================

/// 一条内容级安全命中
#[derive(Debug, Clone, Serialize)]
pub struct SecurityFinding {
    /// 相对技能目录的路径（如 scripts/install.sh）
    pub file: String,
    /// 行号；文件级命中（如 .pem 文件）为 0
    pub line: u32,
    /// exec / network / credential / obfuscation
    pub category: String,
    /// high / medium / low
    pub level: String,
    /// 命中模式说明（中文）
    pub pattern: String,
    /// 命中片段（trim 后截断）
    pub snippet: String,
}

/// 单技能安全扫描报告
#[derive(Debug, Clone, Serialize)]
pub struct SecurityReport {
    pub skill_key: String,
    /// 总体级别：high / medium / low / none
    pub level: String,
    pub findings: Vec<SecurityFinding>,
    pub scanned_files: u32,
}

/// 技能列表 badge 用的轻量汇总
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SecuritySummaryItem {
    pub skill_key: String,
    pub level: String,
    pub high: u32,
    pub medium: u32,
    pub low: u32,
}

// ================= Doctor 体检 =================

/// 一条体检问题
#[derive(Debug, Clone, Serialize)]
pub struct DoctorIssue {
    /// 稳定 ID："kind|agent_id|path"（修复时原样回传）
    pub id: String,
    /// broken_link / wrong_link / drift / stale_config / orphan_dir / unhealthy_skill
    pub kind: String,
    /// 库级问题为 ""
    pub agent_id: String,
    pub agent_name: String,
    pub path: String,
    pub skill_key: String,
    /// 中文说明
    pub detail: String,
    pub fixable: bool,
}

/// 体检修复结果
#[derive(Debug, Clone, Serialize)]
pub struct DoctorFixResult {
    pub fixed: u32,
    pub failed: u32,
    pub messages: Vec<String>,
}

// ================= Token 上下文预算 =================

/// 单个技能的 token 估算
#[derive(Debug, Clone, Serialize)]
pub struct SkillTokens {
    pub skill_key: String,
    pub name: String,
    /// 常驻开销（frontmatter name+display_name+description）
    pub resident_tokens: u64,
    /// SKILL.md 全文开销（加载后参考）
    pub body_tokens: u64,
}

/// 单个 Agent 的 token 统计
#[derive(Debug, Clone, Serialize)]
pub struct AgentTokenStats {
    pub agent_id: String,
    pub agent_name: String,
    pub enabled_count: usize,
    pub resident_tokens: u64,
    pub body_tokens: u64,
    pub budget: u64,
    pub over_budget: bool,
    /// 按 resident 降序，便于定位大户
    pub skills: Vec<SkillTokens>,
}

// ================= 环境清单导出/导入 =================

/// 环境清单（分配关系的可迁移快照，不含技能文件与审计日志）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    pub exported_ts: u64,
    pub app_version: String,
    pub skills_root: String,
    pub agents: Vec<AgentDef>,
    pub agent_configs: HashMap<String, AgentConfig>,
    #[serde(default)]
    pub skill_meta: HashMap<String, SkillUserMeta>,
}

/// 清单导入结果
#[derive(Debug, Clone, Serialize)]
pub struct ManifestImportResult {
    pub ok: bool,
    pub message: String,
    /// 导入后指向缺失技能的分配条数（前端显示为"已失效"）
    pub missing_skills: u32,
}

/// 全局配置（持久化到 app_config_dir/config.json）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub skills_root: String,
    pub agents: Vec<AgentDef>,
    pub agent_configs: HashMap<String, AgentConfig>,
    pub audit: Vec<AuditEntry>,
    /// 技能市场源列表
    #[serde(default)]
    pub market_sources: Vec<MarketSource>,
    /// 用户对技能的个性化标注（收藏 / 标签），key 为技能 key
    #[serde(default)]
    pub skill_meta: HashMap<String, SkillUserMeta>,
    /// 每个 Agent 启用技能的常驻上下文预算（token 数）
    #[serde(default = "default_token_budget")]
    pub token_budget: u64,
}

fn default_token_budget() -> u64 {
    8000
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            skills_root: String::new(),
            agents: Vec::new(),
            agent_configs: HashMap::new(),
            audit: Vec::new(),
            market_sources: Vec::new(),
            skill_meta: HashMap::new(),
            token_budget: default_token_budget(),
        }
    }
}

/// 一次同步拉取的完整数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncResult {
    pub skills: Vec<SkillInfo>,
    pub agents: Vec<AgentStatus>,
    pub config: AppConfig,
}
