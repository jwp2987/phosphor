use anyhow::Result;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warpui::{Entity, ModelContext, SingletonEntity};

use super::GlobalRules;

/// Default list of rule files. Order = priority (earlier wins); when multiple
/// files coexist in the same directory, `RuleAtPath::respected_rule()` only
/// picks the highest-priority one.
///
/// - WARP.md  the project's native convention.
/// - AGENTS.md community-wide convention (recognized by opencode / Cursor / Cline etc.).
/// - CLAUDE.md Claude Code's native convention, so projects migrated from Claude Code work out of the box.
///
/// Adding a new name only requires adjusting this array (insertion position =
/// priority); `RuleAtPath` is implemented as a priority-indexed slot array, so
/// no if-else logic needs to change.
///
/// Defined outside `cfg_if` so that paths not compiling `local_fs` (WASM / tests) can still reference it.
pub(crate) const RULES_FILE_PATTERN: &[&str] = &["WARP.md", "AGENTS.md", "CLAUDE.md"];

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        use repo_metadata::entry::{Entry, FileMetadata};
        use repo_metadata::repository::RepositorySubscriber;
        use repo_metadata::{Repository, DirectoryWatcher, RepositoryUpdate};
        use repo_metadata::{RepositoryIdentifier, StandingQueryContent};
        use ignore::gitignore::Gitignore;
        use async_channel::Sender;
        // `instant::Instant` is this repo's global convention for a cross-platform
        // (including WASM) time origin, replacing `std::time::Instant`. Enforced via
        // `disallowed_types` in `clippy.toml`.
        use instant::Instant;
        use std::time::{Duration, SystemTime};

        const MAX_SCAN_DEPTH: usize = 3;
        const MAX_FILES_TO_SCAN: usize = 5000;

        // —— Fast-path (aligned with opencode's `findUp` pattern) ——
        //
        // Main purpose: in the time window after `cd`-ing into a new git repo,
        // before the async `index_and_store_rules` completes, `pending_context`
        // synchronously calls this fast-path to directly stat + read the rule
        // files in cwd and its ancestor directories, ensuring AGENTS.md / WARP.md /
        // CLAUDE.md **never get dropped due to async race conditions**.
        // Once the normal path (`find_applicable_rules`) becomes available, the
        // fast-path steps aside and clears its cache.
        //
        // UI-never-stutters guarantees:
        //   - Worst case per call: `MAX_WALK_DEPTH * RULES_FILE_PATTERN.len()` metadata
        //     calls + `read_to_string` on hit files (rule files are typically a few KB,
        //     Windows NTFS < 1ms/file).
        //   - `FAST_PATH_BUDGET` hard-caps the time budget; on timeout it immediately
        //     returns what's been collected so far, never blocking.
        //   - Steady-state hits (directory unchanged) only stat, never re-read files;
        //     any change in mtime / size / parent-dir-mtime triggers a rescan.
        const MAX_WALK_DEPTH: usize = 6;
        const FAST_PATH_BUDGET: Duration = Duration::from_millis(20);
    }
}

/// Converts a repository's standing-query rule-file results into the paths
/// [`ProjectContextModel::reconcile_project_rules`] should treat as "still
/// standing" for that repository — filtering out directories (a rule *file*
/// replacing what was a directory, or vice versa, is exactly the transition
/// a standing-query diff can produce) and host-qualifying remote entries.
///
/// Ported from the pinned oracle (`02b53fcd8`, release `2026.07.29.09.05`
/// stable)'s free function of the same name, unchanged in shape. Bridges
/// `RemoteRepositoryIdentifier::host_id` (`warp_core::HostId`, the family
/// `repo_metadata` speaks) to `warp_util::host_id::HostId` (the family this
/// module speaks — see `HostId` above, and `remote_global_rules`/
/// `remote_path_to_rules`, which are keyed by it) by value. This fork
/// carries two structurally-identical but non-converting `HostId` newtypes;
/// `app/src/code/buffer_location.rs::core_host_id_to_util` does the same
/// bridge for the app crate, duplicated here rather than depending on `app`
/// (which depends on `ai`, not the reverse).
///
/// No non-test caller yet — see [`ProjectContextModel::reconcile_project_rules`]'s
/// doc comment for why.
#[cfg(feature = "local_fs")]
#[cfg_attr(not(test), allow(dead_code))]
fn standing_project_rule_paths<'a>(
    repo_id: &RepositoryIdentifier,
    contents: impl IntoIterator<Item = &'a StandingQueryContent>,
) -> Vec<LocalOrRemotePath> {
    contents
        .into_iter()
        .filter(|content| !content.is_directory)
        .filter_map(|content| match repo_id {
            RepositoryIdentifier::Local(_) => {
                content.path.to_local_path().map(LocalOrRemotePath::Local)
            }
            RepositoryIdentifier::Remote(remote_root) => {
                Some(LocalOrRemotePath::Remote(RemotePath::new(
                    HostId::new(remote_root.host_id.as_str().to_string()),
                    content.path.clone(),
                )))
            }
        })
        .collect()
}

/// Fast-path cache entry. `stamps` records the (path, mtime, size) of files that
/// were hit; `walked_dir_stamps` records the (path, mtime) of directories that
/// were walked, used to detect two kinds of invalidation: "a rule file was
/// added / removed / modified in a directory". The `negative` cache means the
/// last scan found no rules at all, so subsequent calls with identical stamps
/// can reuse the result without any IO.
#[cfg(feature = "local_fs")]
#[derive(Clone, Debug)]
struct FastPathEntry {
    rules: Vec<ProjectRule>,
    /// The "root" used by the fast-path — the directory of the **first-level hit**;
    /// falls back to cwd itself if everything misses.
    /// Used to construct `ProjectRulesResult.root_path`, semantically aligned with `find_applicable_rules`.
    root_path: PathBuf,
    stamps: Vec<(PathBuf, SystemTime, u64)>,
    walked_dir_stamps: Vec<(PathBuf, SystemTime)>,
}

#[derive(Debug, Default, Clone)]
pub struct ProjectRule {
    pub path: PathBuf,
    pub content: String,
}

#[derive(Debug, Default)]
struct RuleAtPath {
    parent_path: PathBuf,
    warp_md: Option<ProjectRule>,
    agents_md: Option<ProjectRule>,
    claude_md: Option<ProjectRule>,
}

impl RuleAtPath {
    fn respected_rule(&self) -> Option<&ProjectRule> {
        // Priority order matches RULES_FILE_PATTERN: WARP.md > AGENTS.md > CLAUDE.md.
        self.warp_md
            .as_ref()
            .or(self.agents_md.as_ref())
            .or(self.claude_md.as_ref())
    }
}

#[derive(Debug, Default, Clone)]
pub struct ProjectRulesResult {
    pub root_path: PathBuf,
    pub active_rules: Vec<ProjectRule>,
    pub additional_rule_paths: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRulePath {
    pub path: PathBuf,
    pub project_root: PathBuf,
}

struct FindRulesResult {
    /// Rules that are active and should be eagerly applied.
    active_rules: Vec<ProjectRule>,
    /// Rule paths that are currently not active but available to be applied if
    /// a file under its directory is edited.
    available_rule_paths: Vec<String>,
}

#[cfg(feature = "local_fs")]
fn matches_rules_pattern(file_name_str: &str) -> bool {
    for pattern in RULES_FILE_PATTERN {
        if file_name_str.to_lowercase() == pattern.to_lowercase() {
            return true;
        }
    }
    false
}

#[derive(Debug, Default)]
struct ProjectRules {
    rules: Vec<RuleAtPath>,
}

impl ProjectRules {
    /// Every rule file path currently held, regardless of priority — unlike
    /// [`RuleAtPath::respected_rule`], which only surfaces the
    /// highest-priority file per directory. Ported from the pinned oracle
    /// (`02b53fcd8`)'s `rule_paths`, extended with `claude_md` (this fork's
    /// third priority slot; the pin only has `warp_md`/`agents_md`).
    ///
    /// No non-test caller yet — see
    /// [`ProjectContextModel::reconcile_project_rules`]'s doc comment.
    #[cfg_attr(not(test), allow(dead_code))]
    fn rule_paths(&self) -> impl Iterator<Item = &PathBuf> {
        self.rules.iter().flat_map(|rule| {
            rule.warp_md
                .iter()
                .chain(rule.agents_md.iter())
                .chain(rule.claude_md.iter())
                .map(|rule| &rule.path)
        })
    }

    /// Drops any held rule file whose path is not in `retained_paths`,
    /// dropping the whole `RuleAtPath` entry once none of its slots survive.
    /// Ported unchanged in shape from the pinned oracle (`02b53fcd8`)'s
    /// `retain_rule_paths`, extended with `claude_md` (see [`Self::rule_paths`]).
    ///
    /// No non-test caller yet — see
    /// [`ProjectContextModel::reconcile_project_rules`]'s doc comment, whose
    /// only call site today is `model_tests.rs`.
    #[cfg_attr(not(test), allow(dead_code))]
    fn retain_rule_paths(&mut self, retained_paths: &HashSet<PathBuf>) {
        self.rules.retain_mut(|rule| {
            if rule
                .warp_md
                .as_ref()
                .is_some_and(|rule| !retained_paths.contains(&rule.path))
            {
                rule.warp_md = None;
            }
            if rule
                .agents_md
                .as_ref()
                .is_some_and(|rule| !retained_paths.contains(&rule.path))
            {
                rule.agents_md = None;
            }
            if rule
                .claude_md
                .as_ref()
                .is_some_and(|rule| !retained_paths.contains(&rule.path))
            {
                rule.claude_md = None;
            }
            rule.warp_md.is_some() || rule.agents_md.is_some() || rule.claude_md.is_some()
        });
    }

    /// Finds the set of rules that are active in the given path and the set that are available to be applied.
    fn find_active_or_applicable_rules(&self, path: &Path) -> FindRulesResult {
        let mut active_rules = Vec::new();
        let mut available_rule_paths = Vec::new();

        // Collect all applicable rules (rules in directories that are ancestors of the target path)
        for rule in &self.rules {
            if let Some(respected_rule) = rule.respected_rule() {
                // Check if the rule's directory is an ancestor of or equal to the target path
                if path.starts_with(&rule.parent_path) {
                    active_rules.push(respected_rule.clone());
                } else {
                    available_rule_paths.push(respected_rule.path.to_string_lossy().to_string());
                }
            }
        }

        FindRulesResult {
            active_rules,
            available_rule_paths,
        }
    }

    /// Remove a rule from the set of project rules. This returns the removed rule.
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn remove_rule(&mut self, path: &Path) -> Option<ProjectRule> {
        let parent = path.parent()?;
        let file_name = path.file_name().and_then(|name| name.to_str())?;

        let rule = self
            .rules
            .iter_mut()
            .find(|rule| rule.parent_path == parent)?;

        if file_name.to_lowercase() == "warp.md" {
            rule.warp_md.take()
        } else if file_name.to_lowercase() == "agents.md" {
            rule.agents_md.take()
        } else if file_name.to_lowercase() == "claude.md" {
            rule.claude_md.take()
        } else {
            None
        }
    }

    /// Upsert a rule to the set of project rules. This will create a new RuleAtPath entry if none exists and update the existing one
    /// otherwise.
    #[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
    fn upsert_rule(&mut self, path: &Path, content: String) {
        let Some(parent) = path.parent() else {
            return;
        };
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            return;
        };

        let existing_rule = self
            .rules
            .iter_mut()
            .find(|rule| rule.parent_path == parent);

        let rule_file = Some(ProjectRule {
            path: path.to_path_buf(),
            content,
        });

        match existing_rule {
            Some(rule) => {
                if file_name.to_lowercase() == "warp.md" {
                    rule.warp_md = rule_file;
                } else if file_name.to_lowercase() == "agents.md" {
                    rule.agents_md = rule_file;
                } else if file_name.to_lowercase() == "claude.md" {
                    rule.claude_md = rule_file;
                }
            }
            None => {
                let mut rule = RuleAtPath {
                    parent_path: parent.to_path_buf(),
                    ..Default::default()
                };
                if file_name.to_lowercase() == "warp.md" {
                    rule.warp_md = rule_file;
                } else if file_name.to_lowercase() == "agents.md" {
                    rule.agents_md = rule_file;
                } else if file_name.to_lowercase() == "claude.md" {
                    rule.claude_md = rule_file;
                }
                self.rules.push(rule);
            }
        };
    }
}

/// Singleton model that keeps track of mapping between paths and rule files
/// Currently supports WARP.md files, but designed to be extensible
#[cfg_attr(not(feature = "local_fs"), allow(dead_code))]
#[derive(Debug, Default)]
pub struct ProjectContextModel {
    /// Mapping from directory path to list of rule files found in that directory
    path_to_rules: HashMap<PathBuf, ProjectRules>,
    /// Fast-path synchronous rule cache (aligned with opencode's `findUp` pattern).
    ///
    /// Only used as a fallback when `find_applicable_rules` returns None (async
    /// indexing not ready yet / not under an already-indexed root), to avoid
    /// dropping AGENTS.md / WARP.md when an AI request fires right after a `cd`.
    /// Single-threaded access (WarpUI Singleton runs on the main thread), so we
    /// use `RefCell` instead of a lock, matching the `&self` call shape of
    /// `pending_context(&self, app: &AppContext)`.
    #[cfg(feature = "local_fs")]
    fast_path_cache: RefCell<HashMap<PathBuf, FastPathEntry>>,
    /// File-based global rules (e.g. `~/.agents/AGENTS.md`) and their local
    /// watcher state. Kept separate from `path_to_rules`, which is
    /// project-scoped. `pub(super)` (matching the pin) because
    /// `global_rules.rs` — a sibling module of `model.rs`, not a descendant —
    /// reaches into this field directly (`me.global_rules.rules...` inside
    /// its `ctx.spawn`/`ctx.subscribe_to_model` callbacks, where `me: &mut
    /// ProjectContextModel`); a private field would only be visible to
    /// `model.rs` and its own descendants (e.g. `model_tests.rs`), not to
    /// `project_context`'s other children. #575.
    pub(super) global_rules: GlobalRules,
    /// File-based global rules published by connected remote hosts, keyed by
    /// the host that published them. Populated client-side by
    /// `app::ai::remote_agent_context::RemoteAgentContext` from each
    /// connected host's `RemoteAgentContextSnapshot.global_rules` (itself
    /// produced daemon-side from that host's own `global_rules`, see
    /// `remote_agent_context_snapshot` in `app/src/remote_server/
    /// server_model.rs`), and cleared on host disconnect. #575.
    ///
    /// `path_to_rules` itself still has no per-host dimension at all
    /// (`ProjectRule::path` there is a plain local `PathBuf`, not a
    /// `LocalOrRemotePath`) — see `remote_path_to_rules` below for the
    /// *project*-rule counterpart added alongside it, following the same
    /// per-host-`HashMap` pattern rather than teaching either local map a
    /// host dimension. `remote_project_rules` (below) layers this field
    /// (global) but not `remote_path_to_rules` (project-scoped, since that
    /// method takes no target path); `remote_project_rules_for_path` layers
    /// both.
    remote_global_rules: HashMap<HostId, Vec<ProjectRule>>,
    /// Per-host project rules (WARP.md/AGENTS.md/CLAUDE.md discovered under
    /// a remote host's project root), keyed first by host then by that
    /// host's project root — the *project*-scoped remote counterpart of
    /// `remote_global_rules` above, added the same way: a parallel
    /// per-host `HashMap` rather than adding a host dimension to the
    /// existing local-only `path_to_rules`.
    ///
    /// `ProjectRule::path` and the inner map's `PathBuf` keys stay plain
    /// local-shaped `PathBuf`s for entries here too — same convention as
    /// `remote_global_rules` (see above): they are the *raw* path from that
    /// host's wire representation, reparsed locally, meaningful only
    /// combined with the `HostId` key that disambiguates them from every
    /// other host's (and from local paths').
    ///
    /// Consulted by `find_applicable_project_rules` (via
    /// `find_remote_project_rules`) and `remote_project_rules_for_path`.
    /// Scaffolding in the same sense `remote_global_rules` originally was
    /// (before #575 gave it `RemoteAgentContext` as a real writer): nothing
    /// in this fork's production wiring populates it yet — there is no
    /// remote equivalent of `register_watcher_for_path`/
    /// `scan_directory_for_rules` for a connected host's project rules — so
    /// today it is populated only by tests
    /// (`model_tests.rs::insert_remote_project_rule`), poking the field
    /// directly, the same way `insert_project_rule` already pokes
    /// `path_to_rules`.
    remote_path_to_rules: HashMap<HostId, HashMap<PathBuf, ProjectRules>>,
}

#[derive(Default, Debug)]
pub struct RulesDelta {
    pub discovered_rules: Vec<ProjectRulePath>,
    pub deleted_rules: Vec<PathBuf>,
}

/// Delta of file-based global rule files (e.g. `~/.agents/AGENTS.md`)
/// discovered or removed since the last event. Ported from the pinned oracle
/// (`02b53fcd8`); field-identical to the pin's `GlobalRulesDelta` since both
/// are local-only (`PathBuf`) by construction — see `global_rules.rs`'s
/// module doc comment.
#[derive(Default, Debug)]
pub struct GlobalRulesDelta {
    pub discovered_rules: Vec<PathBuf>,
    pub deleted_rules: Vec<PathBuf>,
}

impl RulesDelta {
    /// Merge another delta into this one, preserving the ordering of operations.
    ///
    /// When the same path appears across sequential deltas the *last* operation
    /// wins. For example:
    ///   - (add A, delete A) -> net effect is **delete**
    ///   - (delete A, add A) -> net effect is **add**
    ///
    /// This is important because consumers (e.g. persistence) apply the delta
    /// incrementally; a symmetric "cancel both sides" approach would silently
    /// drop real state changes.
    ///
    /// Ported from the pinned oracle (`02b53fcd8`, release `2026.07.29.09.05`
    /// stable), where this is also `#[cfg(test)]`-only: `merge` has no
    /// production call site there either (`model.rs` never calls it outside
    /// `model_tests.rs`), so gating it to tests matches upstream rather than
    /// inventing a production use this fork doesn't have. Refs #150 item 2.
    #[cfg(test)]
    fn merge(&mut self, other: RulesDelta) {
        // Each newly-discovered path supersedes any prior deletion or earlier
        // discovery of the same path.
        for discovered in &other.discovered_rules {
            self.deleted_rules.retain(|p| *p != discovered.path);
            self.discovered_rules.retain(|r| r.path != discovered.path);
        }
        // Each newly-deleted path supersedes any prior discovery or earlier
        // deletion of the same path.
        for deleted in &other.deleted_rules {
            self.discovered_rules.retain(|r| r.path != *deleted);
            self.deleted_rules.retain(|p| *p != *deleted);
        }
        self.discovered_rules.extend(other.discovered_rules);
        self.deleted_rules.extend(other.deleted_rules);
    }
}

/// Events emitted by the ProjectContextModel
pub enum ProjectContextModelEvent {
    /// Emitted when a path has been indexed
    PathIndexed,
    /// Emitted when the known set of rule files changed
    KnownRulesChanged(RulesDelta),
    /// Emitted when the set of indexed global rule files changed. #575.
    GlobalRulesChanged(GlobalRulesDelta),
}

impl ProjectContextModel {
    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
    pub fn new_from_persisted(
        persisted_rules: Vec<ProjectRulePath>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        #[cfg(feature = "local_fs")]
        ctx.spawn(
            async move { Self::read_persisted_rules(persisted_rules).await },
            |me, mut res, ctx| {
                // Zap: this used to call `try_initialize_and_register_watcher`
                // for each persisted root, which internally went through
                // `DetectedRepositories::detect_possible_git_repo(ProjectRulesIndexing)`
                // to fire an event, causing RepoMetadataModel to fully index all 6
                // persisted repos (the single biggest cold-start background CPU
                // cost for Zap BYOP).
                //
                // Now it only populates the in-memory path_to_rules cache and
                // does not proactively fire a detect event. When the user later
                // `cd`s into the repo via the terminal,
                // RepoDetectionSource::TerminalNavigation naturally triggers an
                // independent detect, which then goes through
                // register_watcher_for_path.
                //
                // Practical effect: persisted rules are not watched in real time
                // until the user enters that repo. The cache itself is still
                // usable, so AI rule lookups are unaffected.
                res.extend(me.path_to_rules.drain());
                me.path_to_rules = res;
                ctx.emit(ProjectContextModelEvent::PathIndexed);
            },
        );

        Self::default()
    }

    /// Index a path and find all rule files from that path up to the root directory
    /// Returns a list of all rule files found
    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
    pub fn index_and_store_rules(
        &mut self,
        root_path: PathBuf,
        ctx: &mut ModelContext<Self>,
    ) -> Result<()> {
        if self.path_to_rules.contains_key(&root_path) {
            return Ok(());
        }
        #[cfg(feature = "local_fs")]
        {
            let root_clone = root_path.clone();

            ctx.spawn(
                async move { Self::scan_directory_for_rules(&root_path).await },
                move |me, res: Result<ProjectRules>, ctx| match res {
                    Ok(rule_files) => {
                        me.register_watcher_for_path(&root_clone, ctx);

                        // Persist the discovered rules.
                        let delta = RulesDelta {
                            discovered_rules: rule_files
                                .rules
                                .iter()
                                .filter_map(|rule| {
                                    rule.warp_md.as_ref().map(|rule| ProjectRulePath {
                                        project_root: root_clone.clone(),
                                        path: rule.path.clone(),
                                    })
                                })
                                .chain(rule_files.rules.iter().filter_map(|rule| {
                                    rule.agents_md.as_ref().map(|rule| ProjectRulePath {
                                        project_root: root_clone.clone(),
                                        path: rule.path.clone(),
                                    })
                                }))
                                .collect(),
                            deleted_rules: Default::default(),
                        };
                        ctx.emit(ProjectContextModelEvent::KnownRulesChanged(delta));

                        me.path_to_rules.insert(root_clone, rule_files);
                        ctx.emit(ProjectContextModelEvent::PathIndexed);
                    }
                    Err(e) => log::warn!(
                        "Couldn't index rules for path {}: {}",
                        root_clone.display(),
                        e
                    ),
                },
            );
        }

        Ok(())
    }

    // Zap: `try_initialize_and_register_watcher` used to be the entry point
    // that forced a repo detect when starting from a persisted rule path,
    // which then went through a full RepoMetadataModel index. It was removed
    // along with the detect call in `new_from_persisted`; now
    // register_watcher_for_path is only reached passively via the
    // `RepoDetectionSource::TerminalNavigation` path triggered by terminal cd.

    #[cfg(feature = "local_fs")]
    fn register_watcher_for_path(&self, path: &Path, ctx: &mut ModelContext<Self>) {
        let Some(repository_model) =
            DirectoryWatcher::as_ref(ctx).get_watched_directory_for_path(path)
        else {
            return;
        };

        let (repository_update_tx, repository_update_rx) = async_channel::unbounded();
        let start = repository_model.update(ctx, |repo, ctx| {
            repo.start_watching(
                Box::new(ProjectContextRepositorySubscriber {
                    repository_update_tx,
                }),
                ctx,
            )
        });

        let subscriber_id = start.subscriber_id;
        let repository_model_for_cleanup = repository_model.downgrade();
        let path_clone = path.to_path_buf();
        let path_for_log = path_clone.clone();
        ctx.spawn(start.registration_future, move |_, res, ctx| {
            if let Err(err) = res {
                log::warn!(
                    "Failed to start watching repository for rule updates at {}: {err}",
                    path_for_log.display()
                );

                if let Some(repository_model) = repository_model_for_cleanup.upgrade(ctx) {
                    repository_model.update(ctx, |repo, ctx| {
                        repo.stop_watching(subscriber_id, ctx);
                    });
                }
            }
        });

        ctx.spawn_stream_local(
            repository_update_rx.clone(),
            move |me, update, ctx| {
                if update.is_empty() {
                    return;
                }

                let existing_rules = me.path_to_rules.remove(&path_clone);
                let repo_path = path_clone.clone();
                if let Some(rules) = existing_rules {
                    let repo_path_for_closure = repo_path.clone();
                    ctx.spawn(
                        async move {
                            Self::process_repository_updates(update, rules, repo_path).await
                        },
                        move |me, (rules, rule_delta), ctx| {
                            ctx.emit(ProjectContextModelEvent::KnownRulesChanged(rule_delta));

                            me.path_to_rules.insert(repo_path_for_closure, rules);
                            ctx.emit(ProjectContextModelEvent::PathIndexed);
                        },
                    );
                }
            },
            |_, _| {},
        );
    }

    /// Reconciles a project root's cached rules against the paths currently
    /// known to be "standing" (still present at the last standing-query
    /// scan) plus whatever content was freshly read for them: a standing
    /// path with no fresh content (the read hasn't completed, or failed)
    /// keeps its cached content; a cached rule whose path is no longer
    /// standing at all is dropped.
    ///
    /// Ported from the pinned oracle (`02b53fcd8`, release `2026.07.29.09.05`
    /// stable), adapted: the pin's version takes `Vec<LocalOrRemotePath>` /
    /// `Vec<(LocalOrRemotePath, String)>` because there `path_to_rules` is a
    /// single map spanning local and every connected remote host. This fork
    /// keeps local (`path_to_rules`) and per-host remote
    /// (`remote_path_to_rules`) rules in separate maps (see
    /// `remote_path_to_rules`'s doc comment), so a single reconcile call
    /// always operates within one already-selected root regardless of
    /// origin — plain `PathBuf`s are enough, and the pin's per-path
    /// local/remote dispatch inside the loop body isn't needed here.
    ///
    /// Like `RulesDelta::merge` above (ported under the same reasoning),
    /// this fork has no non-test caller yet: the pin calls this from
    /// `refresh_project_rules_for_repo`, which hangs off
    /// `RepoMetadataModel`'s standing-query results — a different discovery
    /// pipeline than this fork's directory-watcher-based
    /// `register_watcher_for_path`/`process_repository_updates` above. Ported
    /// as a pure function so its behavior — the actual test debt this closes
    /// — is covered without inventing a call site upstream doesn't have
    /// either. Refs #150 item 2, #170.
    #[cfg_attr(not(test), allow(dead_code))]
    fn reconcile_project_rules(
        rule_paths: Vec<PathBuf>,
        rule_contents: Vec<(PathBuf, String)>,
        mut existing_rules: ProjectRules,
    ) -> ProjectRules {
        let retained_paths: HashSet<PathBuf> = rule_paths.into_iter().collect();
        existing_rules.retain_rule_paths(&retained_paths);
        for (path, content) in rule_contents {
            existing_rules.upsert_rule(&path, content);
        }
        existing_rules
    }

    /// Remote counterpart of [`Self::find_applicable_rules`]'s indexed-root
    /// walk, backed by `remote_path_to_rules` (see that field's doc comment)
    /// instead of the local-only `path_to_rules`. Walks from `remote`'s path
    /// up through its `StandardizedPath` ancestors — mirroring the local
    /// walk's `PathBuf::pop` loop — stopping at the first directory this
    /// host has indexed rules for. Returns `None` immediately for a host
    /// with nothing stored at all, so a mismatched host never falls through
    /// to another host's rules.
    fn find_remote_project_rules(&self, remote: &RemotePath) -> Option<ProjectRulesResult> {
        let host_rules = self.remote_path_to_rules.get(&remote.host_id)?;
        let full_path = PathBuf::from(remote.path.as_str());

        let mut current = remote.path.clone();
        loop {
            let current_path = PathBuf::from(current.as_str());
            if let Some(rules) = host_rules.get(&current_path) {
                let result = rules.find_active_or_applicable_rules(&full_path);
                if result.active_rules.is_empty() && result.available_rule_paths.is_empty() {
                    return None;
                }
                return Some(ProjectRulesResult {
                    root_path: current_path,
                    active_rules: result.active_rules,
                    additional_rule_paths: result.available_rule_paths,
                });
            }
            current = current.parent()?;
        }
    }

    /// Like [`Self::find_applicable_rules`], but accepts a `LocalOrRemotePath`.
    /// `Local` paths delegate to the local-only walk; `Remote` paths are
    /// looked up in `remote_path_to_rules` via
    /// [`Self::find_remote_project_rules`] — see that field's doc comment
    /// for why remote project rules live in a separate per-host structure
    /// rather than `path_to_rules` growing a `LocalOrRemotePath` key.
    pub fn find_applicable_project_rules(
        &self,
        path: &LocalOrRemotePath,
    ) -> Option<ProjectRulesResult> {
        match path {
            LocalOrRemotePath::Local(path) => self.find_applicable_rules(path),
            LocalOrRemotePath::Remote(remote) => self.find_remote_project_rules(remote),
        }
    }

    pub fn find_applicable_rules(&self, path: &Path) -> Option<ProjectRulesResult> {
        let mut current_path = path.to_owned();
        let mut active_rules = Vec::new();
        let mut available_rule_paths = Vec::new();

        // Find the root path with indexed rules and collect active rules
        let mut found_rules = false;
        loop {
            if let Some(rules) = self.path_to_rules.get(&current_path) {
                let result = rules.find_active_or_applicable_rules(path);

                active_rules = result.active_rules;
                available_rule_paths = result.available_rule_paths;

                found_rules = true;
                break;
            }

            if !current_path.pop() {
                break;
            }
        }

        if !found_rules {
            return None;
        }

        if active_rules.is_empty() && available_rule_paths.is_empty() {
            return None;
        }

        Some(ProjectRulesResult {
            root_path: current_path,
            active_rules,
            additional_rule_paths: available_rule_paths,
        })
    }

    /// Unified entry point for rule lookups: the normal path takes priority,
    /// with the synchronous fast-path as a fallback when async indexing isn't
    /// ready yet. Layers file-based global rules (e.g. `~/.agents/AGENTS.md`,
    /// see `Self::index_global_rules`) on top of whichever project result is
    /// found, matching `Self::find_applicable_rules_with_globals`. This is
    /// the entry point used to pack `AIAgentContext::ProjectRules` for an
    /// agent query (`app/src/ai/blocklist/context_model.rs::pending_context`),
    /// so it is deliberately the layered variant rather than the project-only
    /// `find_applicable_rules`. #575.
    ///
    /// Aligned with opencode's `Instruction.systemPaths()` `findUp` behavior
    /// (`opencode/packages/opencode/src/session/instruction.ts`): stat rule
    /// files level by level upward from cwd, stopping at the first-level hit.
    /// The fast-path and the normal path **never coexist**: as soon as the
    /// normal path returns Some, the corresponding fast-path cache entry is
    /// immediately cleared, ensuring that once indexing completes, subsequent
    /// requests go through the normal path 100% of the time (which can pick up
    /// subdirectory rules + real-time watcher updates).
    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
    pub fn find_rules_with_fast_path(&self, cwd: &Path) -> Option<ProjectRulesResult> {
        let project_result = if let Some(found) = self.find_applicable_rules(cwd) {
            #[cfg(feature = "local_fs")]
            {
                // Normal path is now available; drop the fast-path cache (avoid stale data later).
                self.fast_path_cache.borrow_mut().remove(cwd);
            }
            Some(found)
        } else {
            #[cfg(feature = "local_fs")]
            {
                self.fast_path_lookup(cwd)
            }
            #[cfg(not(feature = "local_fs"))]
            {
                None
            }
        };
        self.layer_global_rules(project_result)
    }

    /// Like [`Self::find_applicable_rules`], but layers file-based global
    /// rules on top of the project rules found for `path`. Global rules are
    /// always included when present, regardless of whether a project root
    /// was found — matching the pin's `find_applicable_rules(&LocalOrRemotePath)`
    /// layering semantics (global first, then project). #575.
    ///
    /// Deliberately a *separate* method from `find_applicable_rules` rather
    /// than folding global-layering into it: `find_applicable_rules` has an
    /// existing project-only caller (`app/src/code_review/
    /// code_review_view.rs`'s "Repo is initialized with a {file_name} file."
    /// hint) that must not have a stray `~/.agents/AGENTS.md` flip every repo
    /// into "already initialized" — exactly the regression the pin's own
    /// `specs/APP-3893/TECH.md` documents fixing by keeping that exact
    /// distinction. Only local; there is no remote-host counterpart *here*
    /// specifically — `find_applicable_project_rules` now has a
    /// remote-aware sibling walk (`remote_path_to_rules`, see its doc
    /// comment), and `remote_project_rules_for_path` layers global +
    /// remote-global + remote-project the same way this method does for
    /// local paths, but nothing folds all of that into a single
    /// `LocalOrRemotePath`-typed entry point mirroring the pin's unified
    /// `find_applicable_rules` — this method stays local-`Path`-only.
    pub fn find_applicable_rules_with_globals(&self, path: &Path) -> Option<ProjectRulesResult> {
        self.layer_global_rules(self.find_applicable_rules(path))
    }

    /// Layers `self.global_rules` on top of an already-computed project
    /// lookup. `project_result` may be `None` (no project root indexed / no
    /// rules found there) — global-only results are still returned, with
    /// `root_path` falling back to the parent of the first active rule.
    fn layer_global_rules(
        &self,
        project_result: Option<ProjectRulesResult>,
    ) -> Option<ProjectRulesResult> {
        let mut active_rules: Vec<ProjectRule> = self.global_rules.active_rules().collect();
        let (root_path, additional_rule_paths) = match project_result {
            Some(project) => {
                active_rules.extend(project.active_rules);
                (Some(project.root_path), project.additional_rule_paths)
            }
            None => (None, Vec::new()),
        };

        if active_rules.is_empty() && additional_rule_paths.is_empty() {
            return None;
        }

        let root_path = root_path.or_else(|| {
            active_rules
                .first()
                .and_then(|rule| rule.path.parent().map(Path::to_path_buf))
        })?;

        Some(ProjectRulesResult {
            root_path,
            active_rules,
            additional_rule_paths,
        })
    }

    /// Fast-path synchronous lookup + read of rule files in cwd and ancestor
    /// directories. Only called when the normal path returns None.
    ///
    /// Return semantics match `find_applicable_rules`:
    ///   - Some(ProjectRulesResult) with at least 1 active rule
    ///   - None means no rules found at all (negative cache written; subsequent
    ///     calls with identical stamps skip IO)
    #[cfg(feature = "local_fs")]
    fn fast_path_lookup(&self, cwd: &Path) -> Option<ProjectRulesResult> {
        // 1) Cache-hit path: stat all stamps; if they all still match, reuse the cache (no re-reading files).
        if let Some(entry) = self.fast_path_cache.borrow().get(cwd).cloned() {
            if Self::fast_path_entry_still_valid(&entry) {
                return Self::result_from_fast_path_entry(&entry);
            }
        }

        // 2) Cache miss / invalidated: synchronous scan. `FAST_PATH_BUDGET` hard-caps the time, so the UI never stalls.
        let entry = Self::scan_fast_path(cwd);
        let result = Self::result_from_fast_path_entry(&entry);
        self.fast_path_cache
            .borrow_mut()
            .insert(cwd.to_path_buf(), entry);
        result
    }

    /// Synchronously stat + read rule files level by level upward from `start`.
    /// Aligned with opencode's `findUp`, but adds the dual safeguard of
    /// `MAX_WALK_DEPTH` + `FAST_PATH_BUDGET` so the UI never blocks.
    ///
    /// At each level, picks the first hit per `RULES_FILE_PATTERN` (WARP.md >
    /// AGENTS.md), aligned with `RuleAtPath::respected_rule()` semantics.
    #[cfg(feature = "local_fs")]
    fn scan_fast_path(start: &Path) -> FastPathEntry {
        let deadline = Instant::now() + FAST_PATH_BUDGET;
        let mut rules = Vec::new();
        let mut stamps = Vec::new();
        let mut walked_dir_stamps = Vec::new();
        let mut first_hit_dir: Option<PathBuf> = None;
        let mut current: PathBuf = start.to_path_buf();

        for _ in 0..MAX_WALK_DEPTH {
            if Instant::now() >= deadline {
                break;
            }

            // Record the directory mtime, so we can later detect the two kinds
            // of change: "a rule file was added/removed in this directory".
            if let Ok(meta) = std::fs::metadata(&current) {
                if let Ok(mtime) = meta.modified() {
                    walked_dir_stamps.push((current.clone(), mtime));
                }
            }

            // Find the first rule file at this level by priority. Aligned with RuleAtPath::respected_rule() semantics.
            for filename in RULES_FILE_PATTERN {
                if Instant::now() >= deadline {
                    break;
                }
                let candidate = current.join(filename);
                let Ok(meta) = std::fs::metadata(&candidate) else {
                    continue;
                };
                if !meta.is_file() {
                    continue;
                }
                let Ok(mtime) = meta.modified() else { continue };
                let size = meta.len();
                let Ok(content) = std::fs::read_to_string(&candidate) else {
                    continue;
                };
                if first_hit_dir.is_none() {
                    first_hit_dir = Some(current.clone());
                }
                rules.push(ProjectRule {
                    path: candidate.clone(),
                    content,
                });
                stamps.push((candidate, mtime, size));
                break; // Only take 1 per level
            }

            if !current.pop() {
                break;
            }
        }

        FastPathEntry {
            root_path: first_hit_dir.unwrap_or_else(|| start.to_path_buf()),
            rules,
            stamps,
            walked_dir_stamps,
        }
    }

    /// Cache invalidation check. Only stats, never reads file content.
    /// - Hit-file mtime/size unchanged → content can be reused
    /// - Walked-directory mtime unchanged → no rule file was added/removed
    ///
    /// Bounded by `FAST_PATH_BUDGET`; a timeout during stat-ing is treated as invalidated and triggers a rescan.
    #[cfg(feature = "local_fs")]
    fn fast_path_entry_still_valid(entry: &FastPathEntry) -> bool {
        let deadline = Instant::now() + FAST_PATH_BUDGET;
        for (path, mtime, size) in &entry.stamps {
            if Instant::now() >= deadline {
                return false;
            }
            let Ok(meta) = std::fs::metadata(path) else {
                return false;
            };
            if meta.len() != *size {
                return false;
            }
            if meta.modified().ok().as_ref() != Some(mtime) {
                return false;
            }
        }
        for (dir, mtime) in &entry.walked_dir_stamps {
            if Instant::now() >= deadline {
                return false;
            }
            let Ok(meta) = std::fs::metadata(dir) else {
                return false;
            };
            if meta.modified().ok().as_ref() != Some(mtime) {
                return false;
            }
        }
        true
    }

    /// Convert a FastPathEntry into the unified public `ProjectRulesResult`.
    /// Empty rules returns None, semantically aligned with `find_applicable_rules`.
    #[cfg(feature = "local_fs")]
    fn result_from_fast_path_entry(entry: &FastPathEntry) -> Option<ProjectRulesResult> {
        if entry.rules.is_empty() {
            return None;
        }
        Some(ProjectRulesResult {
            root_path: entry.root_path.clone(),
            active_rules: entry.rules.clone(),
            additional_rule_paths: Vec::new(),
        })
    }

    #[cfg(feature = "local_fs")]
    async fn process_repository_updates(
        repository_update: RepositoryUpdate,
        mut existing_rules: ProjectRules,
        project_root: PathBuf,
    ) -> (ProjectRules, RulesDelta) {
        let mut rules_delta = RulesDelta::default();
        // Handle deleted files - remove rules for deleted rule files
        for target_file in &repository_update.deleted {
            // Skip gitignored files
            if target_file.is_ignored {
                continue;
            }
            if let Some(file_name_str) = target_file.path.file_name().and_then(|name| name.to_str())
            {
                if matches_rules_pattern(file_name_str) {
                    // Remove the rule from existing rules
                    existing_rules.remove_rule(&target_file.path);
                    rules_delta.deleted_rules.push(target_file.path.clone());

                    log::debug!("Removed rule file: {}", target_file.path.display());
                }
            }
        }

        // Handle moved files - update paths for moved rule files
        for (to_target, from_target) in &repository_update.moved {
            // Skip gitignored files
            if to_target.is_ignored || from_target.is_ignored {
                continue;
            }
            if let Some(file_name_str) = to_target.path.file_name().and_then(|name| name.to_str()) {
                if matches_rules_pattern(file_name_str) {
                    // Find and update the rule with the old path
                    if let Some(rule) = existing_rules.remove_rule(&from_target.path) {
                        // Emit deletion event for old path
                        rules_delta.deleted_rules.push(from_target.path.clone());

                        existing_rules.upsert_rule(&to_target.path, rule.content);

                        // Emit upsert event for new path
                        rules_delta.discovered_rules.push(ProjectRulePath {
                            path: to_target.path.clone(),
                            project_root: project_root.clone(),
                        });

                        log::debug!(
                            "Updated rule file path: {} -> {}",
                            from_target.path.display(),
                            to_target.path.display()
                        );
                    }
                }
            }
        }

        // Handle added/updated files - upsert rules for rule files
        for target_file in repository_update.added_or_modified() {
            // Skip gitignored files
            if target_file.is_ignored {
                continue;
            }
            if let Some(file_name_str) = target_file.path.file_name().and_then(|name| name.to_str())
            {
                if matches_rules_pattern(file_name_str) {
                    // Read the content of the rule file
                    match async_fs::read_to_string(&target_file.path).await {
                        Ok(content) => {
                            existing_rules.upsert_rule(&target_file.path, content);
                        }
                        Err(e) => {
                            log::warn!(
                                "Failed to read updated rule file {}: {}",
                                target_file.path.display(),
                                e
                            );
                        }
                    }
                }
            }
        }

        (existing_rules, rules_delta)
    }

    /// Scan a directory for rule files (currently WARP.md, extensible for future file types)
    /// Uses repo_metadata::entry::build_tree for efficient directory traversal
    #[cfg(feature = "local_fs")]
    async fn scan_directory_for_rules(dir_path: &Path) -> Result<ProjectRules> {
        use repo_metadata::entry::IgnoredPathStrategy;

        let mut rule_files = ProjectRules::default();

        if !async_fs::metadata(dir_path).await?.is_dir() {
            return Ok(rule_files);
        }

        // Use build_tree to collect all files, then filter for rule files
        let mut files = Vec::<FileMetadata>::new();
        let mut gitignores = Vec::<Gitignore>::new();

        // Collect patterns that should not be ignored
        let override_ignore_patterns: Vec<String> =
            RULES_FILE_PATTERN.iter().map(|s| s.to_string()).collect();
        let mut file_limit = MAX_FILES_TO_SCAN;

        // Build the file tree using repo_metadata's build_tree function
        let ignore_behavior = IgnoredPathStrategy::IncludeOnly(override_ignore_patterns.clone());

        let _ = Entry::build_tree(
            dir_path,
            &mut files,
            &mut gitignores,
            Some(&mut file_limit),
            MAX_SCAN_DEPTH,
            0,
            &ignore_behavior,
            repo_metadata::entry::BudgetExceededBehavior::FailFast,
        )
        .await?;

        // Filter files to only include those matching RULES_FILE_PATTERN
        for file_metadata in files {
            let path = &file_metadata.path;
            let file_name = path.file_name();

            if let Some(file_name_str) = file_name {
                if matches_rules_pattern(file_name_str) {
                    // Read the content of the rule file
                    let local_path = file_metadata.path.to_local_path_lossy();
                    let content = match async_fs::read_to_string(&local_path).await {
                        Ok(content) => content,
                        Err(e) => {
                            log::warn!("Failed to read rule file {}: {e}", file_metadata.path,);
                            break;
                        }
                    };

                    rule_files.upsert_rule(&local_path, content);
                }
            }
        }

        Ok(rule_files)
    }

    #[cfg(feature = "local_fs")]
    async fn read_persisted_rules(
        rule_paths: Vec<ProjectRulePath>,
    ) -> HashMap<PathBuf, ProjectRules> {
        let mut rules: HashMap<PathBuf, ProjectRules> = HashMap::new();

        for rule in rule_paths {
            match async_fs::read_to_string(&rule.path).await {
                Ok(content) => {
                    let existing_rules = rules.entry(rule.project_root).or_default();
                    existing_rules.upsert_rule(&rule.path, content);
                }
                Err(e) => {
                    log::debug!(
                        "Failed to read rule file from persistence {}: {}",
                        rule.path.display(),
                        e
                    );
                    // Continue processing other files even if one fails
                }
            }
        }

        rules
    }

    pub fn indexed_rules(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.path_to_rules.values().flat_map(|rules| {
            rules.rules.iter().filter_map(|rules| {
                rules
                    .respected_rule()
                    .map(|project_rule| project_rule.path.clone())
            })
        })
    }

    /// Returns the rule file paths associated with a specific workspace root path.
    pub fn rules_for_workspace(&self, workspace_path: &Path) -> Vec<PathBuf> {
        self.path_to_rules
            .get(workspace_path)
            .into_iter()
            .flat_map(|rules| {
                rules.rules.iter().filter_map(|rule| {
                    rule.respected_rule()
                        .map(|project_rule| project_rule.path.clone())
                })
            })
            .collect()
    }

    /// Index all configured global rule sources (e.g. `~/.agents/AGENTS.md`).
    ///
    /// `ProjectContextModel` remains the public rule-context facade; the
    /// global source registry, cache, and watcher plumbing live in
    /// `global_rules` (`crates/ai/src/project_context/global_rules.rs`).
    /// Called once at startup — see `app/src/lib.rs` (local app/TUI) and
    /// `app/src/remote_server/mod.rs::run_daemon_app` (remote daemon), both
    /// of which run their own `ProjectContextModel` singleton. #575.
    pub fn index_global_rules(&mut self, ctx: &mut ModelContext<Self>) {
        self.global_rules.index(ctx);
    }

    /// Returns every indexed global rule with its cached content, sorted by
    /// path. This is what the remote daemon serializes into
    /// `RemoteAgentContextSnapshot.global_rules` for connected clients (see
    /// `remote_agent_context_snapshot` in `app/src/remote_server/
    /// server_model.rs`), and what `find_applicable_rules_with_globals` /
    /// `find_rules_with_fast_path` layer on top of project rules locally.
    /// #575.
    pub fn global_rules(&self) -> impl Iterator<Item = ProjectRule> + '_ {
        self.global_rules.active_rules()
    }

    /// Absolute locations of every indexed global rule file (e.g.
    /// `~/.agents/AGENTS.md`), without their content. #575.
    pub fn global_rule_paths(&self) -> impl Iterator<Item = PathBuf> + '_ {
        self.global_rules.paths()
    }

    /// Replaces the file-based global rule catalog published by one
    /// connected remote host, keyed by `warp_util::host_id::HostId` — the
    /// same per-host `HashMap` pattern `BundledSkills`
    /// (`app/src/ai/skills/bundled.rs`) uses for remote skill catalogs
    /// (#487/#353). Called from `app::ai::remote_agent_context::
    /// RemoteAgentContext::reconcile_snapshot` as `RemoteAgentContextSnapshot`s
    /// arrive.
    ///
    /// `rules` carries the *raw* paths from the wire (that host's own
    /// `global_rules()` at the time of the snapshot) reparsed locally as
    /// `PathBuf`s — this fork's `ProjectRule::path` has no `LocalOrRemotePath`
    /// variant (unlike the pin), so these `PathBuf`s do not name a location
    /// on *this* machine; they are meaningful only in combination with the
    /// `host_id` key that disambiguates them. #575: this method and
    /// `remote_global_rules` are scaffolding only — nothing in this fork
    /// currently layers a host's stored entries into a rule lookup for a
    /// path on that host (there is no remote-path-aware `pending_context`
    /// equivalent here to layer them into), so populated entries are inert
    /// until such a consumer exists.
    pub fn set_remote_global_rules(&mut self, host_id: HostId, mut rules: Vec<ProjectRule>) {
        rules.sort_by(|a, b| a.path.cmp(&b.path));
        self.remote_global_rules.insert(host_id, rules);
    }

    /// Removes the file-based global rule catalog for a disconnected remote
    /// host. Called from `RemoteAgentContext::remove_host_context`. #575.
    pub fn remove_remote_global_rules(&mut self, host_id: &HostId) {
        self.remote_global_rules.remove(host_id);
    }

    /// Returns the currently-stored global rule catalog for one remote host,
    /// or an empty slice if none is stored (never connected, or removed on
    /// disconnect). Mirrors `BundledSkills::remote` (`app/src/ai/skills/
    /// bundled.rs`)'s per-host accessor shape. #575.
    pub fn remote_global_rules(&self, host_id: &HostId) -> &[ProjectRule] {
        self.remote_global_rules
            .get(host_id)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// The remote-host counterpart to [`Self::find_rules_with_fast_path`]: builds a
    /// `ProjectRulesResult` for a connected remote host, for `app/src/ai/blocklist/
    /// context_model.rs::pending_context` to layer into `AIAgentContext::ProjectRules`
    /// when the active session is that host. `None` when there is nothing to show
    /// (no local global rules indexed, and nothing stored for `host_id`).
    ///
    /// Layers this client's own file-based global rules ([`Self::global_rules`],
    /// e.g. this machine's `~/.agents/AGENTS.md`) ahead of `host_id`'s stored remote
    /// global rules ([`Self::remote_global_rules`]) — same "local global rules apply
    /// everywhere" precedent as the pin's `find_applicable_rules(&LocalOrRemotePath)`
    /// (its `test_remote_global_rules_only_layer_for_matching_remote_host` asserts a
    /// local-global entry appears for every host, matched-host remote entries appear
    /// only for their own host, and content is fully isolated between hosts).
    ///
    /// This method takes no target path (only a `host_id`), so — unlike the
    /// local path — it cannot layer path-scoped project rules on top; it
    /// only ever surfaces global ones (local + that host's). `path_to_rules`
    /// itself still has no `HostId` dimension (see `remote_global_rules`'s
    /// doc comment), but as of `remote_path_to_rules` (see its doc comment)
    /// project-scoped remote rules are no longer categorically unavailable —
    /// a caller with an actual target path should use
    /// [`Self::remote_project_rules_for_path`] instead, which layers all
    /// three (local global, remote global, remote project).
    ///
    /// `root_path` is synthesized as the parent of the first active rule's path,
    /// matching `layer_global_rules`'s global-only fallback; a remote rule's raw wire
    /// path is meaningful only in combination with `host_id`, not as a location on
    /// this machine, but `root_path` here is display-only (it labels the rules in
    /// `AIAgentContext::ProjectRules`, never used for further filesystem lookups).
    pub fn remote_project_rules(&self, host_id: &HostId) -> Option<ProjectRulesResult> {
        let mut active_rules: Vec<ProjectRule> = self.global_rules.active_rules().collect();
        active_rules.extend(self.remote_global_rules(host_id).iter().cloned());
        if active_rules.is_empty() {
            return None;
        }
        let root_path = active_rules.first()?.path.parent()?.to_path_buf();
        Some(ProjectRulesResult {
            root_path,
            active_rules,
            additional_rule_paths: Vec::new(),
        })
    }

    /// Like [`Self::remote_project_rules`], but also layers this host's
    /// project-scoped rules ([`Self::find_remote_project_rules`], backed by
    /// `remote_path_to_rules`) for a specific remote path — the adapted
    /// counterpart of the pin's single `find_applicable_rules` applied to a
    /// `LocalOrRemotePath::Remote`, restoring
    /// `test_remote_global_rules_only_layer_for_matching_remote_host`'s
    /// coverage.
    ///
    /// Kept as a separate, additive method rather than changing
    /// [`Self::remote_project_rules`]'s signature: that method's only caller
    /// (`app/src/ai/blocklist/context_model.rs`'s `pending_context`) has no
    /// path parameter to feed it today, and threading one through is a
    /// wiring change outside the scope of restoring this test's coverage —
    /// this method exists so the *behavior* is covered and available to a
    /// future caller that does have a path.
    ///
    /// Layering order matches the pin: local global rules first, then
    /// `host_id`'s remote global rules, then `host_id`'s remote project
    /// rules for `remote`'s path.
    pub fn remote_project_rules_for_path(&self, remote: &RemotePath) -> Option<ProjectRulesResult> {
        let mut active_rules: Vec<ProjectRule> = self.global_rules.active_rules().collect();
        active_rules.extend(self.remote_global_rules(&remote.host_id).iter().cloned());

        let project_result = self.find_remote_project_rules(remote);
        let (root_path, additional_rule_paths) = match project_result {
            Some(project) => {
                active_rules.extend(project.active_rules);
                (Some(project.root_path), project.additional_rule_paths)
            }
            None => (None, Vec::new()),
        };

        if active_rules.is_empty() && additional_rule_paths.is_empty() {
            return None;
        }

        let root_path = root_path.or_else(|| {
            active_rules
                .first()
                .and_then(|rule| rule.path.parent().map(Path::to_path_buf))
        })?;

        Some(ProjectRulesResult {
            root_path,
            active_rules,
            additional_rule_paths,
        })
    }
}

impl Entity for ProjectContextModel {
    type Event = ProjectContextModelEvent;
}

impl SingletonEntity for ProjectContextModel {}

#[cfg(feature = "local_fs")]
struct ProjectContextRepositorySubscriber {
    repository_update_tx: Sender<RepositoryUpdate>,
}

#[cfg(feature = "local_fs")]
impl RepositorySubscriber for ProjectContextRepositorySubscriber {
    fn on_scan(
        &mut self,
        _repository: &Repository,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::prelude::rust_2024::Future<Output = ()> + Send + 'static>> {
        // The model can safely ignore the initial scan because the model only subscribes
        // after the repository is already scanned.
        Box::pin(async {})
    }

    fn on_files_updated(
        &mut self,
        _repository: &Repository,
        update: &repo_metadata::RepositoryUpdate,
        _ctx: &mut ModelContext<Repository>,
    ) -> std::pin::Pin<Box<dyn std::prelude::rust_2024::Future<Output = ()> + Send + 'static>> {
        let tx = self.repository_update_tx.clone();
        let update = update.clone();
        Box::pin(async move {
            let _ = tx.send(update).await;
        })
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
