use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};
#[cfg(feature = "local_fs")]
use crate::ai::agent::AIAgentActionResultType;
use crate::ai::blocklist::SessionContext;
#[cfg(feature = "local_fs")]
use crate::ai::skills::extract_skill_parent_directory;
use crate::ai::skills::{SkillManager, SkillTelemetryEvent};
use crate::send_telemetry_from_ctx;
use crate::terminal::model::session::active_session::ActiveSession;
use ai::agent::action_result::AnyFileContent;
use ai::skills::SkillReference;
#[cfg(feature = "local_fs")]
use ai::skills::parse_skill;
use std::path::Path;
#[cfg(feature = "local_fs")]
use std::path::{Component, PathBuf};
#[cfg(feature = "local_fs")]
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::{ModelContext, ModelHandle, SingletonEntity};

use crate::ai::agent::AIAgentActionType;
use crate::ai::agent::ReadSkillRequest;
use crate::ai::agent::ReadSkillResult;
use ai::agent::action_result::FileContext;
use futures::future::{BoxFuture, FutureExt};
use warpui::Entity;

pub struct ReadSkillExecutor {
    /// The session this executor's actions run against. Used to resolve which
    /// skill catalog (local, or a connected remote host's) a `BundledSkillId`
    /// reference should be read from — see `execute`'s use of
    /// `SessionContext::skill_path_origin`.
    active_session: ModelHandle<ActiveSession>,
}

impl ReadSkillExecutor {
    pub fn new(active_session: ModelHandle<ActiveSession>) -> Self {
        Self { active_session }
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

        // Resolve which skill catalog a `BundledSkillId` reference should be read
        // from: the local catalog for a local session, or the connected remote
        // host's catalog for a warpified-remote (SSH) session. Without this, a
        // remote session would silently read the client's own bundled-skill
        // catalog instead of the host's.
        let session_context = SessionContext::from_session(self.active_session.as_ref(ctx), ctx);
        let path_origin = session_context.skill_path_origin();
        // The session's working directory bounds which project skills the SkillManager
        // would ever surface, so it also bounds what the cache-miss disk fallback below
        // is allowed to open. Captured here because `session_context` borrows `ctx`.
        #[cfg(feature = "local_fs")]
        let working_directory: Option<String> = session_context.current_working_directory().clone();

        let manager = SkillManager::as_ref(ctx);

        // Cache hit: the proto's `SkillReference::Path(p)` only matches here when p
        // is exactly the real SKILL.md absolute path in the index.
        //
        // Uses `active_skill_by_reference_with_origin` (not `skill_by_reference`) so
        // a `BundledSkillId` reference is rejected once its activation condition is
        // no longer met (a `tui_only` skill read from the GUI, a feature-gated
        // skill whose flag flipped off, ...), and is resolved against the session's
        // host rather than always the local one. Path-based user skills are
        // unaffected by the activation check: they have no activation condition to
        // check. See issue #370.
        if let Ok(skill) =
            manager.active_skill_by_reference_with_origin(skill_ref, &path_origin, ctx)
        {
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
        // - `extract_skill_parent_directory` validates the path *shape* only, and shape
        //   is not permission: it accepts any prefix at all, so on its own it would let
        //   a model name `/home/someone-else/.agents/skills/x/SKILL.md`, or a SKILL.md
        //   symlinked at any file it likes, and have it read back with
        //   `should_autoexecute` unconditionally true. `skill_path_is_in_session_scope`
        //   supplies the missing permission by confining the read to exactly what a warm
        //   cache could have surfaced (home skills, plus skills owned by the working
        //   directory or an ancestor of it). In-project skills
        //   (`/some/repo/.agents/skills/...`) stay readable, which is the point of the
        //   fallback; a home-directory prefix is still not required.
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
            if let Some(local_path) = path.to_local_path() {
                // Normalise first, then judge the normalised form and read that same
                // form. `extract_skill_parent_directory` matches a regex against the
                // path *text*, so a reference still carrying `.`/`..` would be judged
                // in one spelling and opened in another — the exact shape that walks a
                // traversal past a guard which compares unresolved strings.
                let normalized = lexically_normalized(local_path);
                let working_directory = working_directory.as_deref().map(PathBuf::from);
                let home_directory = dirs::home_dir();
                if skill_path_is_in_session_scope(
                    &normalized,
                    working_directory.as_deref(),
                    home_directory.as_deref(),
                ) {
                    let skill_ref_for_async = skill_ref.clone();
                    return ActionExecution::new_async(
                        async move {
                            read_confined_skill(normalized, working_directory, home_directory)
                        },
                        move |parsed, _app| match parsed {
                            Ok(skill) => {
                                AIAgentActionResultType::ReadSkill(ReadSkillResult::Success {
                                    content: FileContext::new(
                                        skill.path.display_path(),
                                        AnyFileContent::StringContent(skill.content.clone()),
                                        skill.line_range.clone(),
                                        None,
                                    ),
                                })
                            }
                            Err(err) => AIAgentActionResultType::ReadSkill(ReadSkillResult::Error(
                                format!("Skill not found: {skill_ref_for_async:?} ({err})"),
                            )),
                        },
                    );
                }
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
fn success_execution(
    skill: &ai::skills::ParsedSkill,
) -> ActionExecution<anyhow::Result<ai::skills::ParsedSkill>> {
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

/// Lexically normalises `path`: `.` components are dropped and `..` pops the preceding
/// named component. Nothing on disk is touched.
///
/// `canonicalize` is deliberately *not* used for the shape/scope decision. It blocks, and
/// it returns `Err` for a path that does not exist — so a guard built on it has to rule on
/// "could not resolve", and the convenient ruling is the fail-open one. Symlinks are
/// handled separately, in [`read_confined_skill`], where the file must exist to be read at
/// all.
#[cfg(feature = "local_fs")]
fn lexically_normalized(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                // `/..` is `/`. There is nothing above the root to escape to.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                // A relative path that opens with `..` keeps it: dropping it would
                // silently reinterpret the path as relative to the process's cwd.
                _ => normalized.push(".."),
            },
            other => normalized.push(other.as_os_str()),
        }
    }
    normalized
}

/// Whether a cache-miss disk read of `skill_path` is inside what this session's skill
/// index could ever contain.
///
/// This mirrors [`crate::ai::skills::SkillManager::get_skills_for_working_directory_with_origin`]
/// exactly: that function surfaces the home directory's skills, plus skills whose owning
/// directory is an ancestor of (or equal to) the working directory. The fallback below is
/// only meant to cover "the skill exists but the cache is not warm yet" (issue #99), so it
/// must not reach anywhere the warm cache could not.
///
/// [`extract_skill_parent_directory`] is a *shape* check — it answers "does this look like
/// `<owner>/<provider>/skills/<name>/SKILL.md`", and happily accepts `/etc`, another user's
/// home, or a scratch directory as `<owner>`. Shape alone is therefore not a permission,
/// which is what this adds.
///
/// Fails closed in every unknown: no working directory and no home directory means nothing
/// is in scope. Absent is never treated as allowed.
#[cfg(feature = "local_fs")]
fn skill_path_is_in_session_scope(
    skill_path: &Path,
    working_directory: Option<&Path>,
    home_directory: Option<&Path>,
) -> bool {
    let Ok(owner) = extract_skill_parent_directory(&LocalOrRemotePath::Local(
        skill_path.to_path_buf(),
    )) else {
        return false;
    };
    let Some(owner) = owner.to_local_path().map(lexically_normalized) else {
        return false;
    };

    // Home skills are always in scope, exactly as the index has them (they are keyed on
    // the home directory itself, whatever the working directory is).
    if home_directory
        .map(lexically_normalized)
        .is_some_and(|home| home == owner)
    {
        return true;
    }

    // Project skills are in scope when their owning directory is an ancestor of the
    // working directory, or the working directory itself. `Path::starts_with` compares
    // whole components, so an owner of `/repo` does not match a working directory of
    // `/repo-secrets` the way a string prefix test would.
    working_directory
        .map(lexically_normalized)
        .is_some_and(|working_directory| working_directory.starts_with(&owner))
}

/// Reads a skill from disk after re-checking the symlink-resolved path against the same
/// scope test the caller already applied lexically.
///
/// The lexical test constrains the *name*. A symlink anywhere along that name still
/// resolves somewhere else when the file is opened, which is what would turn "reads a
/// SKILL.md inside this session's scope" into an arbitrary-file read — plant
/// `<repo>/.agents/skills/x/SKILL.md -> ~/.ssh/id_ed25519` and the lexical test says yes.
/// Here the file has to exist to be read anyway, so `canonicalize` costs nothing extra and
/// a failure is the same "skill not found" the caller already reports.
///
/// The roots are canonicalised too, or a repository reached through a symlinked path (a
/// macOS `TempDir` under `/var` -> `/private/var`, a home directory behind an automounter)
/// would be refused for no reason.
///
/// The skill is parsed at the path as named, not as resolved: the resolved form answers the
/// security question, the named form is what the model asked for and what the result should
/// report back.
#[cfg(feature = "local_fs")]
fn read_confined_skill(
    skill_path: PathBuf,
    working_directory: Option<PathBuf>,
    home_directory: Option<PathBuf>,
) -> anyhow::Result<ai::skills::ParsedSkill> {
    let resolved = std::fs::canonicalize(&skill_path)?;
    let working_directory = working_directory.and_then(|dir| std::fs::canonicalize(dir).ok());
    let home_directory = home_directory.and_then(|dir| std::fs::canonicalize(dir).ok());
    if !skill_path_is_in_session_scope(
        &resolved,
        working_directory.as_deref(),
        home_directory.as_deref(),
    ) {
        anyhow::bail!(
            "{} resolves outside this session's skills",
            skill_path.display()
        );
    }
    parse_skill(&skill_path)
}

impl Entity for ReadSkillExecutor {
    type Event = ();
}

#[cfg(test)]
#[path = "read_skill_tests.rs"]
mod tests;

/// Unit coverage for the cache-miss scope guard. These are pure path predicates, so they
/// need no `App` harness and can use paths that do not exist; the wiring into `execute` is
/// covered end to end in `read_skill_tests.rs`.
///
/// Unix path literals, so this is skipped on Windows, where `SKILL_FILE_PATTERN` itself
/// splits on backslashes.
#[cfg(all(test, feature = "local_fs", not(target_os = "windows")))]
mod scope_tests {
    use super::*;

    #[test]
    fn normalization_resolves_dot_and_parent_components() {
        assert_eq!(
            lexically_normalized(Path::new("/a/./b/../c")),
            Path::new("/a/c")
        );
        // Nothing above the root.
        assert_eq!(lexically_normalized(Path::new("/../../a")), Path::new("/a"));
        // A leading `..` on a relative path is preserved, not silently dropped.
        assert_eq!(lexically_normalized(Path::new("../a")), Path::new("../a"));
    }

    #[test]
    fn project_skill_under_the_working_directory_is_in_scope() {
        assert!(skill_path_is_in_session_scope(
            Path::new("/repo/.agents/skills/deploy/SKILL.md"),
            Some(Path::new("/repo/crate/src")),
            Some(Path::new("/home/user")),
        ));
    }

    #[test]
    fn home_skill_is_in_scope_without_a_working_directory() {
        assert!(skill_path_is_in_session_scope(
            Path::new("/home/user/.claude/skills/review/SKILL.md"),
            None,
            Some(Path::new("/home/user")),
        ));
    }

    #[test]
    fn well_shaped_path_outside_the_session_is_refused() {
        let outside = Path::new("/home/someone-else/.agents/skills/private/SKILL.md");
        // The shape check on its own accepts it — that is the whole point of this guard.
        assert!(
            extract_skill_parent_directory(&LocalOrRemotePath::Local(outside.to_path_buf()))
                .is_ok()
        );
        assert!(!skill_path_is_in_session_scope(
            outside,
            Some(Path::new("/repo")),
            Some(Path::new("/home/user")),
        ));
    }

    #[test]
    fn sibling_directory_sharing_a_name_prefix_is_refused() {
        // Component-wise containment, not string-prefix containment.
        assert!(!skill_path_is_in_session_scope(
            Path::new("/repo-secrets/.agents/skills/exfil/SKILL.md"),
            Some(Path::new("/repo")),
            None,
        ));
    }

    #[test]
    fn nothing_is_in_scope_without_a_working_directory_or_home() {
        assert!(!skill_path_is_in_session_scope(
            Path::new("/repo/.agents/skills/deploy/SKILL.md"),
            None,
            None,
        ));
    }

    #[test]
    fn traversal_out_of_the_skills_directory_is_refused() {
        // `<root>/.agents/skills/../SKILL.md` matches the provider regex with `..` as the
        // skill name, so validating the raw text would accept it while the open lands on
        // `<root>/.agents/SKILL.md`. Normalising first collapses it to a path with no
        // skill shape at all.
        let traversal = Path::new("/repo/.agents/skills/../SKILL.md");
        assert!(
            extract_skill_parent_directory(&LocalOrRemotePath::Local(traversal.to_path_buf()))
                .is_ok()
        );
        assert!(!skill_path_is_in_session_scope(
            &lexically_normalized(traversal),
            Some(Path::new("/repo")),
            None,
        ));
    }
}
