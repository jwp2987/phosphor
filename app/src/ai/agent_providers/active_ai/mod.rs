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
    OneshotConfig, OneshotOptions, WhileKeysLoading, byop_oneshot_completion,
    resolve_active_ai_oneshot, resolve_active_ai_oneshot_with, resolve_next_command_oneshot,
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
// The ssh gate shared by nld_predict (#1982), prompt_suggestions and next_command (#2094)
// ---------------------------------------------------------------------------

/// Whether an active-AI request may be built for a session (#1982, #2094). Each of these
/// requests carries what the user typed or ran (the input buffer; a finished command and a
/// summary of its output; recent commands) to the model provider without the user asking, so in
/// an ssh session it is built only when the user allowed that for the feature.
/// `in_ssh_session` is `None` when the session is not known; that is treated like an ssh
/// session, so not knowing never sends.
pub(crate) fn allowed_for_session(in_ssh_session: Option<bool>, allowed_in_ssh: bool) -> bool {
    allowed_in_ssh || in_ssh_session == Some(false)
}

/// Whether `session` runs on another host over ssh: one the ssh wrapper set up, or one
/// bootstrapped in a plain `ssh` the user ran.
pub(crate) fn is_ssh_session(session: &crate::terminal::model::session::Session) -> bool {
    !session.is_local() || session.is_legacy_ssh_session()
}

/// Whether the active session of the pane `terminal_view_id` is an ssh session; `None` when
/// there is no such pane or it has no active session.
fn pane_is_in_ssh_session(
    app: &warpui::AppContext,
    terminal_view_id: warpui::EntityId,
) -> Option<bool> {
    let terminal_view = app.window_ids().find_map(|window_id| {
        app.view_with_id::<crate::terminal::TerminalView>(window_id, terminal_view_id)
    })?;
    let session = terminal_view
        .as_ref(app)
        .active_session()
        .as_ref(app)
        .session(app)?;
    Some(is_ssh_session(&session))
}

/// [`allowed_for_session`] for the pane a request is for. The pane is looked up only when the
/// answer depends on it.
fn allowed_for_pane(
    app: &warpui::AppContext,
    terminal_view_id: Option<warpui::EntityId>,
    allowed_in_ssh: bool,
) -> bool {
    let in_ssh_session = if allowed_in_ssh {
        None
    } else {
        terminal_view_id.and_then(|id| pane_is_in_ssh_session(app, id))
    };
    allowed_for_session(in_ssh_session, allowed_in_ssh)
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
        // #2094: the request carries the finished command and a summary of its output. In an
        // ssh session (or an unknown one) it is built only when the user turned that on.
        let allowed_in_ssh =
            crate::settings::AISettings::as_ref(app).is_prompt_suggestions_in_ssh_enabled();
        if !allowed_for_pane(app, terminal_view_id, allowed_in_ssh) {
            log::debug!(
                "[active_ai] prompt_suggestions not requested: ssh session (or unknown) and \
                 prompt_suggestions_in_ssh_enabled is off"
            );
            return None;
        }
        let cfg = resolve_active_ai_oneshot_with(app, terminal_view_id, WhileKeysLoading::Skip)?;
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
    use warpui::{AppContext, EntityId, SingletonEntity as _};

    use crate::settings::AISettings;

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
        let allowed_in_ssh =
            AISettings::as_ref(app).is_natural_language_autosuggestions_in_ssh_enabled();
        if !allowed_for_pane(app, terminal_view_id, allowed_in_ssh) {
            log::debug!(
                "[active_ai] nld_predict not requested: ssh session (or unknown) and \
                 natural_language_autosuggestions_in_ssh_enabled is off"
            );
            return None;
        }
        let cfg = resolve_active_ai_oneshot_with(app, terminal_view_id, WhileKeysLoading::Skip)?;
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

    /// The query-independent half of a relevant-files request, resolved while an
    /// `&AppContext` is still available (in `RequestParams::new`): the BYOP oneshot
    /// config + the rendered system prompt + options. Carried across the spawn
    /// boundary to the (AppContext-less) `chat_stream` tool interceptor, which pairs
    /// it with the model's query via [`run_with_context`].
    #[derive(Clone)]
    pub struct PreparedContext {
        pub cfg: OneshotConfig,
        pub system: String,
        pub opts: OneshotOptions,
    }

    // Manual `Debug`: never print `cfg` — it holds the provider `api_key`. This type
    // rides on `RequestParams` (which derives `Debug`), so a derived impl would risk
    // leaking the key into any `{:?}` log line.
    impl std::fmt::Debug for PreparedContext {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("PreparedContext")
                .field("model_id", &self.cfg.model_id)
                .field("system_len", &self.system.len())
                .finish_non_exhaustive()
        }
    }

    /// Phase 1 (pre-spawn, needs `&AppContext`): resolve the BYOP oneshot config and
    /// render the query-independent system prompt. Returns `None` when no BYOP
    /// active-AI oneshot is configured (the caller then has nothing to run).
    pub fn prepare_context(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
    ) -> Option<PreparedContext> {
        let cfg = resolve_active_ai_oneshot(app, terminal_view_id)?;
        let overrides = active_prompt_overrides(app, terminal_view_id);
        let system = render_with_override(
            "relevant_files_system.j2",
            overrides.relevant_files.as_ref(),
            context! {},
        );
        Some(PreparedContext {
            cfg,
            system,
            opts: OneshotOptions {
                response_format_json: true,
                max_chars: Some(12000),
                ..Default::default()
            },
        })
    }

    /// Longest the one-shot filter may take before the tool reports a timeout. The call runs
    /// inline in the agent's stream, so a stalled provider would otherwise hold the turn
    /// until the user pressed Stop (#1276).
    pub const ONESHOT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

    /// What the one-shot filter returned, plus how much of the candidate list the model saw.
    #[derive(Debug, Clone, PartialEq)]
    pub struct FilterRun {
        pub files: Vec<String>,
        /// Candidate files that fit in the prompt before the `max_chars` cut.
        pub shown: usize,
        pub total: usize,
    }

    /// Awaits `fut`, giving up after `deadline` (`None` on timeout).
    pub async fn with_deadline<T>(
        fut: impl std::future::Future<Output = T>,
        deadline: std::time::Duration,
    ) -> Option<T> {
        use futures::future::{Either, select};
        let fut = std::pin::pin!(fut);
        let timer = std::pin::pin!(warpui::r#async::Timer::after(deadline));
        match select(fut, timer).await {
            Either::Left((value, _)) => Some(value),
            Either::Right(_) => None,
        }
    }

    /// How many file lines of the rendered user prompt lie entirely within the first
    /// `max_chars` characters (what `oneshot` keeps), and how many there are in all. The
    /// file list is the `- ` lines after the `# Files` header.
    pub fn files_within_budget(user: &str, max_chars: usize) -> (usize, usize) {
        let mut consumed = 0usize;
        let (mut shown, mut total) = (0usize, 0usize);
        let mut in_files = false;
        for line in user.split_inclusive('\n') {
            consumed += line.chars().count();
            let trimmed = line.trim_end_matches('\n');
            if trimmed == "# Files" {
                in_files = true;
            } else if in_files && trimmed.starts_with("- ") {
                total += 1;
                // The trailing newline is not needed for the line to be intact.
                if consumed - (line.len() - trimmed.len()) <= max_chars {
                    shown += 1;
                }
            }
        }
        (shown, total)
    }

    /// Phase 2 (in the spawn / tool interceptor, no `AppContext`): render the user
    /// prompt from `query` + `files`, run the BYOP oneshot, and parse the response
    /// into the subset of `files` the model judged relevant. A failed or timed-out
    /// call is an `Err` carrying a message for the agent -- never an empty list, which
    /// would read as "no relevant files" (#1276).
    pub async fn run_with_context(
        prepared: &PreparedContext,
        query: &str,
        files: &[FileEntry],
    ) -> Result<FilterRun, String> {
        let input_paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
        let user = render(
            "relevant_files_user.j2",
            context! {
                query => query,
                files => files,
            },
        );
        let (shown, total) =
            files_within_budget(&user, prepared.opts.max_chars.unwrap_or(usize::MAX));
        let raw = match with_deadline(
            byop_oneshot_completion(&prepared.cfg, &prepared.system, &user, &prepared.opts),
            ONESHOT_TIMEOUT,
        )
        .await
        {
            Some(Ok(s)) => s,
            Some(Err(e)) => {
                log::debug!("[active_ai] relevant_files oneshot failed: {e:#}");
                return Err(format!(
                    "the outline filter's model call failed ({}); this is not an empty result",
                    e.root_cause()
                ));
            }
            None => {
                log::warn!("[active_ai] relevant_files oneshot timed out");
                return Err(format!(
                    "the outline filter's model call timed out after {}s; this is not an empty result",
                    ONESHOT_TIMEOUT.as_secs()
                ));
            }
        };
        Ok(FilterRun {
            files: parsing::parse_relevant_files(&raw, &input_paths),
            shown,
            total,
        })
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

    /// [`resolve`] for a suggestion in a known session (#2094). The request carries recent
    /// commands, similar history and the typed prefix; in an ssh session (`in_ssh_session`
    /// `Some(true)`, or `None` = not known) it is not made, so `None` comes back, unless
    /// `intelligent_autosuggestions_in_ssh_enabled` is on. The suggestions that need no request
    /// (history, completions) are the caller's and are not affected.
    pub fn resolve_for_session(
        app: &AppContext,
        terminal_view_id: Option<EntityId>,
        in_ssh_session: Option<bool>,
    ) -> Option<OneshotConfig> {
        use warpui::SingletonEntity as _;
        let allowed_in_ssh = crate::settings::AISettings::as_ref(app)
            .is_intelligent_autosuggestions_in_ssh_enabled();
        if !allowed_for_session(in_ssh_session, allowed_in_ssh) {
            log::debug!(
                "[active_ai] next_command not requested: ssh session (or unknown) and \
                 intelligent_autosuggestions_in_ssh_enabled is off"
            );
            return None;
        }
        resolve(app, terminal_view_id)
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

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
