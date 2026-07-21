//! 主动式 AI(active AI)子链路的 BYOP 适配。
//!
//! 涵盖三类:
//! - `prompt_suggestions`:命令完成后给出"问问 Agent"建议(Simple/Coding)
//! - `nld_predict`:Agent 输入框打字时实时补全
//! - `relevant_files`:从给定文件列表中筛选与 query 相关的子集
//!
//! 共同模式:
//! 1. 调用方在 spawn 之前(还有 `&AppContext`)调 `dispatch::*` 系列 helper,
//!    解出 `OneshotConfig` + 渲染好的 system/user prompt → `RenderedRequest`
//! 2. spawn 闭包内调 `run_*(req)` 发请求 + 解析,返回各子链路对应的 response 类型
//! 3. UI 回调里直接消费返回的 response,与原 `ServerApi` 路径完全等价
//!
//! 没有 BYOP 配置(`active_ai_model` 解码失败)→ `dispatch::*` 返回 `None`,
//! 调用方静默 no-op(Zap 已剥云,不再 fallback ServerApi)。

use minijinja::context;
use serde::Serialize;

use super::oneshot::{
    byop_oneshot_completion, resolve_active_ai_oneshot, resolve_next_command_oneshot,
    OneshotConfig, OneshotOptions,
};
use crate::ai::predict::generate_am_query_suggestions::GenerateAMQuerySuggestionsResponse;

pub mod parsing;

// ---------------------------------------------------------------------------
// 模板
// ---------------------------------------------------------------------------

// The active-ai templates now live in `prompt_renderer`'s EMBEDDED table (names
// prefixed with `active_ai/`) and share that hot-reload env — dropping an
// `active_ai/<name>.j2` into the Prompt template directory overrides the built-in,
// live, no rebuild. All that remains here is a thin prefix-adding wrapper.

/// Render an active-ai template (`template` without the `active_ai/` prefix, e.g.
/// `nld_predict_system.j2`) through [`prompt_renderer`]'s hot-reload env.
fn render(template: &str, ctx: minijinja::Value) -> String {
    super::prompt_renderer::render_template(&format!("active_ai/{template}"), ctx)
}

/// Like [`render`], but additionally applies the profile's per-prompt override for
/// this prompt (only `CustomFile` takes effect; `None` / `Builtin` fall back to the
/// built-in `template`).
fn render_with_override(
    template: &str,
    prompt_override: Option<&crate::ai::execution_profiles::PromptSource>,
    ctx: minijinja::Value,
) -> String {
    super::prompt_renderer::render_template_with_override(
        &format!("active_ai/{template}"),
        prompt_override,
        ctx,
    )
}

/// Fetch the active profile's per-prompt override table (the picker results for the
/// active-ai sub-prompts). Each dispatch reads it once pre-spawn (while it still
/// holds `&AppContext`) and passes the relevant slot to [`render_with_override`].
fn active_prompt_overrides(
    app: &warpui::AppContext,
    terminal_view_id: Option<warpui::EntityId>,
) -> crate::ai::execution_profiles::ProfilePromptOverrides {
    use warpui::SingletonEntity;
    crate::ai::execution_profiles::profiles::AIExecutionProfilesModel::as_ref(app)
        .active_profile(terminal_view_id, app)
        .data()
        .prompt_overrides
        .clone()
}

// ---------------------------------------------------------------------------
// 公共上下文片段
// ---------------------------------------------------------------------------

/// 单条已完成命令块的精简上下文(供 prompt_suggestions / nld_predict 消费)。
#[derive(Debug, Clone, Serialize, Default)]
pub struct BlockSnippet {
    pub command: String,
    pub output_summary: String,
    pub exit_code: i32,
    pub pwd: String,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct LastBlockSnippet {
    pub command: String,
    pub exit_code: i32,
    pub pwd: String,
}

/// 已渲染好 prompt + 解析好 OneshotConfig 的请求 — 跨 spawn 边界传递。
pub struct RenderedRequest {
    pub cfg: OneshotConfig,
    pub system: String,
    pub user: String,
    pub opts: OneshotOptions,
}

// ---------------------------------------------------------------------------
// prompt_suggestions
// ---------------------------------------------------------------------------

pub mod prompt_suggestions {
    use super::*;
    use crate::settings::language::LanguageSettings;
    use warpui::{AppContext, EntityId, SingletonEntity};

    pub struct Input {
        pub recent_blocks: Vec<BlockSnippet>,
        pub system_context: Option<String>,
        pub last_exit_code: i32,
    }

    /// Spawn 前调用:解 BYOP 配置 + 渲染 prompt。`None` ⇒ 静默 no-op。
    pub fn dispatch(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
        input: Input,
    ) -> Option<RenderedRequest> {
        let cfg = resolve_active_ai_oneshot(app, terminal_view_id)?;
        let language = (*LanguageSettings::as_ref(app).language).prompt_language_name();
        let overrides = active_prompt_overrides(app, terminal_view_id);
        let system = render_with_override(
            "prompt_suggestions_system.j2",
            overrides.prompt_suggestions.as_ref(),
            context! { language => language },
        );
        let user = render(
            "prompt_suggestions_user.j2",
            context! {
                recent_blocks => input.recent_blocks,
                system_context => input.system_context,
                last_exit_code => input.last_exit_code,
            },
        );
        Some(RenderedRequest {
            cfg,
            system,
            user,
            opts: OneshotOptions {
                response_format_json: true,
                max_chars: Some(6000),
                ..Default::default()
            },
        })
    }

    /// Spawn 内执行:发请求 + 解析。失败 → `None`(调用方映射为 Error)。
    pub async fn run(req: RenderedRequest) -> Option<GenerateAMQuerySuggestionsResponse> {
        let raw = match byop_oneshot_completion(&req.cfg, &req.system, &req.user, &req.opts).await {
            Ok(s) => s,
            Err(e) => {
                log::debug!("[active_ai] prompt_suggestions oneshot failed: {e:#}");
                return None;
            }
        };
        log::debug!(
            "[active_ai] prompt_suggestions raw response ({} chars): {raw}",
            raw.len()
        );
        parsing::parse_suggestion(&raw)
    }
}

// ---------------------------------------------------------------------------
// nld_predict
// ---------------------------------------------------------------------------

pub mod nld_predict {
    use super::*;
    use warpui::{AppContext, EntityId};

    pub struct Input {
        pub partial_query: String,
        pub last_block: Option<LastBlockSnippet>,
        pub system_context: Option<String>,
    }

    pub fn dispatch(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
        input: Input,
    ) -> Option<RenderedRequest> {
        let cfg = resolve_active_ai_oneshot(app, terminal_view_id)?;
        let overrides = active_prompt_overrides(app, terminal_view_id);
        let system = render_with_override(
            "nld_predict_system.j2",
            overrides.nld_predict.as_ref(),
            context! {},
        );
        let user = render(
            "nld_predict_user.j2",
            context! {
                partial_query => input.partial_query,
                last_block => input.last_block,
                system_context => input.system_context,
            },
        );
        Some(RenderedRequest {
            cfg,
            system,
            user,
            opts: OneshotOptions {
                response_format_json: false,
                max_chars: Some(4000),
                ..Default::default()
            },
        })
    }

    pub async fn run(req: RenderedRequest) -> Option<String> {
        let raw = match byop_oneshot_completion(&req.cfg, &req.system, &req.user, &req.opts).await {
            Ok(s) => s,
            Err(e) => {
                log::debug!("[active_ai] nld_predict oneshot failed: {e:#}");
                return None;
            }
        };
        parsing::sanitize_predict(&raw)
    }
}

// ---------------------------------------------------------------------------
// relevant_files
// ---------------------------------------------------------------------------

pub mod relevant_files {
    use super::*;
    use warpui::{AppContext, EntityId};

    #[derive(Debug, Clone, Serialize)]
    pub struct FileEntry {
        pub path: String,
        pub symbols: String,
    }

    pub struct Input {
        pub query: String,
        pub files: Vec<FileEntry>,
    }

    pub struct Prepared {
        pub req: RenderedRequest,
        pub input_paths: Vec<String>,
    }

    pub fn dispatch(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
        input: Input,
    ) -> Option<Prepared> {
        let cfg = resolve_active_ai_oneshot(app, terminal_view_id)?;
        let input_paths: Vec<String> = input.files.iter().map(|f| f.path.clone()).collect();
        let overrides = active_prompt_overrides(app, terminal_view_id);
        let system = render_with_override(
            "relevant_files_system.j2",
            overrides.relevant_files.as_ref(),
            context! {},
        );
        let user = render(
            "relevant_files_user.j2",
            context! {
                query => input.query,
                files => input.files,
            },
        );
        Some(Prepared {
            req: RenderedRequest {
                cfg,
                system,
                user,
                opts: OneshotOptions {
                    response_format_json: true,
                    max_chars: Some(12000),
                    ..Default::default()
                },
            },
            input_paths,
        })
    }

    pub async fn run(prepared: Prepared) -> Vec<String> {
        let raw = match byop_oneshot_completion(
            &prepared.req.cfg,
            &prepared.req.system,
            &prepared.req.user,
            &prepared.req.opts,
        )
        .await
        {
            Ok(s) => s,
            Err(e) => {
                log::debug!("[active_ai] relevant_files oneshot failed: {e:#}");
                return Vec::new();
            }
        };
        parsing::parse_relevant_files(&raw, &prepared.input_paths)
    }
}

// ---------------------------------------------------------------------------
// workflow_metadata(Workflow Editor 的 Autofill 按钮:命令 → 参数化 metadata)
// ---------------------------------------------------------------------------

pub mod workflow_metadata {
    use super::*;
    use warpui::{AppContext, EntityId};

    pub use parsing::WorkflowMetadataDto;

    pub struct Input {
        pub command: String,
    }

    /// Spawn 前调用:解 BYOP 配置 + 渲染 prompt。`None` ⇒ 调用方提示用户配置 BYOP。
    pub fn dispatch(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
        input: Input,
    ) -> Option<RenderedRequest> {
        let cfg = resolve_active_ai_oneshot(app, terminal_view_id)?;
        let overrides = active_prompt_overrides(app, terminal_view_id);
        let system = render_with_override(
            "workflow_metadata_system.j2",
            overrides.workflow_metadata.as_ref(),
            context! {},
        );
        let user = render(
            "workflow_metadata_user.j2",
            context! {
                command => input.command,
            },
        );
        Some(RenderedRequest {
            cfg,
            system,
            user,
            opts: OneshotOptions {
                response_format_json: true,
                max_chars: Some(4000),
                ..Default::default()
            },
        })
    }

    /// Spawn 内执行:发请求 + 解析。失败 → `None`(调用方映射为 BadCommand)。
    pub async fn run(req: RenderedRequest) -> Option<WorkflowMetadataDto> {
        let raw = match byop_oneshot_completion(&req.cfg, &req.system, &req.user, &req.opts).await {
            Ok(s) => s,
            Err(e) => {
                log::debug!("[active_ai] workflow_metadata oneshot failed: {e:#}");
                return None;
            }
        };
        log::debug!(
            "[active_ai] workflow_metadata raw response ({} chars): {raw}",
            raw.len()
        );
        parsing::parse_workflow_metadata(&raw)
    }
}

// ---------------------------------------------------------------------------
// next_command(灰色补全 / zero-state 建议)
// ---------------------------------------------------------------------------

pub mod next_command {
    use super::*;
    use warpui::{AppContext, EntityId};

    #[derive(Debug, Serialize)]
    struct UserRuleCtx {
        name: Option<String>,
        content: String,
    }

    pub struct Input {
        pub recent_blocks: Vec<BlockSnippet>,
        /// 已在 client 端从历史 DB 选出的相似命令上下文(可选)。
        pub history_context: String,
        pub system_context: Option<String>,
        /// 用户已输入的前缀(必须用作输出前缀)。
        pub prefix: Option<String>,
        /// 之前已 reject 的建议(避免重复)。
        pub rejected_suggestions: Vec<String>,
        /// Snapshot of the global Rules configured in Settings → Agents → Rules.
        pub user_rules: Vec<(Option<String>, String)>,
        /// Per-prompt override for `next_command_system.j2`, resolved from the
        /// active profile before spawn (the render runs where `&AppContext` is
        /// gone). `None` = Auto (built-in / hot-reloaded template).
        pub prompt_override: Option<crate::ai::execution_profiles::PromptSource>,
    }

    /// Pre-spawn: resolve the BYOP config (needs `&AppContext`). `None` ⇒ silent no-op.
    pub fn resolve(app: &AppContext, terminal_view_id: Option<EntityId>) -> Option<OneshotConfig> {
        resolve_next_command_oneshot(app, terminal_view_id)
    }

    /// Pre-spawn: resolve this profile's override for the next-command system
    /// prompt, to be carried into [`Input`] (the render happens in-spawn where the
    /// `&AppContext` is no longer available).
    pub fn resolve_prompt_override(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Option<crate::ai::execution_profiles::PromptSource> {
        active_prompt_overrides(app, terminal_view_id)
            .next_command
            .clone()
    }

    /// In-spawn: render the prompt from cfg + Input and send the request.
    /// Template rendering does not depend on AppContext, so it can run in-spawn.
    pub async fn run_with(cfg: OneshotConfig, input: Input) -> Option<String> {
        let user_rule_ctxs: Vec<UserRuleCtx> = input
            .user_rules
            .into_iter()
            .map(|(name, content)| UserRuleCtx { name, content })
            .collect();
        let system = render_with_override(
            "next_command_system.j2",
            input.prompt_override.as_ref(),
            context! {
                user_rules => user_rule_ctxs,
            },
        );
        let user = render(
            "next_command_user.j2",
            context! {
                recent_blocks => input.recent_blocks,
                history_context => input.history_context,
                system_context => input.system_context,
                prefix => input.prefix,
                rejected_suggestions => input.rejected_suggestions,
            },
        );
        let opts = OneshotOptions {
            response_format_json: false,
            max_chars: Some(8000),
            ..Default::default()
        };
        let raw = match byop_oneshot_completion(&cfg, &system, &user, &opts).await {
            Ok(s) => s,
            Err(e) => {
                log::debug!("[active_ai] next_command oneshot failed: {e:#}");
                return None;
            }
        };
        log::info!(
            "[active_ai] next_command raw response ({} chars): {raw:?}",
            raw.len()
        );
        let sanitized = parsing::sanitize_predict(&raw);
        if sanitized.is_none() && !raw.trim().is_empty() {
            log::warn!("[active_ai] next_command sanitize REJECTED raw response");
        }
        sanitized
    }
}
