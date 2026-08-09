use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};
#[cfg(feature = "local_fs")]
use crate::ai::agent::AIAgentActionResultType;
use crate::ai::skills::{SkillManager, SkillTelemetryEvent};
#[cfg(feature = "local_fs")]
use crate::ai::skills::extract_skill_parent_directory;
use crate::send_telemetry_from_ctx;
use ai::agent::action_result::AnyFileContent;
use ai::skills::SkillReference;
#[cfg(feature = "local_fs")]
use ai::skills::parse_skill;
use std::path::Path;
use warpui::{ModelContext, SingletonEntity};

use crate::ai::agent::AIAgentActionType;
use crate::ai::agent::ReadSkillRequest;
use crate::ai::agent::ReadSkillResult;
use ai::agent::action_result::FileContext;
use futures::future::{BoxFuture, FutureExt};
use warpui::Entity;

pub struct ReadSkillExecutor;

impl ReadSkillExecutor {
    pub fn new() -> Self {
        Self
    }

    pub(super) fn should_autoexecute(
        &self,
        _input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> bool {
        // User-created skills are readable on demand.
        true
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let ExecuteActionInput { action, .. } = input;
        let AIAgentActionType::ReadSkill(ReadSkillRequest { skill: skill_ref }) = &action.action
        else {
            return ActionExecution::InvalidAction;
        };

        let manager = SkillManager::as_ref(ctx);

        // Cache hit: the proto's `SkillReference::Path(p)` only matches here when p
        // is exactly the real SKILL.md absolute path in the index.
        //
        // Uses `active_skill_by_reference` (not `skill_by_reference`) so a
        // `BundledSkillId` reference is rejected once its activation condition is
        // no longer met (a `tui_only` skill read from the GUI, a feature-gated
        // skill whose flag flipped off, ...). Path-based user skills are
        // unaffected: they have no activation condition to check. See issue #370.
        if let Some(skill) = manager.active_skill_by_reference(skill_ref, ctx) {
            send_telemetry_from_ctx!(
                SkillTelemetryEvent::Read {
                    reference: skill_ref.clone(),
                    name: Some(skill.name.clone()),
                    scope: Some(skill.scope),
                    provider: Some(skill.provider),
                    error: false,
                },
                ctx
            );
            return success_execution(skill);
        }

        // The BYOP `read_skill` tool's argument is the skill **name**, which
        // `from_args` stuffs into the `SkillReference::SkillPath(name)` slot (to
        // avoid changing the proto schema). On cache miss, look up the real SKILL.md
        // path by name here, covering every skill the Skill manager can see (file
        // skills + bundled skills).
        if let SkillReference::Path(p) = skill_ref {
            if let Some(candidate_name) = p.to_local_path().and_then(name_candidate) {
                if let Some(skill) = manager.find_skill_by_name(candidate_name) {
                    send_telemetry_from_ctx!(
                        SkillTelemetryEvent::Read {
                            reference: skill_ref.clone(),
                            name: Some(skill.name.clone()),
                            scope: Some(skill.scope),
                            provider: Some(skill.provider),
                            error: false,
                        },
                        ctx
                    );
                    return success_execution(skill);
                }
            }
        }

        // Cache miss fallback: for a `SkillReference::Path`-shaped reference, if the
        // path shape looks like a valid skill file
        // (`.../<provider>/skills/<name>/SKILL.md` or under a warp-managed skill
        // directory), read and parse it straight from disk, fixing the "skill exists
        // but cache isn't warm" scenario described in issue #99.
        //
        // Design tradeoffs:
        // - Doesn't proactively warm the SkillManager cache. The cache is maintained
        //   one-way by SkillWatcher; writing to it here would break the data flow.
        //   Repeated read_skill calls on the same path will re-read from disk, but
        //   SKILL.md is usually tiny, so this is negligible.
        // - `extract_skill_parent_directory` only validates the path shape, at the
        //   same safety level as the path returned on cache hit — neither restricts
        //   to a home-directory prefix. This is intentional: in-project skills
        //   (`/some/repo/.agents/skills/...`) need to be readable too.
        // - On Windows the regex splits on backslashes, so Linux-style
        //   `/home/<u>/...` paths get rejected; this means the fallback doesn't work
        //   for a "Windows host process + WSL session" setup, a known limitation of
        //   issue #99 (see the PR description).
        // The cache-miss fallback is only available in builds with a local
        // filesystem; in fs-less builds like WASM, `extract_skill_parent_directory` /
        // `parse_skill` don't exist, so there's naturally no way to read from disk.
        #[cfg(feature = "local_fs")]
        if let SkillReference::Path(path) = skill_ref {
            // The cache-miss disk fallback only ever makes sense for a local path:
            // extract_skill_parent_directory/parse_skill are local-fs operations,
            // and nothing populates a remote skill reference here yet (issue #299
            // covers the type, not remote skill reads via this path).
            if let Some(local_path) = path.to_local_path()
                && extract_skill_parent_directory(local_path).is_ok()
            {
                let path = local_path.to_path_buf();
                let skill_ref_for_async = skill_ref.clone();
                return ActionExecution::new_async(
                    async move { parse_skill(&path) },
                    move |parsed, _app| match parsed {
                        Ok(skill) => AIAgentActionResultType::ReadSkill(
                            ReadSkillResult::Success {
                                content: FileContext::new(
                                    skill.path.display_path(),
                                    AnyFileContent::StringContent(skill.content.clone()),
                                    skill.line_range.clone(),
                                    None,
                                ),
                            },
                        ),
                        Err(err) => AIAgentActionResultType::ReadSkill(
                            ReadSkillResult::Error(format!(
                                "Skill not found: {skill_ref_for_async:?} ({err})"
                            )),
                        ),
                    },
                );
            }
        }

        send_telemetry_from_ctx!(
            SkillTelemetryEvent::Read {
                reference: skill_ref.clone(),
                name: None,
                scope: None,
                provider: None,
                error: true,
            },
            ctx
        );
        ActionExecution::Sync(
            ReadSkillResult::Error(format!("Skill not found: {:?}", skill_ref)).into(),
        )
    }

    pub(super) fn preprocess_action(
        &mut self,
        _input: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

/// Build a sync success execution from a parsed skill.
///
/// This helper is factored out so the generic `T` of `ActionExecution<T>` infers to
/// the same type on both the `success_execution` and `new_async` paths (otherwise
/// Rust would require the function to declare its return type explicitly).
fn success_execution(skill: &ai::skills::ParsedSkill) -> ActionExecution<anyhow::Result<ai::skills::ParsedSkill>> {
    let content = FileContext::new(
        skill.path.display_path(),
        AnyFileContent::StringContent(skill.content.clone()),
        skill.line_range.clone(),
        None,
    );
    ActionExecution::Sync(ReadSkillResult::Success { content }.into())
}

/// Determines whether the value inside `SkillReference::Path` should be treated as a
/// skill **name** lookup.
///
/// A real SKILL.md path contains a path separator (`/` or `\`) or is absolute, while
/// a BYOP tool call's name (e.g. `"build-feature"`) is a plain string. Distinguishing
/// the two avoids mistaking `/home/.../SKILL.md` for a name and missing the
/// filesystem fallback.
fn name_candidate(p: &Path) -> Option<&str> {
    if p.is_absolute() {
        return None;
    }
    let s = p.to_str()?;
    if s.is_empty() || s.contains('/') || s.contains('\\') {
        return None;
    }
    Some(s)
}

impl Entity for ReadSkillExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "read_skill_tests.rs"]
mod tests;
