//! BYOP system prompt template rendering.
//!
//! Renders the `AIAgentContext` (env / git / skills / project_rules / current_time) that the
//! warp client already collected into the `system` message string for an OpenAI-compatible endpoint.
//!
//! ## Workflow
//!
//! 1. Extract the most recent `UserQuery.context: Arc<[AIAgentContext]>` from `params.input`
//!    (warp's `convert_to.rs::convert_input` reads the same one)
//! 2. `collect_prompt_context` flattens each enum variant into a flat `PromptContext` struct
//! 3. `pick_template` selects `system/{anthropic,gpt,beast,codex,
//!    gemini,kimi,trinity,default}.j2` by model-id substring match (mirrors opencode
//!    `packages/opencode/src/session/system.ts::provider`)
//! 4. minijinja rendering
//!
//! ## Template loading
//!
//! All templates are `include_str!`'d into the binary (zero runtime IO); changing a template requires a rebuild.
//!
//! Exception: once `ZAP_PROMPT_DIR` (or Settings -> AI -> Prompt template directory) is set,
//! templates are re-read from that directory by name; saving takes effect immediately, no rebuild. Missing files / syntax errors fall back per-file to the built-in
//! version. See [`PROMPT_DIR_ENV`].
//!
//! Overridable assets live in two tables:
//! - [`EMBEDDED`] -- the minijinja `.j2` templates (the system prompt and its partials).
//!   Has an mtime cache; if nothing changed, the parsed Environment is reused.
//! - [`EMBEDDED_RAW`] -- plain text fed directly to the model (13 tool descriptions +
//!   the conversation-title and commit-message prompts). Does **not** go through minijinja;
//!   see that constant's comment.

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

/// Name of the environment variable for the template hot-reload override directory.
///
/// Unset -> use the `include_str!`'d copy in [`ENV`] compiled into the binary, zero runtime IO (default behavior unchanged).
/// Set -> every render re-reads from that directory by template name (relative paths like `system/local.j2`),
/// so editing a template and saving takes effect immediately, without rebuilding the `app` crate (800k lines, where changing one line of a prompt otherwise forces a full relink).
///
/// A missing file / read failure / syntax error each **falls back per-file to the built-in version**, never panicking -- hot-reload is a
/// dev-time convenience and should not let one fat-fingered template edit interrupt an in-progress session.
const PROMPT_DIR_ENV: &str = "ZAP_PROMPT_DIR";

/// Table of (name, built-in source) for all templates. The name is also the relative path under `ZAP_PROMPT_DIR`.
///
/// Dispatches the system prompt by model-id substring match (mirrors opencode
/// `packages/opencode/src/session/system.ts::provider`). OpenRouter paths like
/// `anthropic/claude-3.5-sonnet` / `google/gemini-2.5-flash` / `openai/gpt-4o`
/// also match correctly. An unrecognized family falls back to default.j2, so custom model ids are safe.
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
    (
        "system/troubleshooting.j2",
        include_str!("prompts/system/troubleshooting.j2"),
    ),
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

/// Table of plain-text assets (not run through minijinja). The name is likewise the relative path under `ZAP_PROMPT_DIR`.
///
/// Kept separate from [`EMBEDDED`] **on purpose**: these are markdown fed directly to the model, not templates.
/// Putting them in the Environment would have jinja parse them -- `websearch.md` contains the literal `{{year}}`
/// (substituted by `chat_stream::build_tools_array` itself), which jinja parsing would destroy.
///
/// Tool descriptions are looked up by **tool name**: `tool_descriptions/{name}.md`.
/// The 13 tools that currently have their own file map name-to-filename one-to-one;
/// the documents / markers / suggest descriptions live in code and are not overridable.
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
    (
        "tasks/commit_message_system.md",
        include_str!("prompts/tasks/commit_message_system.md"),
    ),
];

/// Look up a plain-text asset: use the override version if present in the override dir, otherwise the built-in.
///
/// Returns `Cow` rather than `&'static str`: the override version is an owned String read at runtime.
/// An unregistered asset name returns `None` (callers should pass compile-time constants, so this shouldn't happen).
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
                "[byop prompt] read {} failed: {e} — using built-in version",
                path.display()
            ),
        }
    }
    Some(Cow::Borrowed(embedded))
}

/// Get a tool's description: `tool_descriptions/{tool_name}.md` in the override dir takes priority,
/// otherwise use `fallback` (the copy `include_str!`'d in from the registry).
///
/// Skips the [`CACHE`] mtime cache: a single request only looks up these dozen-odd files, each a few KB,
/// and only reads from disk when hot-reload is on; building a separate cache for it isn't worth it.
pub fn tool_description(tool_name: &str, fallback: &'static str) -> Cow<'static, str> {
    if active_override_dir().is_none() {
        return Cow::Borrowed(fallback);
    }
    raw_asset(&format!("tool_descriptions/{tool_name}.md")).unwrap_or(Cow::Borrowed(fallback))
}

/// Get the system prompt used for conversation-title generation (`tasks/title_system.md`).
/// The built-in version lives in [`EMBEDDED_RAW`], so callers don't need to pass a fallback.
pub fn title_system_prompt() -> Cow<'static, str> {
    raw_asset("tasks/title_system.md")
        .expect("tasks/title_system.md is registered in EMBEDDED_RAW")
}

/// Get the system prompt used for AI commit-message generation
/// (`tasks/commit_message_system.md`).
///
/// Sits alongside [`title_system_prompt`]: both are plain markdown fed straight
/// to the model by a one-shot call, and both pick up a hot-reloaded copy from
/// the prompt template dir when one is configured.
pub fn commit_message_system_prompt() -> Cow<'static, str> {
    raw_asset("tasks/commit_message_system.md")
        .expect("tasks/commit_message_system.md is registered in EMBEDDED_RAW")
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
    // Defense in depth beyond the `..`/absolute check above: resolve symlinks and
    // confirm the target still lives inside the prompt dir, so a symlink placed in
    // the dir can't be used to read a file outside it.
    if let (Ok(canon_dir), Ok(canon_path)) =
        (std::fs::canonicalize(&dir), std::fs::canonicalize(&path))
    {
        if !canon_path.starts_with(&canon_dir) {
            log::error!(
                "[byop prompt] custom prompt {rel:?} resolves outside the prompt dir; refusing"
            );
            return None;
        }
    }
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

/// Export the built-in templates + plain-text assets to `dir`, returning the number of files actually written.
///
/// The semantics are "fill in the gaps, don't overwrite":
/// - Existing files are always skipped -- user edits must not be wiped out by this action.
/// - So it's safe to click repeatedly: templates added by an upgrade get filled in, old edits are kept as-is.
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
        "[byop prompt] exported built-in templates to {} — wrote {written} new file(s) (existing ones skipped)",
        dir.display()
    );
    Ok(written)
}

/// The default suggested path shown by the settings panel (`~/.zap/prompts`). Returns `None` when home can't be resolved.
pub fn default_prompts_dir() -> Option<PathBuf> {
    warp_core::paths::warp_home_prompts_dir()
}

/// The current template hot-reload status, so the settings panel can show a visible indicator --
/// otherwise "set a dir but it didn't take effect" and "forgot to set a dir" are both silent states, and the user just sees the built-in templates go out as usual.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverrideStatus {
    /// No effective override dir (setting empty and `ZAP_PROMPT_DIR` unset) -> use built-in templates.
    Inactive,
    /// Override is active.
    Active {
        /// The directory actually in effect.
        dir: PathBuf,
        /// Whether it comes from the `ZAP_PROMPT_DIR` environment variable (higher priority than the settings panel).
        from_env: bool,
        /// Number of overridable files that actually exist in the override dir.
        on_disk: usize,
        /// Total number of overridable files (`EMBEDDED` + `EMBEDDED_RAW`).
        total: usize,
    },
}

/// Compute the [`OverrideStatus`]. Only stats whether files exist; does not read contents.
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

/// Build an Environment overridden from `dir`. Tries to read each template from disk, falling back to the built-in source on failure.
fn build_env_from_dir(dir: &Path) -> Environment<'static> {
    let mut env = Environment::new();
    let mut overridden = 0usize;

    for (name, embedded) in EMBEDDED {
        // name is a compile-time constant (like `system/local.j2`), contains no user input, so join has no traversal risk.
        let path = dir.join(name);
        let src = match std::fs::read_to_string(&path) {
            Ok(s) => Some(s),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
            Err(e) => {
                log::warn!("[byop prompt] read {} failed: {e} — using built-in version", path.display());
                None
            }
        };

        match src {
            // Read but has a syntax error: fall back to built-in and report the error (otherwise the user only sees
            // "edited the template but it didn't take effect" with no way to find the cause).
            Some(s) => match env.add_template_owned(*name, s) {
                Ok(()) => overridden += 1,
                Err(e) => {
                    log::error!(
                        "[byop prompt] {} has a syntax error, falling back to built-in: {e}",
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
        "[byop prompt] hot-reload {}: {overridden}/{} template(s) loaded from disk",
        dir.display(),
        EMBEDDED.len()
    );
    env
}

/// Environment handle used for rendering.
///
/// The default path returns the `&'static` in the `OnceLock` (identical to before hot-reload was introduced, zero runtime IO);
/// only with hot-reload on does it go through [`CACHE`]: each render stats the templates' mtimes once and, if unchanged, reuses
/// the already-parsed copy, re-parsing only when something changed.
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

/// The override dir written in by the settings panel (Settings -> AI -> Prompt template directory).
///
/// `prompt_renderer` is a set of free functions with no access to `warpui::AppContext`, so it can't read
/// `AISettings` directly; instead the settings layer calls [`set_override_dir`] at startup / on settings change
/// to push the value in. Lower priority than [`PROMPT_DIR_ENV`] -- the env var is for temporary debugging and
/// should override the persisted config.
static OVERRIDE_DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Called by the settings layer to push the current value of Settings -> AI -> Prompt template directory.
/// Passing `None` / an empty string disables hot-reload (back to built-in templates).
pub fn set_override_dir(dir: Option<PathBuf>) {
    let dir = dir.filter(|p| !p.as_os_str().is_empty());
    match OVERRIDE_DIR.write() {
        Ok(mut slot) => {
            if *slot != dir {
                match &dir {
                    Some(p) => log::info!("[byop prompt] template hot-reload dir → {}", p.display()),
                    None => log::info!("[byop prompt] template hot-reload disabled, using built-in templates"),
                }
                *slot = dir;
            }
        }
        Err(e) => log::error!("[byop prompt] OVERRIDE_DIR lock poisoned, ignoring this update: {e}"),
    }
}

/// The currently-effective Prompt template directory (env var first, then the
/// settings panel). Exposed for the UI to validate / resolve the relative paths of
/// each slot's custom prompt file.
pub fn active_prompt_dir() -> Option<PathBuf> {
    active_override_dir()
}

/// Resolve the override dir to use for this render: env var first, then the settings panel.
fn active_override_dir() -> Option<PathBuf> {
    // Read the env var every time (rather than caching once at startup), so changing the env and reopening a session takes effect.
    // The cost is one getenv, negligible relative to an LLM request.
    if let Some(dir) = std::env::var_os(PROMPT_DIR_ENV) {
        if !dir.is_empty() {
            return Some(PathBuf::from(dir));
        }
    }
    // On a poisoned lock, degrade to "no override" rather than panicking -- hot-reload shouldn't take down a session.
    OVERRIDE_DIR.read().ok().and_then(|slot| slot.clone())
}

/// mtime snapshot of each template in the override dir. `None` = that file currently doesn't exist
/// (recording this is necessary: creating a previously-missing override file must also trigger a rebuild).
type Stamps = Vec<Option<SystemTime>>;

/// A parsed override Environment + its source directory + the mtime snapshot at that time.
struct CachedEnv {
    dir: PathBuf,
    stamps: Stamps,
    env: Arc<Environment<'static>>,
}

static CACHE: RwLock<Option<CachedEnv>> = RwLock::new(None);

/// stat each template in the override dir to take an mtime snapshot. Only stats, doesn't read contents.
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

    // Hit condition: the directory didn't change + every template's mtime is unchanged.
    // The common path (no template edits) is only ~20 stats + one Arc clone, no re-parse.
    //
    // Note mtime has only 1-second granularity on some filesystems: two consecutive edits within the same second may
    // go unnoticed. Hand-editing templates won't hit this; even if it does, only that one render uses the old template,
    // and the next one is fine. Hashing to avoid this isn't worth it.
    let stamps = stamp_dir(&dir);
    if let Ok(cache) = CACHE.read() {
        if let Some(cached) = cache.as_ref() {
            if cached.dir == dir && cached.stamps == stamps {
                return EnvHandle::Cached(Arc::clone(&cached.env));
            }
        }
    }

    let env = Arc::new(build_env_from_dir(&dir));
    // A failed cache write (poisoned lock) doesn't affect this render, it just means the next one re-parses again.
    match CACHE.write() {
        Ok(mut slot) => {
            *slot = Some(CachedEnv {
                dir,
                stamps,
                env: Arc::clone(&env),
            })
        }
        Err(e) => log::error!("[byop prompt] template cache lock poisoned, skipping cache this time: {e}"),
    }
    EnvHandle::Cached(env)
}

// ---------------------------------------------------------------------------
// Template selection
// ---------------------------------------------------------------------------

/// Select a template by model-id substring match (mirrors opencode
/// `packages/opencode/src/session/system.ts::provider`)。
///
/// Ollama / local BYOP uses [`pick_template`]'s short `local.j2` template (see the `api_type` parameter),
/// to avoid the 9k+ default.j2 drowning a small model's conversation context.
pub fn pick_template(model_id: &str, api_type: AgentProviderApiType) -> &'static str {
    if api_type == AgentProviderApiType::Ollama {
        return "system/local.j2";
    }
    pick_template_by_model(model_id)
}

/// Select a template by model-id substring match (without provider-level override).
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

/// Extract the model-id string from an `LLMId`. BYOP encoding takes the model part,
/// otherwise returns it as-is (in theory the BYOP path only passes BYOP ids, but this is a fallback).
fn model_id_from_llm_id(id: &LLMId) -> String {
    if let Some((_pid, mid)) = super::llm_id::decode(id) {
        mid
    } else {
        id.as_str().to_owned()
    }
}

// ---------------------------------------------------------------------------
// AIAgentContext -> flat template context
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

/// Zap BYOP fix for Issue #116: a flat view of the global Rules (created by the user in
/// Settings -> Agents -> Rules), fed to `partials/user_rules.j2` to render into the system prompt.
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
    /// Zap BYOP fix for Issue #116: injected by the caller (`render_system`) from
    /// `RequestParams.user_rules`, rendered via `partials/user_rules.j2`.
    user_rules: Vec<UserRuleCtx>,
    current_time: String,
    model_id: String,
    /// The list of tool names actually fed to the upstream model this turn (computed by `chat_stream::available_tool_names`,
    /// including the gated built-in tools and the current MCP tools).
    /// The template renders the whitelist dynamically from this, no longer hardcoded.
    available_tools: Vec<String>,
    /// Whether this turn is in the `/plan`-triggered Plan Mode (read-only research mode).
    /// Computed by `chat_stream::is_plan_mode_turn`; the template uses it to include
    /// `partials/plan_mode.j2`, injecting the read-only constraints + plan-output guidance.
    plan_mode: bool,
}

fn collect_prompt_context(model_id: &str, ctx: &[AIAgentContext]) -> PromptContext {
    let mut out = PromptContext {
        // P0-1 prompt cache optimization: `current_time` is kept only to calendar-day granularity,
        // no longer down to the second. Reasons:
        // - Any per-request-changing content in the system prompt makes the hash written by
        //   Anthropic's first system breakpoint unique -> discarded as soon as written, never a hit.
        //   OpenAI's first-256-token routing hash is the same, scattering requests across different machines.
        // - The model really only needs to know "what day it is today", so the one miss when crossing
        //   a calendar day is acceptable (one day x all active conversations x system tokens).
        // - Crossing a year has the same cost as crossing a day, no extra handling needed.
        // A later step could move "current time" to the end of the user message (P0-1 option C),
        // making the system section 100% stable; this step takes the lower-risk option B first.
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
                // P0-1: consistent with the default, keep only calendar-day granularity.
                // Upstream Zap may pass a second-precise timestamp; here we uniformly collapse it to "current date".
                out.current_time = current_time.format("%Y-%m-%d").to_string();
            }
            // Code indexing is not implemented, so Codebase context does not go into the system prompt.
            AIAgentContext::Codebase { .. } => {}
            // P1-7 prompt cache note: `Git { head, branch }` depends on the current repo state,
            // so switching branches changes the rendered system section, invalidating the system+messages
            // cache of all upstream providers (Anthropic / OpenAI / DeepSeek).
            // This is **expected behavior**:
            //   - the model must not treat the old git context as valid on a new branch;
            //   - as the cost, the first request on a new branch is a 100% miss and writes a new cache, after which that
            //     branch reuses it. Developers who jump between branches often will see the most misses.
            // Considered alternative: move git state to the end of the user message (same as P0-1 option C),
            // but then the system section loses the contextual meaning of "the model sees the current branch at a glance",
            // and models that rely on it for reasoning get worse. This patch keeps the status quo.
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
            // User-attachment context (File / Image / SelectedText / Block) does not go into the system prompt;
            // it's injected into the current turn's user message by `user_context::render_user_attachments`
            // in chat_stream's UserQuery branch. This mirrors warp's own two-category split:
            // - environment type -> InputContext.{directory,shell,git,...} -> backend injects into the system section
            // - attachment type -> InputContext.{executed_shell_commands,selected_text,files,images}
            //            -> backend injects into the user section
            AIAgentContext::File(_)
            | AIAgentContext::Image(_)
            | AIAgentContext::SelectedText(_)
            | AIAgentContext::Block(_) => {}
            // Repository identity and pull-request metadata reach the Warp API
            // through `input_context::Git.{repository,pull_request}`; the BYOP
            // system prompt has no corresponding section yet, so they are not
            // rendered here. Tracked as a follow-up (see the PR that added
            // `AIAgentContext::{Repository,PullRequest}`).
            AIAgentContext::Repository { .. } | AIAgentContext::PullRequest { .. } => {}
        }
    }

    out
}

// ---------------------------------------------------------------------------
// Public API
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

/// Render the final system message string sent to the upstream model.
///
/// `ctx` usually comes from the most recent `AIAgentInput::UserQuery.context` in `params.input`.
/// No context (empty array) is fine too -- the template renders with default placeholders.
///
/// `available_tools` is computed by `chat_stream::available_tool_names`: the list of tool names actually exposed
/// to the upstream LLM this turn (built-in + MCP, gating already applied). The template renders the whitelist dynamically from this,
/// no longer hardcoding an "unavailable tools" blacklist -- the model naturally won't call tools it can't see,
/// whereas a text blacklist would make the model afraid to call even genuinely-available tools.
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
    // Defense in depth beyond the `..`/absolute check above (mirrors `custom_prompt_raw`):
    // resolve symlinks and confirm the target still lives inside the prompt dir, so a
    // symlink placed in the dir (e.g. `system/local.j2` -> `/etc/shadow`) can't be used to
    // render an arbitrary file outside it into the system prompt sent to the model / shown
    // in the UI. A canonicalize failure (missing file, broken link) is left to the
    // `read_to_string` below, which reports it as an ordinary read error.
    if let (Ok(canon_dir), Ok(canon_path)) =
        (std::fs::canonicalize(dir), std::fs::canonicalize(&path))
    {
        if !canon_path.starts_with(&canon_dir) {
            return Err(format!(
                "path {rel:?} resolves outside the prompt dir; refusing"
            ));
        }
    }
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

/// Fallback system render (only used when template loading/rendering fails; should never trigger on the normal path).
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

    // -- Template hot-reload (ZAP_PROMPT_DIR) --------------------------------
    //
    // All tests hit `build_env_from_dir` directly (a pure function taking a path), not `env()`.
    // Because `env()` reads a process-level env var, and cargo test runs multi-threaded by default,
    // set_var/remove_var would fight across tests. `env()` itself is just
    // "read var -> pick one of two", logic too thin to be worth pulling in serial_test for.

    /// Write an override file under dir by template name (creating parent dirs automatically).
    fn write_override(dir: &Path, name: &str, content: &str) {
        let path = dir.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, content).unwrap();
    }

    #[test]
    fn hot_reload_empty_dir_falls_back_to_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        let env = build_env_from_dir(tmp.path());
        // No template overridden -> the rendered result should match the built-in version
        let embedded = build_env();
        for (name, _) in EMBEDDED {
            assert!(env.get_template(name).is_ok(), "{name} should exist");
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
        // Only local.j2 is overridden; other templates must still be the built-in version -- overriding is not "all or nothing".
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
        // anthropic.j2 is not overridden -> it should still render the built-in content
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
        // The include chain must use the override version: local.j2 includes partials/env.j2,
        // so overriding just the partial should also show up in the final system prompt.
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "partials/env.j2", "PARTIAL-OVERRIDE");

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();

        assert!(out.contains("PARTIAL-OVERRIDE"), "{out}");
        // The built-in local.j2 body is still there (only the partial was swapped)
        assert!(out.contains("run_shell_command"), "{out}");
    }

    #[test]
    fn hot_reload_bad_syntax_falls_back_to_embedded() {
        // A fat-fingered broken template should not panic, nor make the template disappear -- fall back to the built-in version.
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "{% if unclosed %}");

        let env = build_env_from_dir(tmp.path());
        let out = env
            .get_template("system/local.j2")
            .unwrap()
            .render(Value::from_serialize(&PromptContext::default()))
            .unwrap();

        assert!(out.contains("run_shell_command"), "should fall back to the built-in local.j2: {out}");
    }

    #[test]
    fn hot_reload_unrelated_files_in_dir_are_ignored() {
        // Miscellaneous files in the override dir do not participate in loading (only names in the EMBEDDED table are looked up).
        let tmp = tempfile::tempdir().unwrap();
        std::fs::write(tmp.path().join("README.md"), "noise").unwrap();
        write_override(tmp.path(), "system/nonexistent.j2", "noise");

        let env = build_env_from_dir(tmp.path());
        assert!(env.get_template("system/nonexistent.j2").is_err());
        assert!(env.get_template("system/local.j2").is_ok());
    }

    #[test]
    fn hot_reload_rereads_after_edit() {
        // The core promise of hot-reload: edit and save, and the next render is the new one (no caching).
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
        // Configured a nonexistent directory (typo'd path / unmounted external drive) -> full fallback, no panic.
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
        // Empty dir -> every entry is None (files don't exist)
        assert!(stamps.iter().all(|s| s.is_none()));
    }

    #[test]
    fn stamp_dir_marks_present_files_and_only_those() {
        // Creating a previously-missing override file must flip the snapshot from None to Some,
        // otherwise "putting in an override file for the first time" wouldn't trigger a rebuild.
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "X");

        let stamps = stamp_dir(tmp.path());
        let idx = EMBEDDED
            .iter()
            .position(|(n, _)| *n == "system/local.j2")
            .unwrap();

        assert!(stamps[idx].is_some(), "a written template should have an mtime");
        assert_eq!(
            stamps.iter().filter(|s| s.is_some()).count(),
            1,
            "only the template that was written has an mtime"
        );
    }

    #[test]
    fn stamp_dir_distinguishes_directories() {
        // A cache hit requires the same dir; different dirs shouldn't reuse each other even with identical content.
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        write_override(a.path(), "system/local.j2", "X");

        assert_ne!(stamp_dir(a.path()), stamp_dir(b.path()));
    }

    // -- Plain-text assets (tool descriptions / title) -----------------------
    //
    // These go through `active_override_dir()`, i.e. read a process-level env var, so they can't be
    // bypassed purely by passing a path like above. The default (ZAP_PROMPT_DIR unset, set_override_dir not pushed)
    // behavior is deterministic, so we only test that side; the override-active path is tested directly by `raw_asset_*`.

    #[test]
    fn tool_description_without_override_borrows_fallback() {
        let out = tool_description("grep", "FALLBACK");
        assert_eq!(out, "FALLBACK");
        assert!(matches!(out, Cow::Borrowed(_)), "the default path should not allocate");
    }

    #[test]
    fn tool_description_unknown_tool_falls_back() {
        // documents / markers / suggest have no .md file, so they must fall back to
        // the hardcoded description in the registry, not become an empty string.
        assert_eq!(
            raw_asset("tool_descriptions/read_documents.md").as_deref(),
            None
        );
    }

    #[test]
    fn raw_asset_returns_embedded_for_registered_names() {
        let out = raw_asset("tool_descriptions/websearch.md").unwrap();
        assert!(!out.is_empty());
        // websearch.md contains the literal {{year}}, substituted by chat_stream itself.
        // Here we also confirm it wasn't eaten by jinja (EMBEDDED_RAW does not go through minijinja).
        assert!(out.contains("{{year}}"), "{out}");
    }

    #[test]
    fn raw_asset_unregistered_name_is_none() {
        assert!(raw_asset("tool_descriptions/not_a_tool.md").is_none());
        assert!(raw_asset("system/local.j2").is_none(), "templates are not in the RAW table");
    }

    #[test]
    fn title_system_prompt_has_language_placeholder() {
        let out = title_system_prompt();
        assert!(
            out.contains("{{ language }}"),
            "chat_stream relies on this placeholder for substitution: {out}"
        );
    }

    #[test]
    fn embedded_raw_covers_every_tool_description_file() {
        // Regression guard: adding a tool_descriptions/*.md but forgetting to register it -> that tool can't be hot-reloaded.
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

        assert_eq!(on_disk, registered, "tool_descriptions/ and EMBEDDED_RAW disagree");
    }

    #[test]
    fn embedded_raw_names_match_tool_names() {
        // Overrides are looked up by tool name as `tool_descriptions/{name}.md`,
        // so a name mismatch fails silently -- this pins that contract.
        for (name, _) in EMBEDDED_RAW
            .iter()
            .filter(|(n, _)| n.starts_with("tool_descriptions/"))
        {
            let stem = name
                .trim_start_matches("tool_descriptions/")
                .trim_end_matches(".md");
            assert!(
                super::super::tools::REGISTRY.iter().any(|t| t.name == stem),
                "{stem} has no tool of the same name, so the override can't be found"
            );
        }
    }

    // -- Export built-in templates (one-click seed) --------------------------

    #[test]
    fn seed_dir_writes_every_overridable_file() {
        let tmp = tempfile::tempdir().unwrap();
        let n = seed_dir(tmp.path()).unwrap();

        assert_eq!(n, EMBEDDED.len() + EMBEDDED_RAW.len());
        for (name, content) in EMBEDDED.iter().chain(EMBEDDED_RAW.iter()) {
            let path = tmp.path().join(name);
            assert!(path.is_file(), "{name} should be exported");
            assert_eq!(std::fs::read_to_string(&path).unwrap(), *content);
        }
    }

    #[test]
    fn seed_dir_output_is_loadable() {
        // The exported tree must be readable back by build_env_from_dir as-is -- otherwise the user clicks the button
        // and gets a pile of files that won't load.
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
        // A template the user edited must not be wiped by re-exporting.
        let tmp = tempfile::tempdir().unwrap();
        write_override(tmp.path(), "system/local.j2", "MINE");

        let n = seed_dir(tmp.path()).unwrap();

        assert_eq!(
            std::fs::read_to_string(tmp.path().join("system/local.j2")).unwrap(),
            "MINE"
        );
        assert_eq!(n, EMBEDDED.len() + EMBEDDED_RAW.len() - 1, "only fills in the missing ones");
    }

    #[test]
    fn seed_dir_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let first = seed_dir(tmp.path()).unwrap();
        let second = seed_dir(tmp.path()).unwrap();

        assert!(first > 0);
        assert_eq!(second, 0, "the second click has no new files to write");
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
        // tasks/compaction_* is in neither EMBEDDED nor EMBEDDED_RAW: editing it
        // has no effect, so exporting it would be misleading. (active_ai/* IS
        // exported now — it moved into EMBEDDED to be overridable + hot-reloaded.)
        let tmp = tempfile::tempdir().unwrap();
        seed_dir(tmp.path()).unwrap();

        assert!(!tmp.path().join("tasks/compaction_system.j2").exists());
        // But the overridable tasks/title_system.md must be present.
        assert!(tmp.path().join("tasks/title_system.md").is_file());
    }

    #[test]
    fn embedded_table_covers_every_template_file() {
        // Regression guard: if someone adds a template under prompts/ but forgets
        // to register it in EMBEDDED, it loses both hot-reload override and include
        // resolution.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ai/agent_providers/prompts");
        let mut on_disk = Vec::new();
        for sub in ["partials", "commands", "system", "active_ai"] {
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
            "prompts/ .j2 files and the EMBEDDED name table disagree"
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
        // Direct form
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
            // Fallback
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
        // OpenRouter form `provider/model`; substring matching still hits the correct family
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
        // Stable fields remain in the system prompt.
        assert!(out.contains("Shell: bash 5.1"), "{out}");
        assert!(out.contains("linux (Ubuntu 22.04)"), "{out}");
        // The home field was cut to match opencode, no longer rendered
        assert!(!out.contains("Home directory:"), "{out}");
        // cwd changes with `cd`, so it was moved to the <environment_context> block at the end of the message --
        // any changing field in the system prompt (message[0]) would invalidate the whole cached section.
        assert!(!out.contains("Working directory:"), "{out}");
        assert!(!out.contains("/home/user/project"), "{out}");
    }

    /// Regression: the system prompt must be **byte-for-byte unchanged** across cwd changes.
    ///
    /// This assertion is the direct counterexample to the original bug: when cwd lived in <env>, one `cd` made
    /// message[0] change, FLM's item-by-item compare mismatched on the first item -> matched=0 -> the whole turn re-prefills.
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
        // Any model id renders a non-empty string (containing Zap's self-identifier).
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
                out.contains("Phosphor"),
                "id={id} should mention Phosphor, got: {out}"
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
        // With no skills, the skills block should not appear
        assert!(
            !out.contains("Skills provide specialized instructions"),
            "{out}"
        );
    }

    /// Issue #169 regression: the skill block in the system prompt must include skill_path (absolute path),
    /// not just name/description, otherwise the model can't call the read_skill tool correctly.
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

    /// Issue #169 follow-up: the BundledSkillId variant of a bundled skill can't be loaded via
    /// read_skill on the BYOP path (it goes through InvokeSkill), so the system prompt should not emit <skill_path>,
    /// to avoid the model using the @warp-skill:{id} value that is bound to fail.
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
        // render_system never panics; on failure it goes through fallback_system
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
        // The passed-in tool names must appear in the system prompt (dynamic whitelist)
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
        // The old blacklist wording should no longer appear
        assert!(
            !out.contains("Do not call unavailable tools"),
            "the blacklist section was removed: {out}"
        );
    }

    // -- Per-model-slot system prompt override (PromptSource) ----------------

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
        // claude-* automatically hits anthropic.j2.
        let model = LLMId::from("byop:p:claude-sonnet-4-5");
        let auto = render_system(AgentProviderApiType::OpenAi, &model, &[], &[], false, &[]);

        // An explicit Builtin("anthropic") is equivalent to the auto hit (same template, same model).
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

        // Forcing default.j2 must change the output -- proving the override really changed the template selection.
        let forced_default = render_system_with_override(
            AgentProviderApiType::OpenAi,
            &model,
            &[],
            &[],
            false,
            &[],
            Some(&PromptSource::Builtin("default".into())),
        );
        assert_ne!(auto, forced_default, "overriding to default should change the output");
    }

    #[test]
    fn unknown_builtin_override_falls_back_to_auto() {
        // A misspelled built-in name (system/does-not-exist.j2 doesn't exist) should not send a broken prompt,
        // but fall back to the auto hit by model family.
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
        // A custom prompt file can include the built-in partials (sharing the same env).
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
        assert!(out.starts_with("CUSTOM my-model"), "the custom body should take effect: {out}");
        // footer.j2's content should be included (sharing that partial with the built-in default render).
        let footer = env
            .get_template("partials/footer.j2")
            .unwrap()
            .render(Value::from_serialize(&ctx))
            .unwrap();
        assert!(
            !footer.trim().is_empty() && out.contains(footer.trim()),
            "the footer partial should be included: out={out}"
        );
    }

    #[test]
    fn custom_file_override_rejects_path_traversal() {
        let env = build_env();
        let ctx = PromptContext::default();
        let dir = Path::new("/tmp/prompts");
        assert!(
            render_custom_file_from(&env, dir, "../etc/passwd", &ctx).is_err(),
            "a .. path must be rejected"
        );
        assert!(
            render_custom_file_from(&env, dir, "/etc/passwd", &ctx).is_err(),
            "an absolute path must be rejected"
        );
        assert!(
            render_custom_file_from(&env, dir, "sub/../../escape.j2", &ctx).is_err(),
            "a multi-segment path smuggling in .. must be rejected"
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

    #[test]
    #[cfg(unix)]
    fn custom_file_override_rejects_symlink_escape() {
        // The `..`/absolute check in render_custom_file_value only inspects the *requested*
        // relative path string; a symlink living inside the allowed dir but pointing outside
        // it bypasses that check entirely (the path string itself is innocuous, e.g.
        // "system/local.j2"). Regression test for the canonicalize + starts_with containment
        // check added alongside it.
        let prompt_dir = tempfile::tempdir().unwrap();
        let secret_dir = tempfile::tempdir().unwrap();
        let secret_path = secret_dir.path().join("secret.txt");
        std::fs::write(&secret_path, "TOP SECRET, should never render").unwrap();

        let link_path = prompt_dir.path().join("escape.j2");
        std::os::unix::fs::symlink(&secret_path, &link_path).unwrap();

        let env = build_env();
        let ctx = PromptContext::default();
        let result = render_custom_file_from(&env, prompt_dir.path(), "escape.j2", &ctx);
        assert!(
            result.is_err(),
            "a symlink resolving outside the prompt dir must be refused, got: {result:?}"
        );
    }

    #[test]
    #[cfg(unix)]
    fn custom_file_override_allows_symlink_within_the_prompt_dir() {
        // A symlink is fine as long as it still resolves inside the allowed dir (e.g. a user
        // aliasing one custom prompt to another) -- the containment check must not be
        // overzealous and break that.
        let prompt_dir = tempfile::tempdir().unwrap();
        let real_path = prompt_dir.path().join("real.j2");
        std::fs::write(&real_path, "REAL {{ model_id }}").unwrap();
        let link_path = prompt_dir.path().join("alias.j2");
        std::os::unix::fs::symlink(&real_path, &link_path).unwrap();

        let env = build_env();
        let ctx = PromptContext {
            model_id: "aliased-model".into(),
            ..Default::default()
        };
        let out = render_custom_file_from(&env, prompt_dir.path(), "alias.j2", &ctx).unwrap();
        assert_eq!(out, "REAL aliased-model");
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
        // tool_names empty (shouldn't happen in theory; fallback: don't render the whitelist section)
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
            "plan_mode=false should not contain the Plan Mode section: {out}"
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
                "id={id} plan_mode=true should contain the Plan Mode section: {out}"
            );
            assert!(
                out.contains("Stop and wait"),
                "id={id} plan_mode=true should contain the Stop and wait guidance: {out}"
            );
        }
    }

    // Issue #116: global Rules (created by the user in Settings -> Agents -> Rules) must be injected into the system prompt.
    // The three cases below cover the key branches of `partials/user_rules.j2`.

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
            "when user_rules is empty the user rules block should not render: {out}"
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
            "the user rules block should render: {out}"
        );
        assert!(out.contains("## My rule"), "should contain the rule name: {out}");
        assert!(
            out.contains("Always use snake_case in Rust."),
            "should contain the rule content: {out}"
        );
    }

    #[test]
    fn render_includes_user_rules_across_all_template_families() {
        // user_rules.j2 is injected via footer.j2, and every system template family references footer.
        // This regression case ensures anthropic / beast / codex / gemini / kimi / trinity /
        // default -- any template family renders user rules, and none miss injection because a family didn't pull in footer.
        let rules = vec![(Some("family override".to_string()), "snake_case only.".to_string())];
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
                "id={id} should contain the user rule content: {out}"
            );
        }
    }

    #[test]
    fn render_user_rules_separates_multiple_rules_with_blank_line() {
        // Multiple rules should be separated by a blank line (`{% if not loop.last %}`), with no blank line after the last one.
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

        // Between two rules there should be at least one "blank line" (two adjacent newlines).
        // We don't hardcode the exact newline count, because minijinja's default trim_blocks/lstrip_blocks behavior
        // makes the exact count prone to change with template tweaks (a reviewer observed a 3-newline form in practice).
        // The contract we want is "a visual blank line + correct ordering".
        let pos_r1 = out.find("first content").expect("R1 content not found");
        let pos_r2 = out.find("## R2").expect("R2 heading not found");
        let pos_r3 = out.find("## R3").expect("R3 heading not found");
        assert!(pos_r1 < pos_r2 && pos_r2 < pos_r3, "ordering should be preserved: {out}");
        let between_r1_r2 = &out[pos_r1 + "first content".len()..pos_r2];
        let between_r2_r3 = &out[pos_r2..pos_r3];
        assert!(
            between_r1_r2.contains("\n\n"),
            "there should be a blank line between R1 and R2, actual: {between_r1_r2:?}"
        );
        assert!(
            between_r2_r3.contains("\n\n"),
            "there should be a blank line between R2 and R3, actual: {between_r2_r3:?}"
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
        // With no name, an empty `## ` heading line should not render
        assert!(
            !out.contains("## \n"),
            "with no name, an empty '## ' heading should not render: {out}"
        );
    }

    #[test]
    fn render_includes_thinking_language_across_all_template_families() {
        // thinking_language.j2 is injected via footer.j2, and every system template family references footer.
        // This regression case ensures all 8 template families render thinking_language, and none miss injection because a family didn't pull in footer,
        // which would make the LLM still think in English when a user asks in Chinese.
        // The 8 families are: anthropic / gpt / beast / codex / gemini / kimi / trinity / default
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
                "id={id} should render the thinking_language block: {out}"
            );
            assert!(
                out.contains("internal reasoning"),
                "id={id} should contain the thinking_language anchor: {out}"
            );
        }
    }

    #[test]
    fn render_thinking_language_precedes_tool_aliases() {
        // The meta-rule should come before the tool list and not be overridden by user_rules / project_rules.
        // A non-empty tool list must be passed, otherwise the whole tool_aliases.j2 block is skipped by {% if available_tools %}.
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
            .expect("should contain thinking_language");
        let pos_tools = out.find("# Available Tools").expect("should contain tool_aliases");
        assert!(
            pos_thinking < pos_tools,
            "thinking_language should come before tool_aliases: thinking={pos_thinking}, tools={pos_tools}\n{out}"
        );
    }
}
