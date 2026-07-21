//! BYOP adaptation for the active-AI sub-flows.
//!
//! Covers three kinds:
//! - `prompt_suggestions`: after a command finishes, offer "ask the Agent"
//!   suggestions (Simple/Coding).
//! - `nld_predict`: real-time completion as you type in the Agent input box.
//! - `relevant_files`: filter a given file list down to the subset relevant to the query.
//!
//! Common pattern:
//! 1. Before spawn (while `&AppContext` is still available), the caller calls the
//!    `dispatch::*` helpers to resolve an `OneshotConfig` + rendered system/user
//!    prompt → `RenderedRequest`.
//! 2. Inside the spawn closure, `run_*(req)` sends the request + parses it, returning
//!    the response type for each sub-flow.
//! 3. The UI callback consumes the returned response directly — fully equivalent to
//!    the original `ServerApi` path.
//!
//! With no BYOP config (`active_ai_model` fails to decode), `dispatch::*` returns
//! `None` and the caller silently no-ops (Zap has stripped the cloud and no longer
//! falls back to ServerApi).

use minijinja::context;
use serde::Serialize;

use super::oneshot::{
    byop_oneshot_completion, resolve_active_ai_oneshot, resolve_next_command_oneshot,
    OneshotConfig, OneshotOptions,
};
use crate::ai::predict::generate_am_query_suggestions::GenerateAMQuerySuggestionsResponse;

pub mod parsing;

// ---------------------------------------------------------------------------
// Templates
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
// Shared context fragments
// ---------------------------------------------------------------------------

/// Slim context for a single finished command block (consumed by prompt_suggestions / nld_predict).
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

/// A request with the prompt already rendered + OneshotConfig already resolved — passed across the spawn boundary.
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

    /// Called before spawn: resolve BYOP config + render prompt. `None` ⇒ silent no-op.
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

    /// Runs inside the spawn: send request + parse. On failure → `None` (caller maps to Error).
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
// workflow_metadata (the Workflow Editor's Autofill button: command → parameterized metadata)
// ---------------------------------------------------------------------------

pub mod workflow_metadata {
    use super::*;
    use warpui::{AppContext, EntityId};

    pub use parsing::WorkflowMetadataDto;

    pub struct Input {
        pub command: String,
    }

    /// Called before spawn: resolve BYOP config + render prompt. `None` ⇒ caller prompts the user to configure BYOP.
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

    /// Runs inside the spawn: send request + parse. On failure → `None` (caller maps to BadCommand).
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
// next_command (grey autocompletion / zero-state suggestions)
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
        /// Similar-command context already selected client-side from the history DB (optional).
        pub history_context: String,
        pub system_context: Option<String>,
        /// The prefix the user has already typed (must be used as the output prefix).
        pub prefix: Option<String>,
        /// Previously rejected suggestions (to avoid repeats).
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
