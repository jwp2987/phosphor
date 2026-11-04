//! The agent send awaits the project-rules scan instead of scanning on the UI thread (#1364).
//!
//! `BlocklistAIContextModel::pending_context` builds a request's context synchronously, so it
//! cannot wait. When the rules for the working directory are not known yet (the cd-time
//! background scan, `ProjectContextModel::prewarm_fast_path`, has not landed) it leaves them
//! out. [`begin`] runs when the response stream is created: if the rules are known by then they
//! are added at once; otherwise it registers for the scan's completion and the stream's async
//! block awaits it ([`wait_for_project_rules`]) before the request goes out, the same way the
//! send awaits the keyring load (#1368). The only timer is a logged give-up for a scan that
//! never reports; on give-up (or a dropped sender) the request goes out without the project
//! rules and a warning is logged, rather than blocking the send.

use std::time::Duration;

use futures::channel::oneshot;

use crate::ai::agent::{AIAgentContext, AIAgentInput, api::RequestParams};

/// Give-up for a scan that never reports. The scan itself is structurally bounded (a handful of
/// stats), so this only guards a stuck filesystem.
const PROJECT_RULES_GIVE_UP: Duration = Duration::from_secs(10);

pub(super) type RulesReceiver =
    oneshot::Receiver<Option<ai::project_context::model::ProjectRulesResult>>;

pub(super) enum RulesWaitOutcome {
    /// The scan landed: add these rules (if any) to the request.
    Ready(Option<ai::project_context::model::ProjectRulesResult>),
    Cancelled,
    /// No answer: send without the project rules.
    GaveUp,
}

/// Whether `params` carries a request context built by `pending_context` (it always includes a
/// `Directory` entry).
fn has_request_context(params: &RequestParams) -> bool {
    params.input.iter().any(|input| {
        input.context().is_some_and(|context| {
            context
                .iter()
                .any(|entry| matches!(entry, AIAgentContext::Directory { .. }))
        })
    })
}

/// Adds the project rules for the request's working directory if they are known now (patching
/// `params` in place), or returns a receiver for the scan that will produce them. `None` when no
/// local scan applies (remote session, no cwd, no `ProjectContextModel`) or nothing is needed.
#[cfg(feature = "local_fs")]
pub(super) fn begin<T: warpui::Entity>(
    params: &mut RequestParams,
    ctx: &mut warpui::ModelContext<T>,
) -> Option<RulesReceiver> {
    use ai::project_context::model::{ProjectContextModel, RulesWait};
    use warpui::SingletonEntity as _;

    if params.session_context.is_remote() || !has_request_context(params) {
        return None;
    }
    let cwd = params.session_context.current_working_directory().clone()?;
    if !ctx.has_singleton_model::<ProjectContextModel>() {
        return None;
    }
    let wait = ProjectContextModel::handle(ctx).update(ctx, |model, model_ctx| {
        model.request_rules_wait(std::path::Path::new(&cwd), model_ctx)
    });
    match wait {
        RulesWait::Ready(rules) => {
            apply_project_rules(params, rules);
            None
        }
        RulesWait::Pending(receiver) => Some(receiver),
    }
}

#[cfg(not(feature = "local_fs"))]
pub(super) fn begin<T: warpui::Entity>(
    _params: &mut RequestParams,
    _ctx: &mut warpui::ModelContext<T>,
) -> Option<RulesReceiver> {
    None
}

/// Awaits the scan, racing cancellation and a logged give-up.
pub(super) async fn wait_for_project_rules(
    receiver: RulesReceiver,
    cancellation_rx: &mut oneshot::Receiver<()>,
) -> RulesWaitOutcome {
    use futures::future::{Either, select};

    let timer = warpui::r#async::Timer::after(PROJECT_RULES_GIVE_UP);
    futures::pin_mut!(timer);
    let landed = select(receiver, timer);
    match select(cancellation_rx, landed).await {
        Either::Left(_) => RulesWaitOutcome::Cancelled,
        Either::Right((Either::Left((Ok(rules), _)), _)) => RulesWaitOutcome::Ready(rules),
        Either::Right((Either::Left(_), _)) => {
            log::warn!(
                "[project-rules] the rules scan went away without an answer; \
                 sending without project rules"
            );
            RulesWaitOutcome::GaveUp
        }
        Either::Right((Either::Right(_), _)) => {
            log::warn!(
                "[project-rules] gave up after {PROJECT_RULES_GIVE_UP:?} waiting for the rules \
                 scan; sending without project rules"
            );
            RulesWaitOutcome::GaveUp
        }
    }
}

/// Replaces the project-rules entry of every request context with `rules` (a no-op for `None`
/// or when the entry already says the same).
pub(super) fn apply_project_rules(
    params: &mut RequestParams,
    rules: Option<ai::project_context::model::ProjectRulesResult>,
) {
    let Some(rules) = rules else {
        return;
    };
    let entry = crate::ai::blocklist::context_model::project_rules_context(rules);
    for input in &mut params.input {
        patch_input(input, &entry);
    }
}

fn patch_input(input: &mut AIAgentInput, entry: &AIAgentContext) {
    let Some(context) = input.context_mut() else {
        return;
    };
    // Only requests built by `pending_context` (they carry the Directory entry).
    if !context
        .iter()
        .any(|existing| matches!(existing, AIAgentContext::Directory { .. }))
    {
        return;
    }
    if context.iter().any(|existing| existing == entry) {
        return;
    }
    let mut patched: Vec<AIAgentContext> = context
        .iter()
        .filter(|existing| !matches!(existing, AIAgentContext::ProjectRules { .. }))
        .cloned()
        .collect();
    patched.push(entry.clone());
    *context = patched.into();
}

#[cfg(test)]
mod tests {
    use super::*;
    use ai::project_context::model::{ProjectRule, ProjectRulesResult};
    use std::path::PathBuf;

    fn directory() -> AIAgentContext {
        AIAgentContext::Directory {
            pwd: Some("/repo".to_owned()),
            home_dir: None,
            are_file_symbols_indexed: false,
        }
    }

    fn rules(content: &str) -> ProjectRulesResult {
        ProjectRulesResult {
            root_path: PathBuf::from("/repo"),
            active_rules: vec![ProjectRule {
                path: PathBuf::from("/repo/AGENTS.md"),
                content: content.to_owned(),
            }],
            additional_rule_paths: Vec::new(),
        }
    }

    fn project_rules_of(input: &mut AIAgentInput) -> Vec<AIAgentContext> {
        input
            .context_mut()
            .unwrap()
            .iter()
            .filter(|c| matches!(c, AIAgentContext::ProjectRules { .. }))
            .cloned()
            .collect()
    }

    /// A request built before the scan landed gets the rules once it has: added, once, and a
    /// stale global-only entry is replaced rather than duplicated.
    #[test]
    fn late_rules_are_added_to_request_contexts_once() {
        let mut input = AIAgentInput::ResumeConversation {
            context: vec![directory()].into(),
        };
        let entry = crate::ai::blocklist::context_model::project_rules_context(rules("late"));
        patch_input(&mut input, &entry);
        assert_eq!(project_rules_of(&mut input), vec![entry.clone()]);
        // Idempotent.
        patch_input(&mut input, &entry);
        assert_eq!(project_rules_of(&mut input), vec![entry.clone()]);
        // A different earlier entry (global-only) is replaced.
        let newer = crate::ai::blocklist::context_model::project_rules_context(rules("newer"));
        patch_input(&mut input, &newer);
        assert_eq!(project_rules_of(&mut input), vec![newer]);
    }

    /// Inputs that were not built by `pending_context` (no Directory entry) are left alone.
    #[test]
    fn contexts_without_a_directory_entry_are_untouched() {
        let mut input = AIAgentInput::ResumeConversation {
            context: Vec::<AIAgentContext>::new().into(),
        };
        let entry = crate::ai::blocklist::context_model::project_rules_context(rules("x"));
        patch_input(&mut input, &entry);
        assert!(project_rules_of(&mut input).is_empty());
    }
}
