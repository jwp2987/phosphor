//! BYOP system prompt 模板渲染。
//!
//! 把 warp 客户端已经收集好的 `AIAgentContext`(env / git / skills / project_rules / current_time)
//! 渲染为 OpenAI 兼容 endpoint 的 `system` message 字符串。
//!
//! ## 工作流
//!
//! 1. 从 `params.input` 抽出最近一条 `UserQuery.context: Arc<[AIAgentContext]>`
//!    (warp `convert_to.rs::convert_input` 取的也是同一份)
//! 2. `collect_prompt_context` 把每个 enum variant 拍成扁平 `PromptContext` struct
//! 3. `pick_template` 按 model id 子串匹配选 `system/{anthropic,gpt,beast,codex,
//!    gemini,kimi,trinity,default}.j2`(对齐 opencode
//!    `packages/opencode/src/session/system.ts::provider`)
//! 4. minijinja 渲染
//!
//! ## 模板加载
//!
//! 全部模板 `include_str!` 编进二进制(零运行时 IO),改模板需重编。
//!
//! 例外:设了 `ZAP_PROMPT_DIR`(或 设置 → AI → Prompt template directory)后,
//! 改为从该目录按名字重读,存盘即生效,不用重编。缺文件 / 语法错逐个回退内置
//! 版本。详见 [`PROMPT_DIR_ENV`]。
//!
//! 可覆盖的东西分两张表:
//! - [`EMBEDDED`] —— 走 minijinja 的 `.j2` 模板(system prompt 及其 partials)。
//!   有 mtime 缓存,没改过就复用已解析的 Environment。
//! - [`EMBEDDED_RAW`] —— 直接喂给模型的纯文本(13 个 tool description +
//!   会话标题 prompt)。**不过** minijinja,详见该常量的注释。

use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::SystemTime;

use ai::LLMId;
use chrono::Local;
use minijinja::{Environment, Value};
use serde::Serialize;

use crate::ai::agent::AIAgentContext;
use crate::ai::execution_profiles::PromptSource;
use crate::settings::AgentProviderApiType;
// ---------------------------------------------------------------------------

static ENV: OnceLock<Environment<'static>> = OnceLock::new();

/// 模板热加载覆盖目录的环境变量名。
///
/// 不设 → 走 [`ENV`] 里 `include_str!` 编进二进制的那份,零运行时 IO(默认行为不变)。
/// 设了 → 每次渲染都从该目录按模板名(`system/local.j2` 这种相对路径)重读,
/// 改完模板存盘即生效,不用重编 `app` crate(80w 行,改一行提示词也要全量重链)。
///
/// 目录下缺文件 / 读失败 / 语法错都**逐个回退到内置版本**,不 panic —— 热加载是
/// 开发期便利,不该让一次手滑的模板编辑打断正在进行的会话。
const PROMPT_DIR_ENV: &str = "ZAP_PROMPT_DIR";

/// 全部模板的 (名字, 内置源码) 表。名字同时是 `ZAP_PROMPT_DIR` 下的相对路径。
///
/// 按 model id 子串匹配分发 system prompt(对齐 opencode
/// `packages/opencode/src/session/system.ts::provider`)。OpenRouter 路径形如
/// `anthropic/claude-3.5-sonnet` / `google/gemini-2.5-flash` / `openai/gpt-4o`
/// 也能正确命中。识别不到家族就走 default.j2 兜底,所以自定义 model id 安全。
const EMBEDDED: &[(&str, &str)] = &[
    // Partials
    ("partials/env.j2", include_str!("prompts/partials/env.j2")),
    (
        "partials/skills.j2",
        include_str!("prompts/partials/skills.j2"),
    ),
    (
        "partials/project_rules.j2",
        include_str!("prompts/partials/project_rules.j2"),
    ),
    (
        "partials/user_rules.j2",
        include_str!("prompts/partials/user_rules.j2"),
    ),
    (
        "partials/tool_aliases.j2",
        include_str!("prompts/partials/tool_aliases.j2"),
    ),
    (
        "partials/footer.j2",
        include_str!("prompts/partials/footer.j2"),
    ),
    (
        "partials/thinking_language.j2",
        include_str!("prompts/partials/thinking_language.j2"),
    ),
    (
        "partials/plan_mode.j2",
        include_str!("prompts/partials/plan_mode.j2"),
    ),
    // Commands
    (
        "commands/init_project.j2",
        include_str!("prompts/commands/init_project.j2"),
    ),
    // System
    ("system/default.j2", include_str!("prompts/system/default.j2")),
    (
        "system/anthropic.j2",
        include_str!("prompts/system/anthropic.j2"),
    ),
    ("system/gpt.j2", include_str!("prompts/system/gpt.j2")),
    ("system/beast.j2", include_str!("prompts/system/beast.j2")),
    ("system/codex.j2", include_str!("prompts/system/codex.j2")),
    ("system/gemini.j2", include_str!("prompts/system/gemini.j2")),
    ("system/kimi.j2", include_str!("prompts/system/kimi.j2")),
    ("system/trinity.j2", include_str!("prompts/system/trinity.j2")),
    ("system/local.j2", include_str!("prompts/system/local.j2")),
    ("system/lean.j2", include_str!("prompts/system/lean.j2")),
    // Active-AI prompts (command suggestions / input completion / relevant files /
    // next command / workflow metadata). These used to be `include_str!`'d into a
    // separate Environment in the active_ai module with no hot-reload; they now live
    // here so they share [`env`]'s hot-reload + per-template mtime cache and can be
    // overridden from the Prompt template dir just like the system prompts.
    (
        "active_ai/prompt_suggestions_system.j2",
        include_str!("prompts/active_ai/prompt_suggestions_system.j2"),
    ),
    (
        "active_ai/prompt_suggestions_user.j2",
        include_str!("prompts/active_ai/prompt_suggestions_user.j2"),
    ),
    (
        "active_ai/nld_predict_system.j2",
        include_str!("prompts/active_ai/nld_predict_system.j2"),
    ),
    (
        "active_ai/nld_predict_user.j2",
        include_str!("prompts/active_ai/nld_predict_user.j2"),
    ),
    (
        "active_ai/relevant_files_system.j2",
        include_str!("prompts/active_ai/relevant_files_system.j2"),
    ),
    (
        "active_ai/relevant_files_user.j2",
        include_str!("prompts/active_ai/relevant_files_user.j2"),
    ),
    (
        "active_ai/next_command_system.j2",
        include_str!("prompts/active_ai/next_command_system.j2"),
    ),
    (
        "active_ai/next_command_user.j2",
        include_str!("prompts/active_ai/next_command_user.j2"),
    ),
    (
        "active_ai/workflow_metadata_system.j2",
        include_str!("prompts/active_ai/workflow_metadata_system.j2"),
    ),
    (
        "active_ai/workflow_metadata_user.j2",
        include_str!("prompts/active_ai/workflow_metadata_user.j2"),
    ),
];

/// 纯文本资产表(不过 minijinja)。名字同样是 `ZAP_PROMPT_DIR` 下的相对路径。
///
/// 和 [`EMBEDDED`] 分开是**故意**的:这些是直接喂给模型的 markdown,不是模板。
/// 塞进 Environment 会被当 jinja 解析 —— `websearch.md` 里就有字面量 `{{year}}`
/// (由 `chat_stream::build_tools_array` 自己做替换),jinja 解析会直接毁掉它。
///
/// tool description 按 **tool name** 查:`tool_descriptions/{name}.md`。
/// 目前 13 个有独立文件的 tool 其 name 与文件名一一对应;
/// documents / markers / suggest 那几个描述写在代码里,不参与覆盖。
const EMBEDDED_RAW: &[(&str, &str)] = &[
    (
        "tool_descriptions/run_shell_command.md",
        include_str!("prompts/tool_descriptions/run_shell_command.md"),
    ),
    (
        "tool_descriptions/read_files.md",
        include_str!("prompts/tool_descriptions/read_files.md"),
    ),
    (
        "tool_descriptions/grep.md",
        include_str!("prompts/tool_descriptions/grep.md"),
    ),
    (
        "tool_descriptions/file_glob.md",
        include_str!("prompts/tool_descriptions/file_glob.md"),
    ),
    (
        "tool_descriptions/apply_file_diffs.md",
        include_str!("prompts/tool_descriptions/apply_file_diffs.md"),
    ),
    (
        "tool_descriptions/write_to_long_running_shell_command.md",
        include_str!("prompts/tool_descriptions/write_to_long_running_shell_command.md"),
    ),
    (
        "tool_descriptions/read_shell_command_output.md",
        include_str!("prompts/tool_descriptions/read_shell_command_output.md"),
    ),
    (
        "tool_descriptions/ask_user_question.md",
        include_str!("prompts/tool_descriptions/ask_user_question.md"),
    ),
    (
        "tool_descriptions/read_skill.md",
        include_str!("prompts/tool_descriptions/read_skill.md"),
    ),
    (
        "tool_descriptions/todowrite.md",
        include_str!("prompts/tool_descriptions/todowrite.md"),
    ),
    (
        "tool_descriptions/webfetch.md",
        include_str!("prompts/tool_descriptions/webfetch.md"),
    ),
    (
        "tool_descriptions/websearch.md",
        include_str!("prompts/tool_descriptions/websearch.md"),
    ),
    (
        "tasks/title_system.md",
        include_str!("prompts/tasks/title_system.md"),
    ),
];

/// 查一份纯文本资产:覆盖目录里有就用覆盖版,否则用内置版。
///
/// 返回 `Cow` 而不是 `&'static str`:覆盖版是运行时读出来的 owned String。
/// 未登记的 asset 名返回 `None`(调用方应传编译期常量,不会走到)。
fn raw_asset(name: &str) -> Option<Cow<'static, str>> {
    let embedded = EMBEDDED_RAW
        .iter()
        .find(|(n, _)| *n == name)
        .map(|(_, src)| *src)?;

    if let Some(dir) = active_override_dir() {
        let path = dir.join(name);
        match std::fs::read_to_string(&path) {
            Ok(s) => return Some(Cow::Owned(s)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => log::warn!(
                "[byop prompt] read {} failed: {e} — 用内置版本",
                path.display()
            ),
        }
    }
    Some(Cow::Borrowed(embedded))
}

/// 取某个 tool 的描述:覆盖目录里的 `tool_descriptions/{tool_name}.md` 优先,
/// 否则用 `fallback`(即 registry 里 `include_str!` 进来的那份)。
///
/// 没走 [`CACHE`] 的 mtime 缓存:一次请求只查这十来个文件、每个几 KB,
/// 而且只在开了热加载时才读盘;为它单独再建一套缓存不划算。
pub fn tool_description(tool_name: &str, fallback: &'static str) -> Cow<'static, str> {
    if active_override_dir().is_none() {
        return Cow::Borrowed(fallback);
    }
    raw_asset(&format!("tool_descriptions/{tool_name}.md")).unwrap_or(Cow::Borrowed(fallback))
}

/// 取会话标题生成用的 system prompt(`tasks/title_system.md`)。
/// 内置版就在 [`EMBEDDED_RAW`] 里,调用方不用再传 fallback。
pub fn title_system_prompt() -> Cow<'static, str> {
    raw_asset("tasks/title_system.md")
        .expect("tasks/title_system.md 已登记在 EMBEDDED_RAW 中")
}

/// Read a user-supplied raw prompt file (path relative to the prompt template
/// dir) as plain text — no minijinja. Used by prompts that do their own
/// placeholder substitution (e.g. the title prompt's `{{ language }}`).
///
/// Returns `None` — so the caller falls back to the built-in — when no prompt dir
/// is configured, the path escapes it (absolute or contains `..`), or the file is
/// missing/unreadable.
pub fn custom_prompt_raw(rel: &str) -> Option<String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        log::error!(
            "[byop prompt] custom prompt path {rel:?} must be relative and within the prompt dir"
        );
        return None;
    }
    let dir = active_override_dir()?;
    let path = dir.join(rel_path);
    match std::fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) => {
            log::error!(
                "[byop prompt] read custom prompt {} failed: {e}",
                path.display()
            );
            None
        }
    }
}

/// 把内置模板 + 纯文本资产导出到 `dir`,返回实际写出的文件数。
///
/// 语义是「补齐,不覆盖」:
/// - 已存在的文件一律跳过 —— 用户改过的东西不能被这个动作抹掉。
/// - 因此可以重复点:升级后新增的模板会被补进去,老的改动原样保留。
///
/// Only exports what is **actually overridable** — i.e. everything in `EMBEDDED`
/// and `EMBEDDED_RAW`. `active_ai/*.j2` are now part of `EMBEDDED` (they share the
/// hot-reload env), so they get seeded and can be overridden per file. Prompts that
/// are hardcoded in code (e.g. the real compaction prompt in `byop_compaction::prompt`)
/// are not in either table and are intentionally not exported.
pub fn seed_dir(dir: &Path) -> std::io::Result<usize> {
    let mut written = 0usize;
    for (name, content) in EMBEDDED.iter().chain(EMBEDDED_RAW.iter()) {
        let path = dir.join(name);
        if path.exists() {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, content)?;
        written += 1;
    }
    log::info!(
        "[byop prompt] 导出内置模板到 {} — 新写 {written} 个文件(已存在的已跳过)",
        dir.display()
    );
    Ok(written)
}

/// 设置面板给出的默认建议路径(`~/.zap/prompts`)。拿不到 home 时返回 `None`。
pub fn default_prompts_dir() -> Option<PathBuf> {
    warp_core::paths::warp_home_prompts_dir()
}

/// 模板热加载的当前状态,供设置面板展示一个可见指示 ——
/// 否则“设了目录却没生效”“忘了设目录”都是静默态,用户只会看到内置模板照旧发出去。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideStatus {
    /// 没有生效的覆盖目录(设置为空且没设 `ZAP_PROMPT_DIR`)→ 走内置模板。
    Inactive,
    /// 覆盖生效中。
    Active {
        /// 实际生效的目录。
        dir: PathBuf,
        /// 是否来自 `ZAP_PROMPT_DIR` 环境变量(优先级高于设置面板)。
        from_env: bool,
        /// 覆盖目录里实际存在的可覆盖文件数。
        on_disk: usize,
        /// 可覆盖文件总数(`EMBEDDED` + `EMBEDDED_RAW`)。
        total: usize,
    },
}

/// 计算 [`OverrideStatus`]。只 stat 文件是否存在,不读内容。
pub fn override_status() -> OverrideStatus {
    let Some(dir) = active_override_dir() else {
        return OverrideStatus::Inactive;
    };
    let from_env = std::env::var_os(PROMPT_DIR_ENV)
        .filter(|v| !v.is_empty())
        .is_some();
    let total = EMBEDDED.len() + EMBEDDED_RAW.len();
    let on_disk = EMBEDDED
        .iter()
        .chain(EMBEDDED_RAW.iter())
        .filter(|(name, _)| dir.join(name).is_file())
        .count();
    OverrideStatus::Active {
        dir,
        from_env,
        on_disk,
        total,
    }
}

fn build_env() -> Environment<'static> {
    let mut env = Environment::new();
    for (name, src) in EMBEDDED {
        env.add_template(name, src)
            .unwrap_or_else(|e| panic!("template {name} parses: {e}"));
    }
    env
}

/// 从 `dir` 覆盖构建一份 Environment。逐模板尝试读盘,失败就用内置源码兜底。
fn build_env_from_dir(dir: &Path) -> Environment<'static> {
    let mut env = Environment::new();
    let mut overridden = 0usize;

    for (name, embedded) in EMBEDDED {
        // name 是编译期常量(`system/local.j2` 这类),不含用户输入,join 无穿越风险。
        let path = dir.join(name);
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                log::warn!("[byop prompt] read {} failed: {e} — 用内置版本", path.display());
                None
            }
        };

        match src {
            // 读到了但语法错:回退内置,并把错误报出来(否则用户只会看到
            // “改了模板但没生效”,查不到原因)。
            Some(s) => match env.add_template_owned(*name, s) {
                Ok(()) => overridden += 1,
                Err(e) => {
                    log::error!(
                        "[byop prompt] {} 语法错误,回退内置版本: {e}",
                        path.display()
                    );
                    env.add_template(name, embedded)
                        .unwrap_or_else(|e| panic!("embedded template {name} parses: {e}"));
                }
            },
            None => env
                .add_template(name, embedded)
                .unwrap_or_else(|e| panic!("embedded template {name} parses: {e}")),
        }
    }

    log::debug!(
        "[byop prompt] 热加载 {}: {overridden}/{} 个模板来自磁盘",
        dir.display(),
        EMBEDDED.len()
    );
    env
}

/// 渲染用的 Environment 句柄。
///
/// 默认路径返回 `OnceLock` 里那份 `&'static`(和热加载引入前完全一致,零运行时 IO);
/// 开了热加载才走 [`CACHE`]:每次渲染只 stat 一遍模板对 mtime,没变就复用
/// 已解析好的那份,改动过才重新解析。
enum EnvHandle {
    Static(&'static Environment<'static>),
    Cached(Arc<Environment<'static>>),
}

impl std::ops::Deref for EnvHandle {
    type Target = Environment<'static>;

    fn deref(&self) -> &Self::Target {
        match self {
            EnvHandle::Static(e) => e,
            EnvHandle::Cached(e) => e,
        }
    }
}

/// 设置面板(设置 → AI → Prompt template directory)写进来的覆盖目录。
///
/// `prompt_renderer` 是一组自由函数,拿不到 `warpui::AppContext`,没法直接读
/// `AISettings`;所以由 settings 层在启动 / 设置变更时调 [`set_override_dir`]
/// 把值推过来。优先级低于 [`PROMPT_DIR_ENV`] —— 环境变量是临时调试用的,
/// 应该盖过持久化配置。
static OVERRIDE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// 由 settings 层调用,推送 设置 → AI → Prompt template directory 的当前值。
/// 传 `None` / 空串表示关闭热加载(回到内置模板)。
pub fn set_override_dir(dir: Option<PathBuf>) {
    let dir = dir.filter(|p| !p.as_os_str().is_empty());
    match OVERRIDE_DIR.write() {
        Ok(mut slot) => {
            if *slot != dir {
                match &dir {
                    Some(p) => log::info!("[byop prompt] 模板热加载目录 → {}", p.display()),
                    None => log::info!("[byop prompt] 模板热加载已关闭,使用内置模板"),
                }
                *slot = dir;
            }
        }
        Err(e) => log::error!("[byop prompt] OVERRIDE_DIR 锁中毒,忽略本次设置: {e}"),
    }
}

/// The currently-effective Prompt template directory (env var first, then the
/// settings panel). Exposed for the UI to validate / resolve the relative paths of
/// each slot's custom prompt file.
pub fn active_prompt_dir() -> Option<PathBuf> {
    active_override_dir()
}

/// 解析本次渲染要用的覆盖目录:环境变量优先,其次设置面板。
fn active_override_dir() -> Option<PathBuf> {
    // 每次读环境变量(而不是启动时缓存一次),这样改完环境重开会话即生效。
    // 开销是一次 getenv,相对一次 LLM 请求可忽略。
    if let Some(dir) = std::env::var_os(PROMPT_DIR_ENV) {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    // 锁中毒时退化为“无覆盖”,而不是 panic —— 热加载不该拖垮会话。
    OVERRIDE_DIR.read().ok().and_then(|slot| slot.clone())
}

/// 覆盖目录下每个模板的 mtime 快照。`None` = 该文件当前不存在
/// (记下来是必要的:新建一个原本缺失的覆盖文件同样得触发重建)。
type Stamps = Vec<Option<SystemTime>>;

/// 已解析的覆盖 Environment + 其来源目录 + 当时的 mtime 快照。
struct CachedEnv {
    dir: PathBuf,
    stamps: Stamps,
    env: Arc<Environment<'static>>,
}

static CACHE: RwLock<Option<CachedEnv>> = RwLock::new(None);

/// stat 覆盖目录下的每个模板,取 mtime 快照。只 stat 不读内容。
fn stamp_dir(dir: &Path) -> Stamps {
    EMBEDDED
        .iter()
        .map(|(name, _)| {
            std::fs::metadata(dir.join(name))
                .and_then(|m| m.modified())
                .ok()
        })
        .collect()
}

fn env() -> EnvHandle {
    let Some(dir) = active_override_dir() else {
        return EnvHandle::Static(ENV.get_or_init(build_env));
    };

    // 命中判定:目录没换 + 每个模板的 mtime 都没变。
    // 常见路径(没改模板)只有 ~20 次 stat + 一次 Arc clone,不重新解析。
    //
    // 注意 mtime 在部分文件系统上只有 1 秒粒度:同一秒内的连续两次编辑可能
    // 认不出来。手改模板不会撞上;真撞上了也只是那一次渲染用旧模板,
    // 下一次就正常。为此上 hash 不划算。
    let stamps = stamp_dir(&dir);
    if let Ok(cache) = CACHE.read() {
        if let Some(cached) = cache.as_ref() {
            if cached.dir == dir && cached.stamps == stamps {
                return EnvHandle::Cached(Arc::clone(&cached.env));
            }
        }
    }

    let env = Arc::new(build_env_from_dir(&dir));
    // 写缓存失败(锁中毒)不影响本次渲染,只是下次还得重解析。
    match CACHE.write() {
        Ok(mut slot) => {
            *slot = Some(CachedEnv {
                dir,
                stamps,
                env: Arc::clone(&env),
            })
        }
        Err(e) => log::error!("[byop prompt] 模板缓存锁中毒,本次不缓存: {e}"),
    }
    EnvHandle::Cached(env)
}

// ---------------------------------------------------------------------------
// 模板选择
// ---------------------------------------------------------------------------

/// 按 model id 子串匹配选模板(对齐 opencode
/// `packages/opencode/src/session/system.ts::provider`)。
///
/// Ollama / 本地 BYOP 走 [`pick_template`] 的 `local.j2` 短模板(见 `api_type` 参数),
/// 避免 9k+ 的 default.j2 淹没小模型的对话上下文。
pub fn pick_template(model_id: &str, api_type: AgentProviderApiType) -> &'static str {
    if api_type == AgentProviderApiType::Ollama {
        return "system/local.j2";
    }
    pick_template_by_model(model_id)
}

/// 按 model id 子串匹配选模板(不含 provider 级 override)。
fn pick_template_by_model(model_id: &str) -> &'static str {
    let id = model_id.to_ascii_lowercase();

    if id.contains("gpt-4") || id.contains("o1") || id.contains("o3") || id.contains("o4") {
        return "system/beast.j2";
    }
    if id.contains("gpt") {
        if id.contains("codex") {
            return "system/codex.j2";
        }
        return "system/gpt.j2";
    }
    if id.contains("gemini-") {
        return "system/gemini.j2";
    }
    if id.contains("claude") || id.contains("sonnet") || id.contains("opus") || id.contains("haiku")
    {
        return "system/anthropic.j2";
    }
    if id.contains("trinity") {
        return "system/trinity.j2";
    }
    if id.contains("kimi") {
        return "system/kimi.j2";
    }
    "system/default.j2"
}

/// 从 `LLMId` 中抽取模型 id 字串。BYOP 编码会取 model 部分,
/// 否则原样返回(理论上 BYOP 路径只会传 BYOP id,但兜底一下)。
fn model_id_from_llm_id(id: &LLMId) -> String {
    if let Some((_pid, mid)) = super::llm_id::decode(id) {
        mid
    } else {
        id.as_str().to_owned()
    }
}

// ---------------------------------------------------------------------------
// AIAgentContext → 扁平模板上下文
// ---------------------------------------------------------------------------

#[derive(Debug, Default, Serialize)]
struct ShellCtx {
    name: String,
    version: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct OsCtx {
    platform: String,
    distribution: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct GitCtx {
    head: String,
    branch: Option<String>,
}

#[derive(Debug, Serialize)]
struct SkillCtx {
    name: String,
    description: String,
    /// Absolute path to SKILL.md for filesystem skills; `None` for bundled skills.
    /// Bundled skills are loaded via `AIAgentInput::InvokeSkill`, not `read_skill`,
    /// so exposing `@warp-skill:<id>` here would mislead the model into calling a
    /// path that always fails the BYOP `skill_by_reference` lookup.
    path: Option<String>,
}

#[derive(Debug, Serialize)]
struct ProjectRuleCtx {
    path: String,
    content: String,
}

/// Zap BYOP 修复 Issue #116:全局 Rules(用户在 设置 → Agents → Rules 创建)
/// 的扁平视图,喂给 `partials/user_rules.j2` 渲染进 system prompt。
#[derive(Debug, Serialize)]
struct UserRuleCtx {
    name: Option<String>,
    content: String,
}

#[derive(Debug, Default, Serialize)]
struct InitProjectCommandContext {
    arguments: String,
}

#[derive(Debug, Default, Serialize)]
struct PromptContext {
    cwd: Option<String>,
    shell: Option<ShellCtx>,
    os: Option<OsCtx>,
    git: Option<GitCtx>,
    skills: Vec<SkillCtx>,
    project_rules: Vec<ProjectRuleCtx>,
    /// Zap BYOP 修复 Issue #116:由 caller(`render_system`)从
    /// `RequestParams.user_rules` 注入,经 `partials/user_rules.j2` 渲染。
    user_rules: Vec<UserRuleCtx>,
    current_time: String,
    model_id: String,
    /// 本轮真正喂给上游模型的 tool name 列表(由 `chat_stream::available_tool_names`
    /// 计算,含 gating 后的内置 tools 和当前 MCP tools)。
    /// 模板按此动态渲染白名单,不再硬编码。
    available_tools: Vec<String>,
    /// 本轮是否处于 `/plan` 触发的 Plan Mode(只读研究模式)。
    /// 由 `chat_stream::is_plan_mode_turn` 计算,模板按此 include
    /// `partials/plan_mode.j2` 注入只读约束 + 计划产出引导。
    plan_mode: bool,
}

fn collect_prompt_context(model_id: &str, ctx: &[AIAgentContext]) -> PromptContext {
    let mut out = PromptContext {
        // P0-1 prompt cache 优化:`current_time` 只保留到自然日粒度,
        // 不再精确到秒。原因:
        // - system prompt 中任何每请求都变的内容都会让 Anthropic 的第 1 个
        //   system breakpoint 写入的 hash 独一无二 → 写完即废,永不命中。
        //   OpenAI 前 256 token 路由哈希同理,会被分散到不同机器。
        // - 模型实际只需要知道“今天是哪天”就够了,跳越自然日那一次
        //   miss 成本可接受(一天 × 所有活跃对话 × system tokens)。
        // - 跨年同理成本与跨日一致,不需额外处理。
        // 后续可考虑进一步把“当前时间”移到 user message 末尾(P0-1 方案 C),
        // 让 system 段 100% 稳定;本步先取低风险的方案 B。
        current_time: Local::now().format("%Y-%m-%d").to_string(),
        model_id: model_id.to_owned(),
        ..Default::default()
    };

    for c in ctx {
        match c {
            AIAgentContext::Directory { pwd, .. } => {
                if out.cwd.is_none() {
                    out.cwd = pwd.clone();
                }
            }
            AIAgentContext::ExecutionEnvironment(exec) => {
                out.shell = Some(ShellCtx {
                    name: exec.shell_name.clone(),
                    version: exec.shell_version.clone(),
                });
                let has_os = exec.os.category.is_some() || exec.os.distribution.is_some();
                if has_os {
                    out.os = Some(OsCtx {
                        platform: exec.os.category.clone().unwrap_or_default(),
                        distribution: exec.os.distribution.clone(),
                    });
                }
            }
            AIAgentContext::CurrentTime { current_time } => {
                // P0-1:与默认值保持一致,只保留自然日粒度。
                // 上游 Zap 有可能传入精确到秒的 timestamp,这里统一压到“当前日期”。
                out.current_time = current_time.format("%Y-%m-%d").to_string();
            }
            // 代码索引功能未实现,Codebase 上下文不进 system prompt。
            AIAgentContext::Codebase { .. } => {}
            // P1-7 prompt cache 说明:`Git { head, branch }` 取决于当前仓库状态,
            // 用户切分支会让渲染出的 system 段变化,导致所有上游供应商
            // (Anthropic / OpenAI / DeepSeek)的 system+messages cache 全部失效。
            // 这是**预期行为**:
            //   - 指令模型在新分支上不能认为是老 git context;
            //   - 作为代价用户在新分支上首请求 100% miss、写入新 cache,之后该
            //     分支会复用。跨分支跳转频繁的开发者会看到最多的 miss。
            // 考虑过的替代:把 git 状态移到 user message 末尾(同 P0-1 方案 C),
            // 但那样 system 段会丢失“模型一看就知道当前分支”的上下文意义,
            // 需要依赖它进行推理的模型会变差。本补丁维持现状。
            AIAgentContext::Git { head, branch } => {
                out.git = Some(GitCtx {
                    head: head.clone(),
                    branch: branch.clone(),
                });
            }
            AIAgentContext::Skills { skills } => {
                for s in skills {
                    let path = match &s.reference {
                        ai::skills::SkillReference::Path(p) => {
                            Some(p.to_string_lossy().into_owned())
                        }
                        // Bundled skills load via InvokeSkill, not read_skill.
                        // Omit skill_path to avoid guiding the model toward a
                        // value that will always fail BYOP's skill_by_reference.
                        ai::skills::SkillReference::BundledSkillId(_) => None,
                    };
                    out.skills.push(SkillCtx {
                        name: s.name.clone(),
                        description: s.description.clone(),
                        path,
                    });
                }
            }
            AIAgentContext::ProjectRules {
                root_path,
                active_rules,
                ..
            } => {
                use ai::agent::action_result::AnyFileContent;
                for rule in active_rules {
                    let content = match &rule.content {
                        AnyFileContent::StringContent(s) => s.clone(),
                        AnyFileContent::BinaryContent(_) => continue,
                    };
                    let path = if rule.file_name.starts_with('/') {
                        rule.file_name.clone()
                    } else {
                        format!("{root_path}/{}", rule.file_name)
                    };
                    out.project_rules.push(ProjectRuleCtx { path, content });
                }
            }
            // 用户附件类 context(File / Image / SelectedText / Block)不进 system prompt,
            // 由 `user_context::render_user_attachments` 在 chat_stream 的 UserQuery 分支
            // 注入到当前轮 user message。这跟 warp 自家路径分两类的语义对齐:
            // - 环境型 → InputContext.{directory,shell,git,...} → 后端注入 system 区
            // - 附件型 → InputContext.{executed_shell_commands,selected_text,files,images}
            //            → 后端注入 user 区
            AIAgentContext::File(_)
            | AIAgentContext::Image(_)
            | AIAgentContext::SelectedText(_)
            | AIAgentContext::Block(_) => {}
        }
    }

    out
}

// ---------------------------------------------------------------------------
// 公共 API
// ---------------------------------------------------------------------------

pub fn render_init_project_command(arguments: Option<&str>) -> String {
    let arguments = arguments
        .map(str::trim)
        .filter(|arguments| !arguments.is_empty())
        .unwrap_or("(none)")
        .to_owned();
    let ctx = InitProjectCommandContext { arguments };
    let env = env();
    let template_name = "commands/init_project.j2";
    let tmpl = match env.get_template(template_name) {
        Ok(t) => t,
        Err(e) => {
            log::error!("[byop prompt] failed to get template {template_name}: {e}");
            return fallback_init_project_command(&ctx.arguments);
        }
    };
    match tmpl.render(Value::from_serialize(&ctx)) {
        Ok(s) => s,
        Err(e) => {
            log::error!("[byop prompt] render {template_name} failed: {e}");
            fallback_init_project_command(&ctx.arguments)
        }
    }
}

/// 渲染最终发给上游模型的 system message 字符串。
///
/// `ctx` 一般来自 `params.input` 中最近一条 `AIAgentInput::UserQuery.context`。
/// 拿不到 context(空数组)也 OK — 模板会用 default 占位渲染。
///
/// `available_tools` 由 `chat_stream::available_tool_names` 计算,本轮实际暴露给
/// 上游 LLM 的工具名列表(内置 + MCP,已应用 gating)。模板按此动态渲染白名单,
/// 不要再硬编码"unavailable tools"黑名单 —— 模型看不到的工具自然不会调,
/// 反过来用文本黑名单会让模型连真实可用的工具也不敢调。
pub fn render_system(
    api_type: AgentProviderApiType,
    model: &LLMId,
    ctx: &[AIAgentContext],
    available_tools: &[String],
    plan_mode: bool,
    user_rules: &[(Option<String>, String)],
) -> String {
    render_system_with_override(
        api_type,
        model,
        ctx,
        available_tools,
        plan_mode,
        user_rules,
        None,
    )
}

/// Like [`render_system`], but honors a per-model-slot [`PromptSource`] override
/// resolved from the active profile:
///
/// - `None` → Auto: pick the template by model family ([`pick_template`]), unchanged.
/// - `Some(Builtin(name))` → render `system/<name>.j2` instead of the auto pick.
/// - `Some(CustomFile(rel))` → read `rel` from the prompt template directory and
///   render it as a template in the same minijinja environment, so a custom prompt
///   can still `{% include "partials/..." %}` the shared env / skills / tools blocks.
///
/// Every override path degrades gracefully: a missing/typo'd builtin name, a missing
/// custom file, an unset prompt dir, or a template syntax error all log and fall back
/// to the Auto pick rather than sending a broken prompt.
pub fn render_system_with_override(
    api_type: AgentProviderApiType,
    model: &LLMId,
    ctx: &[AIAgentContext],
    available_tools: &[String],
    plan_mode: bool,
    user_rules: &[(Option<String>, String)],
    prompt_override: Option<&PromptSource>,
) -> String {
    let model_id = model_id_from_llm_id(model);
    let mut prompt_ctx = collect_prompt_context(&model_id, ctx);
    prompt_ctx.available_tools = available_tools.to_vec();
    prompt_ctx.plan_mode = plan_mode;
    prompt_ctx.user_rules = user_rules
        .iter()
        .map(|(name, content)| UserRuleCtx {
            name: name.clone(),
            content: content.clone(),
        })
        .collect();

    let env = env();

    // Custom-file override: read from the prompt dir and render ad-hoc. On any
    // failure, fall through to the builtin/auto path below.
    if let Some(PromptSource::CustomFile(rel)) = prompt_override {
        match render_custom_file(&env, rel, &prompt_ctx) {
            Ok(s) => return s,
            Err(e) => log::error!(
                "[byop prompt] custom prompt file {rel:?} unusable ({e}); falling back to auto pick"
            ),
        }
    }

    // Builtin override (if the slot pins one) then the auto pick as fallback.
    let auto_name = pick_template(&model_id, api_type);
    let override_name = prompt_override.and_then(|s| s.builtin_template_name());
    for template_name in override_name.as_deref().into_iter().chain([auto_name]) {
        let tmpl = match env.get_template(template_name) {
            Ok(t) => t,
            Err(e) => {
                log::error!("[byop prompt] failed to get template {template_name}: {e}");
                continue;
            }
        };
        match tmpl.render(Value::from_serialize(&prompt_ctx)) {
            Ok(s) => return s,
            Err(e) => log::error!("[byop prompt] render {template_name} failed: {e}"),
        }
    }
    fallback_system(&model_id)
}

/// Render a user-supplied custom prompt file (relative to the prompt template dir)
/// as an ad-hoc template in the shared environment, so its `{% include %}`s resolve
/// against the built-in partials.
///
/// The relative path is confined to the prompt dir: absolute paths and any `..`
/// component are rejected so a stored profile can't be used to read arbitrary files.
fn render_custom_file(
    env: &Environment<'static>,
    rel: &str,
    prompt_ctx: &PromptContext,
) -> Result<String, String> {
    let dir = active_override_dir().ok_or("no prompt template directory configured")?;
    render_custom_file_from(env, &dir, rel, prompt_ctx)
}

/// [`render_custom_file`] with the base dir passed explicitly (pure — no global
/// state — so the path guard and template resolution are unit-testable).
fn render_custom_file_from(
    env: &Environment<'static>,
    dir: &Path,
    rel: &str,
    prompt_ctx: &PromptContext,
) -> Result<String, String> {
    render_custom_file_value(env, dir, rel, Value::from_serialize(prompt_ctx))
}

/// Core of the custom-file path, taking an already-built minijinja [`Value`] so
/// both the agent system prompt (`PromptContext`) and the active-ai prompts
/// (ad-hoc `context! {}` values) can share the path guard + template resolution.
fn render_custom_file_value(
    env: &Environment<'static>,
    dir: &Path,
    rel: &str,
    ctx: Value,
) -> Result<String, String> {
    let rel_path = Path::new(rel);
    if rel_path.is_absolute()
        || rel_path
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(format!(
            "path {rel:?} must be relative and stay within the prompt dir"
        ));
    }
    let path = dir.join(rel_path);
    let source = std::fs::read_to_string(&path)
        .map_err(|e| format!("read {} failed: {e}", path.display()))?;
    env.render_named_str(rel, &source, ctx)
        .map_err(|e| e.to_string())
}

/// Render a named template from the hot-reloadable env, returning `""` on failure
/// (mirrors the old `active_ai::render`: a broken auxiliary prompt should degrade
/// to empty, never panic). Used for the active-ai prompts now that they live in the
/// shared [`EMBEDDED`] table and are overridable from the prompt template dir.
pub fn render_template(name: &str, ctx: Value) -> String {
    let env = env();
    match env.get_template(name) {
        Ok(t) => t.render(ctx).unwrap_or_else(|e| {
            log::warn!("[byop prompt] render {name} failed: {e}");
            String::new()
        }),
        Err(e) => {
            log::warn!("[byop prompt] get template {name} failed: {e}");
            String::new()
        }
    }
}

/// Like [`render_template`], but honors a per-prompt profile override. Auxiliary
/// prompts have a single built-in each, so only [`PromptSource::CustomFile`] is
/// meaningful here (`None` / `Builtin` → render the built-in `name`). A missing
/// file / unset prompt dir / traversal / syntax error logs and falls back to `name`.
pub fn render_template_with_override(
    name: &str,
    prompt_override: Option<&PromptSource>,
    ctx: Value,
) -> String {
    if let Some(PromptSource::CustomFile(rel)) = prompt_override {
        match active_override_dir() {
            Some(dir) => {
                let env = env();
                match render_custom_file_value(&env, &dir, rel, ctx.clone()) {
                    Ok(s) => return s,
                    Err(e) => log::error!(
                        "[byop prompt] custom prompt {rel:?} unusable ({e}); falling back to {name}"
                    ),
                }
            }
            None => log::error!(
                "[byop prompt] custom prompt {rel:?} set but no prompt dir configured; falling back to {name}"
            ),
        }
    }
    render_template(name, ctx)
}

fn fallback_init_project_command(arguments: &str) -> String {
    format!(
        "Create or update `AGENTS.md` for this repository.\n\nUser-provided focus or constraints (honor these):\n{arguments}"
    )
}

/// 渲染兜底 system(只在模板加载/渲染失败时用,不应在正常路径触发)。
fn fallback_system(model_id: &str) -> String {
    format!(
        "You are the AI coding agent inside Zap, an AI Development Environment (ADE). \
         Model: {model_id}. \
         Use the registered tools (run_shell_command / read_files / apply_file_diffs / grep / file_glob / ...) \
         to take actions on the user's behalf. Be concise."
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::agent::AIAgentContext;
    use crate::ai_assistant::execution_context::{WarpAiExecutionContext, WarpAiOsContext};

    // -- 模板热加载(ZAP_PROMPT_DIR)-----------------------------------------
    //
    // 都直接测 `build_env_from_dir`(纯函数,入参是路径),不走 `env()`。
    // 因为 `env()` 读进程级环境变量,而 cargo test 默认多线程跑,
    // set_var/remove_var 会跨测试互相打架。`env()` 本身只是
    // “读 var → 二选一”,逻辑薄到不值得为它引 serial_test。

    /// 在 dir 下按模板名写一份覆盖文件(自动建父目录)。
    fn write_override(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    #[test]
    fn hot_reload_empty_dir_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let env = build_env_from_dir(tmp.path());
        // 一个模板都没覆盖 → 渲染结果应和内置版本一致
        let embedded = build_env();
        for (name, _) in EMBEDDED {
            assert!(env.get_template(name).is_ok(), "{name} 应存在");
        }
        let ctx = Value::from_serialize(&PromptContext {
            model_id: "test-model".into(),
            ..Default::default()
        });
        assert_eq!(
            env.get_template("system/local.j2")
                .unwrap()
                .render(ctx.clone())
                .unwrap(),
            embedded
                .get_template("system/local.j2")
                .unwrap()
                .render(ctx)
                .unwrap(),
        );
    }

    #[test]
    fn hot_reload_picks_up_overridden_template() {
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "OVERRIDDEN {{ model_id }}");

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext {
                model_id: "qwen2.5-coder".into(),
                ..Default::default()
            }))
            .unwrap();

        assert_eq!(out, "OVERRIDDEN qwen2.5-coder");
    }

    #[test]
    fn hot_reload_overrides_are_per_file() {
        // 只覆盖 local.j2,其他模板必须仍是内置版本 —— 覆盖不是“全有或全无”。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "OVERRIDDEN");

        let env = build_env_from_dir(tmp.path());
        let ctx = Value::from_serialize(&PromptContext::default());

        assert_eq!(
            env.get_template("system/local.j2")
                .unwrap()
                .render(ctx.clone())
                .unwrap(),
            "OVERRIDDEN"
        );
        // anthropic.j2 没覆盖 → 仍应渲染出内置内容
        let anthropic = env
            .get_template("system/anthropic.j2")
            .unwrap()
            .render(ctx)
            .unwrap();
        assert_ne!(anthropic, "OVERRIDDEN");
        assert!(!anthropic.is_empty());
    }

    #[test]
    fn hot_reload_overridden_partial_reaches_including_template() {
        // include 链要走覆盖版:local.j2 include 了 partials/env.j2,
        // 只覆盖 partial 也应体现在最终 system prompt 里。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "partials/env.j2", "PARTIAL-OVERRIDE");

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();

        assert!(out.contains("PARTIAL-OVERRIDE"), "{out}");
        // 内置 local.j2 的正文仍在(只换了 partial)
        assert!(out.contains("run_shell_command"), "{out}");
    }

    #[test]
    fn hot_reload_bad_syntax_falls_back_to_embedded() {
        // 手滑写坏模板不该 panic,也不该让该模板消失 —— 回退内置版本。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "{% if unclosed %}");

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();

        assert!(out.contains("run_shell_command"), "回退到内置 local.j2: {out}");
    }

    #[test]
    fn hot_reload_unrelated_files_in_dir_are_ignored() {
        // 覆盖目录里的杂项文件不参与加载(只按 EMBEDDED 的名字表查)。
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), "noise").unwrap();
        write_override(tmp.path(), "system/nonexistent.j2", "noise");

        let env = build_env_from_dir(tmp.path());
        assert!(env.get_template("system/nonexistent.j2").is_err());
        assert!(env.get_template("system/local.j2").is_ok());
    }

    #[test]
    fn hot_reload_rereads_after_edit() {
        // 热加载的核心承诺:改完存盘,下一次渲染就是新的(不缓存)。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "V1");
        let ctx = Value::from_serialize(&PromptContext::default());

        let first = build_env_from_dir(tmp.path())
            .get_template("system/local.j2")
            .unwrap()
            .render(ctx.clone())
            .unwrap();
        assert_eq!(first, "V1");

        write_override(tmp.path(), "system/local.j2", "V2");
        let second = build_env_from_dir(tmp.path())
            .get_template("system/local.j2")
            .unwrap()
            .render(ctx)
            .unwrap();
        assert_eq!(second, "V2");
    }

    #[test]
    fn hot_reload_missing_dir_falls_back_to_embedded() {
        // 配了个不存在的目录(打错路径 / 外置盘没挂)→ 全量回退,不 panic。
        let env = build_env_from_dir(Path::new("/nonexistent/zap-prompts-xyz"));
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();
        assert!(out.contains("run_shell_command"), "{out}");
    }

    #[test]
    fn stamp_dir_reports_one_entry_per_template() {
        let tmp = tempfile::tempdir().unwrap();
        let stamps = stamp_dir(tmp.path());
        assert_eq!(stamps.len(), EMBEDDED.len());
        // 空目录 → 每个都是 None(文件不存在)
        assert!(stamps.iter().all(|s| s.is_none()));
    }

    #[test]
    fn stamp_dir_marks_present_files_and_only_those() {
        // 新建一个原本缺失的覆盖文件必须让快照从 None 变 Some,
        // 否则“第一次放进覆盖文件”这一步不会触发重建。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "X");

        let stamps = stamp_dir(tmp.path());
        let idx = EMBEDDED
            .iter()
            .position(|(n, _)| *n == "system/local.j2")
            .unwrap();

        assert!(stamps[idx].is_some(), "已写入的模板应有 mtime");
        assert_eq!(
            stamps.iter().filter(|s| s.is_some()).count(),
            1,
            "只有写过的那个模板有 mtime"
        );
    }

    #[test]
    fn stamp_dir_distinguishes_directories() {
        // 缓存命中要求 dir 相同;不同目录即使内容一样也不该复用。
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_override(a.path(), "system/local.j2", "X");

        assert_ne!(stamp_dir(a.path()), stamp_dir(b.path()));
    }

    // -- 纯文本资产(tool descriptions / title)-------------------------------
    //
    // 这几个要经 `active_override_dir()`,即读进程级环境变量,所以不能像上面
    // 那样纯靠传路径绕开。默认(没设 ZAP_PROMPT_DIR、没推 set_override_dir)
    // 时行为是确定的,只测这一侧;覆盖生效的路径由 `raw_asset_*` 直接测。

    #[test]
    fn tool_description_without_override_borrows_fallback() {
        let out = tool_description("grep", "FALLBACK");
        assert_eq!(out, "FALLBACK");
        assert!(matches!(out, Cow::Borrowed(_)), "默认路径不该有分配");
    }

    #[test]
    fn tool_description_unknown_tool_falls_back() {
        // documents / markers / suggest 那几个没有 .md 文件,必须回退到
        // registry 里写死的描述,而不是变成空串。
        assert_eq!(
            raw_asset("tool_descriptions/read_documents.md").as_deref(),
            None
        );
    }

    #[test]
    fn raw_asset_returns_embedded_for_registered_names() {
        let out = raw_asset("tool_descriptions/websearch.md").unwrap();
        assert!(!out.is_empty());
        // websearch.md 里有字面量 {{year}},由 chat_stream 自己替换。
        // 这里顺带确认它没被 jinja 吃掉(EMBEDDED_RAW 不过 minijinja)。
        assert!(out.contains("{{year}}"), "{out}");
    }

    #[test]
    fn raw_asset_unregistered_name_is_none() {
        assert!(raw_asset("tool_descriptions/not_a_tool.md").is_none());
        assert!(raw_asset("system/local.j2").is_none(), "模板不在 RAW 表里");
    }

    #[test]
    fn title_system_prompt_has_language_placeholder() {
        let out = title_system_prompt();
        assert!(
            out.contains("{{ language }}"),
            "chat_stream 依赖这个占位做替换: {out}"
        );
    }

    #[test]
    fn embedded_raw_covers_every_tool_description_file() {
        // 防回归:新增 tool_descriptions/*.md 但忘了登记 → 该 tool 不可热加载。
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/ai/agent_providers/prompts/tool_descriptions");
        let mut on_disk: Vec<String> = std::fs::read_dir(&root)
            .unwrap()
            .map(|e| format!("tool_descriptions/{}", e.unwrap().file_name().to_string_lossy()))
            .filter(|n| n.ends_with(".md"))
            .collect();
        on_disk.sort();

        let mut registered: Vec<String> = EMBEDDED_RAW
            .iter()
            .map(|(n, _)| (*n).to_owned())
            .filter(|n| n.starts_with("tool_descriptions/"))
            .collect();
        registered.sort();

        assert_eq!(on_disk, registered, "tool_descriptions/ 与 EMBEDDED_RAW 不一致");
    }

    #[test]
    fn embedded_raw_names_match_tool_names() {
        // 覆盖是按 tool name 查 `tool_descriptions/{name}.md` 的,
        // 名字对不上就静默失效 —— 这里钉死这个契约。
        for (name, _) in EMBEDDED_RAW
            .iter()
            .filter(|(n, _)| n.starts_with("tool_descriptions/"))
        {
            let stem = name
                .trim_start_matches("tool_descriptions/")
                .trim_end_matches(".md");
            assert!(
                super::super::tools::REGISTRY.iter().any(|t| t.name == stem),
                "{stem} 没有同名 tool,覆盖查不到"
            );
        }
    }

    // -- 导出内置模板(一键 seed)--------------------------------------------

    #[test]
    fn seed_dir_writes_every_overridable_file() {
        let tmp = tempfile::tempdir().unwrap();
        let n = seed_dir(tmp.path()).unwrap();

        assert_eq!(n, EMBEDDED.len() + EMBEDDED_RAW.len());
        for (name, content) in EMBEDDED.iter().chain(EMBEDDED_RAW.iter()) {
            let path = tmp.path().join(name);
            assert!(path.is_file(), "{name} 应被导出");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), *content);
        }
    }

    #[test]
    fn seed_dir_output_is_loadable() {
        // 导出的树必须能被 build_env_from_dir 原样吃回去 —— 否则用户点了按钮
        // 拿到一堆加载不了的文件。
        let tmp = tempfile::tempdir().unwrap();
        seed_dir(tmp.path()).unwrap();

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();
        assert!(out.contains("run_shell_command"), "{out}");
    }

    #[test]
    fn seed_dir_never_overwrites_existing_files() {
        // 用户改过的模板不能被再次导出抹掉。
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "MINE");

        let n = seed_dir(tmp.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.path().join("system/local.j2")).unwrap(),
            "MINE"
        );
        assert_eq!(n, EMBEDDED.len() + EMBEDDED_RAW.len() - 1, "只补齐缺的");
    }

    #[test]
    fn seed_dir_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let first = seed_dir(tmp.path()).unwrap();
        let second = seed_dir(tmp.path()).unwrap();

        assert!(first > 0);
        assert_eq!(second, 0, "第二次点没有新文件要写");
    }

    #[test]
    fn seed_dir_creates_missing_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("does/not/exist/yet");

        seed_dir(&nested).unwrap();
        assert!(nested.join("system/local.j2").is_file());
    }

    #[test]
    fn seed_dir_omits_non_overridable_prompts() {
        // active_ai/* 和 tasks/compaction_* 改了不生效,导出反而误导。
        let tmp = tempfile::tempdir().unwrap();
        seed_dir(tmp.path()).unwrap();

        assert!(!tmp.path().join("active_ai").exists(), "active_ai 不该导出");
        assert!(!tmp.path().join("tasks/compaction_system.j2").exists());
        // 但可覆盖的 tasks/title_system.md 要在
        assert!(tmp.path().join("tasks/title_system.md").is_file());
    }

    #[test]
    fn embedded_table_covers_every_template_file() {
        // 防回归:有人往 prompts/ 加了模板但忘了登记进 EMBEDDED,
        // 会同时丢掉热加载覆盖能力和 include 解析。
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/agent_providers/prompts");
        let mut on_disk = Vec::new();
        for sub in ["partials", "commands", "system"] {
            for entry in std::fs::read_dir(root.join(sub)).unwrap() {
                let entry = entry.unwrap();
                let name = entry.file_name().to_string_lossy().into_owned();
                if name.ends_with(".j2") {
                    on_disk.push(format!("{sub}/{name}"));
                }
            }
        }
        on_disk.sort();

        let mut registered: Vec<String> = EMBEDDED.iter().map(|(n, _)| (*n).to_owned()).collect();
        registered.sort();

        assert_eq!(
            on_disk, registered,
            "prompts/ 下的 .j2 与 EMBEDDED 名字表不一致"
        );
    }

    #[test]
    fn render_init_project_command_uses_command_template_arguments() {
        let out = render_init_project_command(Some("focus on test commands"));
        assert!(out.contains("Create or update `AGENTS.md`"), "{out}");
        assert!(out.contains("focus on test commands"), "{out}");
        assert!(out.contains("## Writing rules"), "{out}");
    }

    #[test]
    fn pick_template_ollama_uses_local_template() {
        assert_eq!(
            pick_template("qwen2.5-coder", AgentProviderApiType::Ollama),
            "system/local.j2"
        );
        assert_eq!(
            pick_template("llama3.1", AgentProviderApiType::Ollama),
            "system/local.j2"
        );
    }

    #[test]
    fn pick_template_dispatches_by_model_family() {
        // 直连形式
        for (id, want) in [
            ("claude-sonnet-4-5", "system/anthropic.j2"),
            ("claude-opus-4-1", "system/anthropic.j2"),
            ("haiku-3-5", "system/anthropic.j2"),
            ("gpt-4o", "system/beast.j2"),
            ("gpt-4-turbo", "system/beast.j2"),
            ("o1-preview", "system/beast.j2"),
            ("o3-mini", "system/beast.j2"),
            ("o4-mini", "system/beast.j2"),
            ("gpt-5-codex", "system/codex.j2"),
            ("gpt-3.5-turbo", "system/gpt.j2"),
            ("gemini-2.0-flash", "system/gemini.j2"),
            ("gemini-2.5-pro", "system/gemini.j2"),
            ("kimi-k2", "system/kimi.j2"),
            ("trinity-v1", "system/trinity.j2"),
            // 兜底
            ("deepseek-chat", "system/default.j2"),
            ("qwen2.5-coder", "system/default.j2"),
            ("glm-4", "system/default.j2"),
            ("my-custom-model", "system/default.j2"),
            ("", "system/default.j2"),
        ] {
            assert_eq!(
                pick_template(id, AgentProviderApiType::OpenAi),
                want,
                "id={id}"
            );
        }
    }

    #[test]
    fn pick_template_handles_openrouter_path_form() {
        // OpenRouter 形式 `provider/model`,子串匹配仍命中正确家族
        for (id, want) in [
            ("anthropic/claude-3.5-sonnet", "system/anthropic.j2"),
            ("anthropic/claude-opus-4", "system/anthropic.j2"),
            ("openai/gpt-4o", "system/beast.j2"),
            ("openai/gpt-5-codex", "system/codex.j2"),
            ("openai/o1-preview", "system/beast.j2"),
            ("google/gemini-2.5-flash", "system/gemini.j2"),
            ("moonshot/kimi-k2", "system/kimi.j2"),
        ] {
            assert_eq!(
                pick_template(id, AgentProviderApiType::OpenAi),
                want,
                "id={id}"
            );
        }
    }

    #[test]
    fn pick_template_is_case_insensitive() {
        for (id, want) in [
            ("Claude-Sonnet-4", "system/anthropic.j2"),
            ("GPT-4o", "system/beast.j2"),
            ("Gemini-2.5-Pro", "system/gemini.j2"),
            ("KIMI-K2", "system/kimi.j2"),
            ("Anthropic/Claude-3.5", "system/anthropic.j2"),
        ] {
            assert_eq!(
                pick_template(id, AgentProviderApiType::OpenAi),
                want,
                "id={id}"
            );
        }
    }

    #[test]
    fn render_includes_static_env_block_without_volatile_fields() {
        let ctx = vec![
            AIAgentContext::Directory {
                pwd: Some("/home/user/project".into()),
                home_dir: Some("/home/user".into()),
                are_file_symbols_indexed: false,
            },
            AIAgentContext::ExecutionEnvironment(WarpAiExecutionContext {
                os: WarpAiOsContext {
                    category: Some("linux".into()),
                    distribution: Some("Ubuntu 22.04".into()),
                },
                shell_name: "bash".into(),
                shell_version: Some("5.1".into()),
            }),
        ];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &ctx,
            &[],
            false,
            &[],
        );
        // 稳定字段仍留在 system prompt 里。
        assert!(out.contains("Shell: bash 5.1"), "{out}");
        assert!(out.contains("linux (Ubuntu 22.04)"), "{out}");
        // home 字段已对齐 opencode 砍掉,不再渲染
        assert!(!out.contains("Home directory:"), "{out}");
        // cwd 会随 `cd` 变化,已移到消息末尾的 <environment_context> 块 ——
        // system prompt(message[0])里出现任何会变的字段都会让缓存整段失效。
        assert!(!out.contains("Working directory:"), "{out}");
        assert!(!out.contains("/home/user/project"), "{out}");
    }

    /// 回归:system prompt 必须对 cwd 变化**逐字节不变**。
    ///
    /// 这条断言正是当初那个 bug 的直接反例:cwd 待在 <env> 里时,一次 `cd` 就让
    /// message[0] 改变,FLM 逐条比对在第一条就失配 → matched=0 → 整轮重新 prefill。
    #[test]
    fn system_prompt_is_byte_stable_across_cwd_change() {
        let render_with = |pwd: &str| {
            let ctx = vec![
                AIAgentContext::Directory {
                    pwd: Some(pwd.into()),
                    home_dir: Some("/home/user".into()),
                    are_file_symbols_indexed: false,
                },
                AIAgentContext::ExecutionEnvironment(WarpAiExecutionContext {
                    os: WarpAiOsContext {
                        category: Some("linux".into()),
                        distribution: Some("Ubuntu 22.04".into()),
                    },
                    shell_name: "bash".into(),
                    shell_version: Some("5.1".into()),
                }),
            ];
            render_system(
                AgentProviderApiType::OpenAi,
                &LLMId::from("byop:p:deepseek-chat"),
                &ctx,
                &[],
                false,
                &[],
            )
        };

        let before = render_with("/home/winters");
        let after = render_with("/etc");
        assert_eq!(
            before, after,
            "system prompt must not change when the working directory changes"
        );
    }

    #[test]
    fn render_produces_non_empty_for_all_families() {
        // 任意 model id 都能渲染出非空字符串(包含 Zap 自我标识)。
        for id in [
            "claude-sonnet-4-5",
            "gpt-4o",
            "gpt-5-codex",
            "gemini-2.5-pro",
            "kimi-k2",
            "trinity-v1",
            "deepseek-chat",
            "weird-model",
        ] {
            let out = render_system(
                AgentProviderApiType::OpenAi,
                &LLMId::from(format!("byop:p:{id}").as_str()),
                &[],
                &[],
                false,
                &[],
            );
            assert!(
                out.contains("Zap"),
                "id={id} should mention Zap, got: {out}"
            );
        }
    }

    #[test]
    fn render_omits_skills_block_when_empty() {
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &[],
        );
        // 没 skills 时 skills 区块不应出现
        assert!(
            !out.contains("Skills provide specialized instructions"),
            "{out}"
        );
    }

    /// Issue #169 回归:系统 prompt 中的 skill 区块必须包含 skill_path(绝对路径),
    /// 而非仅 name/description,否则模型无法正确调用 read_skill 工具。
    #[test]
    fn render_includes_skill_path_for_read_skill_tool() {
        use crate::ai::skills::SkillDescriptor;
        use ai::skills::{SkillProvider, SkillReference, SkillScope};

        let skill_path = "/home/user/.agents/skills/open-browser-use/SKILL.md";
        let skill = SkillDescriptor {
            reference: SkillReference::Path(skill_path.into()),
            name: "open-browser-use".into(),
            description: "Automates Chrome browser operations.".into(),
            scope: SkillScope::Project,
            provider: SkillProvider::Agents,
            icon_override: None,
        };
        let ctx = vec![AIAgentContext::Skills {
            skills: vec![skill],
        }];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &ctx,
            &[],
            false,
            &[],
        );
        assert!(
            out.contains(skill_path),
            "system prompt must expose the skill_path so the model can pass it to read_skill; got: {out}"
        );
    }

    /// Issue #169 后续:bundled skill 的 BundledSkillId 变体在 BYOP 路径下不可通过
    /// read_skill 加载(走 InvokeSkill),因此 system prompt 中不应输出 <skill_path>
    /// 以避免模型使用必然失败的 @warp-skill:{id} 值。
    #[test]
    fn render_omits_skill_path_for_bundled_skill() {
        use crate::ai::skills::SkillDescriptor;
        use ai::skills::{SkillProvider, SkillReference, SkillScope};
        use warp_core::ui::icons::Icon;

        let skill = SkillDescriptor {
            reference: SkillReference::BundledSkillId("find-skills".into()),
            name: "find-skills".into(),
            description: "Help discover and install new agent skills.".into(),
            scope: SkillScope::Bundled,
            provider: SkillProvider::Zap,
            icon_override: Some(Icon::WarpLogoLight),
        };
        let ctx = vec![AIAgentContext::Skills {
            skills: vec![skill],
        }];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &ctx,
            &[],
            false,
            &[],
        );
        assert!(
            out.contains("find-skills"),
            "bundled skill name should still appear in prompt: {out}"
        );
        assert!(
            !out.contains("@warp-skill:"),
            "bundled skill must NOT emit <skill_path> to avoid misleading the model: {out}"
        );
        assert!(
            !out.contains("<skill_path>"),
            "no <skill_path> tag should be rendered for bundled skills: {out}"
        );
    }

    #[test]
    fn fallback_does_not_panic() {
        // render_system 永远不会 panic,失败也走 fallback_system
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:any"),
            &[],
            &[],
            false,
            &[],
        );
        assert!(!out.is_empty());
    }

    #[test]
    fn render_lists_available_tools_dynamically() {
        // 传入的 tool 名字必须出现在 system prompt 里(动态白名单)
        let tools: Vec<String> = vec![
            "run_shell_command".into(),
            "webfetch".into(),
            "websearch".into(),
            "mcp__github__create_issue".into(),
        ];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &tools,
            false,
            &[],
        );
        for name in &tools {
            assert!(
                out.contains(name),
                "expected `{name}` in prompt, got: {out}"
            );
        }
        // 不应再出现旧黑名单措辞
        assert!(
            !out.contains("Do not call unavailable tools"),
            "黑名单段已删除: {out}"
        );
    }

    // -- 每个模型槽的 system prompt 覆盖(PromptSource)-----------------------

    #[test]
    fn builtin_template_name_maps_family_to_path() {
        assert_eq!(
            PromptSource::Builtin("lean".into()).builtin_template_name(),
            Some("system/lean.j2".to_string())
        );
        assert_eq!(
            PromptSource::CustomFile("mine.j2".into()).builtin_template_name(),
            None
        );
    }

    #[test]
    fn builtin_override_redirects_template_selection() {
        // claude-* 自动命中 anthropic.j2。
        let model = LLMId::from("byop:p:claude-sonnet-4-5");
        let auto = render_system(AgentProviderApiType::OpenAi, &model, &[], &[], false, &[]);

        // 显式 Builtin("anthropic") 等价于自动命中(同模板、同模型)。
        let forced_anthropic = render_system_with_override(
            AgentProviderApiType::OpenAi,
            &model,
            &[],
            &[],
            false,
            &[],
            Some(&PromptSource::Builtin("anthropic".into())),
        );
        assert_eq!(auto, forced_anthropic);

        // 强制换成 default.j2 时,输出必须变化 —— 证明覆盖真的改了模板选择。
        let forced_default = render_system_with_override(
            AgentProviderApiType::OpenAi,
            &model,
            &[],
            &[],
            false,
            &[],
            Some(&PromptSource::Builtin("default".into())),
        );
        assert_ne!(auto, forced_default, "覆盖为 default 应改变输出");
    }

    #[test]
    fn unknown_builtin_override_falls_back_to_auto() {
        // 拼错的内置名(system/does-not-exist.j2 不存在)不该发出坏 prompt,
        // 而是回退到按 model family 的自动命中。
        let model = LLMId::from("byop:p:claude-sonnet-4-5");
        let auto = render_system(AgentProviderApiType::OpenAi, &model, &[], &[], false, &[]);
        let bogus = render_system_with_override(
            AgentProviderApiType::OpenAi,
            &model,
            &[],
            &[],
            false,
            &[],
            Some(&PromptSource::Builtin("does-not-exist".into())),
        );
        assert_eq!(auto, bogus);
    }

    #[test]
    fn custom_file_override_renders_with_shared_partials() {
        // 自定义 prompt 文件可以 include 内置 partials(共用同一个 env)。
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(
            tmp.path().join("mine.j2"),
            "CUSTOM {{ model_id }}\n{% include \"partials/footer.j2\" %}",
        )
        .unwrap();

        let env = build_env();
        let ctx = PromptContext {
            model_id: "my-model".into(),
            ..Default::default()
        };
        let out = render_custom_file_from(&env, tmp.path(), "mine.j2", &ctx).unwrap();
        assert!(out.starts_with("CUSTOM my-model"), "自定义正文应生效: {out}");
        // footer.j2 的内容应被 include 进来(与内置 default 渲染共享该 partial)。
        let footer = env
            .get_template("partials/footer.j2")
            .unwrap()
            .render(Value::from_serialize(&ctx))
            .unwrap();
        assert!(
            !footer.trim().is_empty() && out.contains(footer.trim()),
            "footer partial 应被 include: out={out}"
        );
    }

    #[test]
    fn custom_file_override_rejects_path_traversal() {
        let env = build_env();
        let ctx = PromptContext::default();
        let dir = Path::new("/tmp/prompts");
        assert!(
            render_custom_file_from(&env, dir, "../etc/passwd", &ctx).is_err(),
            ".. 路径必须被拒绝"
        );
        assert!(
            render_custom_file_from(&env, dir, "/etc/passwd", &ctx).is_err(),
            "绝对路径必须被拒绝"
        );
        assert!(
            render_custom_file_from(&env, dir, "sub/../../escape.j2", &ctx).is_err(),
            "夹带 .. 的多段路径必须被拒绝"
        );
    }

    #[test]
    fn missing_custom_file_returns_err() {
        let tmp = tempfile::tempdir().unwrap();
        let env = build_env();
        let ctx = PromptContext::default();
        assert!(
            render_custom_file_from(&env, tmp.path(), "nope.j2", &ctx).is_err(),
            "a missing file should return Err (caller falls back to auto)"
        );
    }

    // -- active-ai templates in the shared hot-reload env --------------------

    #[test]
    fn active_ai_templates_registered_in_shared_env() {
        // After folding them in, the active-ai templates must resolve from the
        // shared (hot-reloadable) env so they can be overridden from the prompt dir.
        for name in [
            "active_ai/prompt_suggestions_system.j2",
            "active_ai/prompt_suggestions_user.j2",
            "active_ai/nld_predict_system.j2",
            "active_ai/nld_predict_user.j2",
            "active_ai/relevant_files_system.j2",
            "active_ai/relevant_files_user.j2",
            "active_ai/next_command_system.j2",
            "active_ai/next_command_user.j2",
            "active_ai/workflow_metadata_system.j2",
            "active_ai/workflow_metadata_user.j2",
        ] {
            assert!(
                build_env().get_template(name).is_ok(),
                "{name} should be registered in the shared env"
            );
        }
    }

    #[test]
    fn render_template_renders_active_ai_builtin() {
        let out = render_template("active_ai/nld_predict_system.j2", Value::from(true));
        assert!(
            !out.is_empty(),
            "built-in active-ai prompt should render non-empty: {out}"
        );
    }

    #[test]
    fn render_template_unknown_name_is_empty() {
        // A missing template degrades to empty (never panics) — matches the old
        // active_ai::render behavior for auxiliary prompts.
        assert_eq!(
            render_template("active_ai/does-not-exist.j2", Value::from(true)),
            ""
        );
    }

    #[test]
    fn custom_prompt_raw_rejects_traversal() {
        // Guard runs before the dir lookup, so these hold regardless of global state.
        assert!(custom_prompt_raw("../secret").is_none());
        assert!(custom_prompt_raw("/etc/passwd").is_none());
        assert!(custom_prompt_raw("a/../../b").is_none());
    }

    #[test]
    fn render_omits_tool_list_when_empty() {
        // tool_names 为空(理论上不会发生,兜底:不渲染白名单段)
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &[],
        );
        assert!(!out.contains("Available Tools"), "{out}");
    }

    #[test]
    fn plan_mode_off_omits_plan_block() {
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &[],
        );
        assert!(
            !out.contains("Plan Mode (Read-Only)"),
            "plan_mode=false 不应包含 Plan Mode 段: {out}"
        );
    }

    #[test]
    fn plan_mode_on_injects_plan_block_for_all_families() {
        for id in [
            "claude-sonnet-4-5",
            "gpt-4o",
            "gpt-5-codex",
            "gemini-2.5-pro",
            "kimi-k2",
            "trinity-v1",
            "deepseek-chat",
            "weird-model",
        ] {
            let out = render_system(
                AgentProviderApiType::OpenAi,
                &LLMId::from(format!("byop:p:{id}").as_str()),
                &[],
                &[],
                true,
                &[],
            );
            assert!(
                out.contains("Plan Mode (Read-Only)"),
                "id={id} plan_mode=true 应包含 Plan Mode 段: {out}"
            );
            assert!(
                out.contains("Stop and wait"),
                "id={id} plan_mode=true 应包含 Stop and wait 引导: {out}"
            );
        }
    }

    // Issue #116:全局 Rules(用户在 设置 → Agents → Rules 创建)必须注入 system prompt。
    // 下面三个用例覆盖 `partials/user_rules.j2` 的关键分支。

    #[test]
    fn render_omits_user_rules_block_when_empty() {
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &[],
        );
        assert!(
            !out.contains("# User rules"),
            "user_rules 为空时不应渲染 user rules 区块: {out}"
        );
    }

    #[test]
    fn render_includes_user_rules_when_present() {
        let rules = vec![(
            Some("My rule".to_string()),
            "Always use snake_case in Rust.".to_string(),
        )];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &rules,
        );
        assert!(
            out.contains("# User rules"),
            "应渲染 user rules 区块: {out}"
        );
        assert!(out.contains("## My rule"), "应包含规则名: {out}");
        assert!(
            out.contains("Always use snake_case in Rust."),
            "应包含规则内容: {out}"
        );
    }

    #[test]
    fn render_includes_user_rules_across_all_template_families() {
        // user_rules.j2 经 footer.j2 注入,所有 system 模板族都引用了 footer。
        // 这个回归用例确保 anthropic / beast / codex / gemini / kimi / trinity /
        // default 任一模板族都会渲染 user rules,不会因为某条家族没拉 footer 而漏注入。
        let rules = vec![(Some("家族覆盖".to_string()), "snake_case only.".to_string())];
        for id in [
            "claude-sonnet-4-5",
            "gpt-4o",
            "gpt-5-codex",
            "gemini-2.5-pro",
            "kimi-k2",
            "trinity-v1",
            "deepseek-chat",
            "weird-model",
        ] {
            let out = render_system(
                AgentProviderApiType::OpenAi,
                &LLMId::from(format!("byop:p:{id}").as_str()),
                &[],
                &[],
                false,
                &rules,
            );
            assert!(
                out.contains("snake_case only."),
                "id={id} 应包含 user rule 内容: {out}"
            );
        }
    }

    #[test]
    fn render_user_rules_separates_multiple_rules_with_blank_line() {
        // 多条规则之间应有空行分隔(`{% if not loop.last %}`),最后一条之后不留空行。
        let rules = vec![
            (Some("R1".to_string()), "first content".to_string()),
            (Some("R2".to_string()), "second content".to_string()),
            (Some("R3".to_string()), "third content".to_string()),
        ];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &rules,
        );

        // 两条规则之间应至少包含一个 "blank line"(两个相邻换行)。
        // 不写死具体换行数,因为 minijinja 的 trim_blocks/lstrip_blocks 默认行为
        // 决定的具体换行数容易随模板微调而变(reviewer 实测出过 3 个换行的形态)。
        // 我们要的契约是"有视觉空行 + 顺序正确"。
        let pos_r1 = out.find("first content").expect("找不到 R1 content");
        let pos_r2 = out.find("## R2").expect("找不到 R2 标题");
        let pos_r3 = out.find("## R3").expect("找不到 R3 标题");
        assert!(pos_r1 < pos_r2 && pos_r2 < pos_r3, "顺序应保持: {out}");
        let between_r1_r2 = &out[pos_r1 + "first content".len()..pos_r2];
        let between_r2_r3 = &out[pos_r2..pos_r3];
        assert!(
            between_r1_r2.contains("\n\n"),
            "R1 与 R2 之间应有空行,实际:{between_r1_r2:?}"
        );
        assert!(
            between_r2_r3.contains("\n\n"),
            "R2 与 R3 之间应有空行,实际:{between_r2_r3:?}"
        );
    }

    #[test]
    fn render_user_rules_handles_no_name() {
        let rules = vec![(None, "Be terse.".to_string())];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:deepseek-chat"),
            &[],
            &[],
            false,
            &rules,
        );
        assert!(out.contains("# User rules"), "{out}");
        assert!(out.contains("Be terse."), "{out}");
        // 无 name 时不应渲染空的 `## ` 标题行
        assert!(
            !out.contains("## \n"),
            "无 name 时不应渲染空的 '## ' 标题: {out}"
        );
    }

    #[test]
    fn render_includes_thinking_language_across_all_template_families() {
        // thinking_language.j2 经 footer.j2 注入,所有 system 模板族都引用了 footer。
        // 回归用例确保 8 族模板都会渲染 thinking_language,不会因为某条家族没拉 footer
        // 而漏注入,导致 LLM 在中文用户提问时仍用英文思考。
        // 8 族对应: anthropic / gpt / beast / codex / gemini / kimi / trinity / default
        for id in [
            "claude-sonnet-4-5",
            "gpt-3.5-turbo",
            "gpt-4o",
            "gpt-5-codex",
            "gemini-2.5-pro",
            "kimi-k2",
            "trinity-v1",
            "weird-model",
        ] {
            let out = render_system(
                AgentProviderApiType::OpenAi,
                &LLMId::from(format!("byop:p:{id}").as_str()),
                &[],
                &[],
                false,
                &[],
            );
            assert!(
                out.contains("# Thinking language"),
                "id={id} 应渲染 thinking_language 区块: {out}"
            );
            assert!(
                out.contains("internal reasoning"),
                "id={id} 应包含 thinking_language 锚点: {out}"
            );
        }
    }

    #[test]
    fn render_thinking_language_precedes_tool_aliases() {
        // meta-rule 应在工具列表之前,不被 user_rules / project_rules 覆盖。
        // 需要传一个非空 tool 列表,否则 tool_aliases.j2 整个块被 {% if available_tools %} 跳过。
        let tools = vec!["read_files".to_string()];
        let out = render_system(
            AgentProviderApiType::OpenAi,
            &LLMId::from("byop:p:claude-sonnet-4-5"),
            &[],
            &tools,
            false,
            &[],
        );
        let pos_thinking = out
            .find("# Thinking language")
            .expect("应包含 thinking_language");
        let pos_tools = out.find("# Available Tools").expect("应包含 tool_aliases");
        assert!(
            pos_thinking < pos_tools,
            "thinking_language 应在 tool_aliases 之前: thinking={pos_thinking}, tools={pos_tools}\n{out}"
        );
    }
}
