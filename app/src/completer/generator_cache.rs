//! Result cache and single-flight for completion generators (#1196).
//!
//! A generator is a read-only shell command from a command signature (`git branch`,
//! `systemctl list-units`, `docker ps`, ...) or a signature alias lookup (`git config --get
//! alias.checkout`). Every pipeline that reads the line (Tab, autosuggestion validation,
//! highlighting) used to spawn its own copy per call, and a copy superseded by the next keystroke
//! was killed, so none of that work was ever reused: Tab on `git checkout fe` or `systemctl
//! status ssh` paid 200-600 ms of process startup and git/systemd work from zero, and over ssh a
//! round trip each.
//!
//! This keeps, per session, one run per (pwd, script): a caller joins the run in flight or takes
//! a finished result younger than [`GENERATOR_CACHE_TTL`]. The cache is dropped when the user
//! submits a command or a block completes ([`invalidate_session`]), the same points as the
//! directory-listing cache of #1130, since a command may change what the generator lists.
//!
//! Only runs that ended on their own are cached. A run killed by `cancel_active_commands`
//! (no exit code), a spawn error and a timeout are returned to the callers that were waiting but
//! never stored, so the next caller starts a fresh run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use futures::future::{BoxFuture, Shared, WeakShared};
use lazy_static::lazy_static;
use warp_completer::completer::{CommandExitStatus, CommandOutput};

use crate::terminal::model::session::SessionId;

/// How long a finished generator result is reused.
///
/// #1381: not a correctness delay for anything this session does. The events that change a
/// generator's answer from inside the session (a command is submitted, a block completes, the
/// cwd changes -- the cwd is part of the key) already invalidate it ([`invalidate_session`]).
/// This bounds the staleness of a change made outside the session (another terminal, an editor,
/// a daemon), for which a generator like `git branch` or `docker ps` has no event source; a
/// watcher per cwd per generator would cost far more than the 5 s of reuse saves.
pub(super) const GENERATOR_CACHE_TTL: Duration = Duration::from_secs(5);

/// How long a git alias lookup is reused while the git config files it reads are unchanged
/// (#1342). The lookup (`git config --get-regexp '^alias.'`) answers from files only, so it
/// survives command submission; on Windows each copy is a `powershell -EncodedCommand` plus git.
pub(super) const ALIAS_LOOKUP_CACHE_TTL: Duration = Duration::from_secs(30 * 60);

/// Environment kill switch: set to `1` to give alias lookups the ordinary 5 s, drop-on-submit
/// lifetime again.
pub(super) const ALIAS_CACHE_DISABLE_ENV: &str = "PHOSPHOR_NO_ALIAS_LOOKUP_CACHE";

/// A generator that has not finished after this long is abandoned (and, locally, killed). The
/// run is detached from the caller that started it, so without a bound a hung generator
/// (`kubectl` without a network) would outlive every caller.
pub(super) const GENERATOR_RUN_TIMEOUT: Duration = Duration::from_secs(20);

/// Upper bound on stored slots; expired results are swept on every lookup, this only bounds a
/// burst of distinct scripts inside the TTL.
const GENERATOR_CACHE_MAX_SLOTS: usize = 64;

/// The outcome every waiter on one run shares. The error is the rendered message; `anyhow::Error`
/// is not `Clone`.
pub(super) type GeneratorRunResult = Arc<Result<CommandOutput, String>>;

pub(super) type SharedGeneratorRun = Shared<BoxFuture<'static, GeneratorRunResult>>;

#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct GeneratorKey {
    pub session_id: SessionId,
    pub pwd: String,
    pub script: String,
}

/// How the cache holds a run in flight. A detached (local) run is kept alive by the cache. An
/// attached (remote) run is held weakly: it is polled only by its waiters and dropped, which
/// aborts the remote command (#1128), when the last one goes away.
enum RunHandle {
    Detached(SharedGeneratorRun),
    Attached(WeakShared<BoxFuture<'static, GeneratorRunResult>>),
}

impl RunHandle {
    fn join(&self) -> Option<SharedGeneratorRun> {
        match self {
            RunHandle::Detached(run) => Some(run.clone()),
            RunHandle::Attached(weak) => weak.upgrade(),
        }
    }
}

/// Size and modification time of every git config file an alias lookup can read, taken when the
/// run starts. A result is reused past the ordinary TTL only while the stamp is unchanged, so a
/// `git config alias.x ...` or an edit of `.gitconfig` is never answered with the old aliases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ConfigStamp(Vec<(PathBuf, Option<(SystemTime, u64)>)>);

impl ConfigStamp {
    #[cfg(test)]
    pub(super) fn from_entries(entries: Vec<(PathBuf, Option<(SystemTime, u64)>)>) -> Self {
        Self(entries)
    }
}

/// Whether `script` is a git alias lookup (`git ... config --get-regexp '^alias.'`, `git config
/// --get alias.co`): a read of git config files only.
pub(super) fn is_alias_lookup_script(script: &str) -> bool {
    let script = script.trim_start();
    script.starts_with("git ") && script.contains(" config ") && script.contains("alias")
}

/// How deep `[include]` chains are followed (git itself stops at 10).
const CONFIG_INCLUDE_MAX_DEPTH: usize = 10;

/// The `path = ...` targets of every `[include]` / `[includeIf ...]` section in git config
/// `text`. Conditions are not evaluated: stamping a file git ends up not reading only costs a
/// spurious re-run when it changes. Relative targets are relative to the including file; `~/`
/// is the home directory.
pub(super) fn config_include_targets(
    text: &str,
    including_file: &Path,
    home: Option<&Path>,
) -> Vec<PathBuf> {
    let mut in_include = false;
    let mut targets = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[') {
            let name = section
                .split(|c: char| c == ']' || c.is_whitespace() || c == '"')
                .next()
                .unwrap_or("");
            in_include =
                name.eq_ignore_ascii_case("include") || name.eq_ignore_ascii_case("includeif");
            continue;
        }
        if !in_include {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.trim().eq_ignore_ascii_case("path") {
            continue;
        }
        let value = value.trim().trim_matches('"');
        if value.is_empty() {
            continue;
        }
        let target = if let Some(rest) = value.strip_prefix("~/") {
            match home {
                Some(home) => home.join(rest),
                None => continue,
            }
        } else if Path::new(value).is_absolute() {
            PathBuf::from(value)
        } else {
            including_file
                .parent()
                .unwrap_or_else(|| Path::new(""))
                .join(value)
        };
        targets.push(target);
    }
    targets
}

/// Every file in `roots` plus, transitively, the files they include.
pub(super) fn config_files_with_includes(roots: Vec<PathBuf>, home: Option<&Path>) -> Vec<PathBuf> {
    let mut seen: Vec<PathBuf> = Vec::new();
    let mut queue: Vec<(PathBuf, usize)> = roots.into_iter().map(|p| (p, 0)).collect();
    while let Some((path, depth)) = queue.pop() {
        if seen.contains(&path) {
            continue;
        }
        seen.push(path.clone());
        if depth >= CONFIG_INCLUDE_MAX_DEPTH {
            continue;
        }
        if let Ok(text) = std::fs::read_to_string(&path) {
            for target in config_include_targets(&text, &path, home) {
                queue.push((target, depth + 1));
            }
        }
    }
    seen.sort();
    seen
}

/// The repository's own config file for the repo containing `pwd`: `.git/config`, or for a
/// worktree/submodule the common dir's config that the `.git` pointer file leads to.
fn repo_config_path(pwd: &str) -> Option<PathBuf> {
    for dir in Path::new(pwd).ancestors() {
        let dot_git = dir.join(".git");
        if dot_git.is_dir() {
            return Some(dot_git.join("config"));
        } else if dot_git.is_file() {
            let text = std::fs::read_to_string(&dot_git).ok()?;
            let gitdir = text.trim().strip_prefix("gitdir:")?.trim();
            let gitdir = if Path::new(gitdir).is_absolute() {
                PathBuf::from(gitdir)
            } else {
                dir.join(gitdir)
            };
            // A linked worktree keeps `commondir` (usually `../..`); a submodule's gitdir is the
            // common dir itself.
            let common = match std::fs::read_to_string(gitdir.join("commondir")) {
                Ok(rel) => gitdir.join(rel.trim()),
                Err(_) => gitdir,
            };
            return Some(common.join("config"));
        }
    }
    None
}

/// Stamps every git config file an alias lookup can read from `pwd`: system, global (including
/// `$XDG_CONFIG_HOME`, `$GIT_CONFIG_GLOBAL` / `$GIT_CONFIG_SYSTEM`), the repository's own, and
/// everything they `[include]`. There is no event source for "git's config changed" (git offers
/// none), so the files are stamped, not watched.
pub(super) fn config_stamp(pwd: &str) -> ConfigStamp {
    let home = dirs::home_dir();
    let mut paths: Vec<PathBuf> = Vec::new();
    match std::env::var_os("GIT_CONFIG_SYSTEM") {
        Some(p) => paths.push(PathBuf::from(p)),
        None => {
            paths.push(PathBuf::from("/etc/gitconfig"));
            if let Some(pd) = std::env::var_os("PROGRAMDATA") {
                paths.push(Path::new(&pd).join("Git").join("config"));
            }
        }
    }
    if let Some(p) = std::env::var_os("GIT_CONFIG_GLOBAL") {
        paths.push(PathBuf::from(p));
    }
    if let Some(home) = &home {
        paths.push(home.join(".gitconfig"));
        paths.push(home.join(".config").join("git").join("config"));
    }
    if let Some(xdg) = std::env::var_os("XDG_CONFIG_HOME") {
        paths.push(Path::new(&xdg).join("git").join("config"));
    }
    paths.extend(repo_config_path(pwd));
    ConfigStamp(
        config_files_with_includes(paths, home.as_deref())
            .into_iter()
            .map(|path| {
                let meta = std::fs::metadata(&path)
                    .ok()
                    .map(|m| (m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len()));
                (path, meta)
            })
            .collect(),
    )
}

enum Slot {
    InFlight {
        run_id: u64,
        run: RunHandle,
        stamp: Option<ConfigStamp>,
    },
    Done {
        at: Instant,
        result: GeneratorRunResult,
        /// Set for results that outlive command submission (alias lookups).
        stamp: Option<ConfigStamp>,
    },
}

impl Slot {
    fn done_ttl(stamp: &Option<ConfigStamp>) -> Duration {
        if stamp.is_some() {
            ALIAS_LOOKUP_CACHE_TTL
        } else {
            GENERATOR_CACHE_TTL
        }
    }
}

#[derive(Default)]
pub(super) struct GeneratorCache {
    slots: HashMap<GeneratorKey, Slot>,
    next_run_id: u64,
}

lazy_static! {
    pub(super) static ref GENERATOR_CACHE: parking_lot::Mutex<GeneratorCache> =
        parking_lot::Mutex::new(GeneratorCache::default());
}

/// Drops every slot of `session_id`. A run still in flight keeps going for its waiters but its
/// result is not stored (it started before the command that invalidated it).
pub(super) fn invalidate_session(session_id: SessionId) {
    GENERATOR_CACHE.lock().invalidate_session(session_id);
}

/// A run was killed from outside (`cancel_active_commands`, a signal): no exit code and not a
/// success. Such a result belongs to a keystroke that is gone, not to a waiter that is still
/// here.
pub(super) fn is_cancelled_run(result: &GeneratorRunResult) -> bool {
    matches!(
        result.as_ref(),
        Ok(output) if output.status == CommandExitStatus::Failure && output.exit_code.is_none()
    )
}

/// Whether a finished run may be reused: it exited on its own (a failure with an exit code, such
/// as `git config --get` on a missing key, is as deterministic as a success).
fn is_cacheable(result: &GeneratorRunResult) -> bool {
    matches!(result.as_ref(), Ok(output) if output.exit_code.is_some())
}

impl GeneratorCache {
    /// Returns the finished result for `key`, or the run to wait on: the one already in flight,
    /// or a new one made by `start` (given the new run's id) and registered before the lock is
    /// released, so two callers cannot both start a run for one key.
    pub(super) fn get_or_start(
        &mut self,
        key: &GeneratorKey,
        now: Instant,
        start: impl FnOnce(u64) -> SharedGeneratorRun,
    ) -> Result<GeneratorRunResult, SharedGeneratorRun> {
        self.get_or_start_with(key, now, true, start)
    }

    /// [`Self::get_or_start`], with `detached` false for a non-local session: the run is then
    /// not kept alive by the cache, so it is aborted when no caller is waiting on it any more
    /// and a later caller starts a new one. A remote generator nobody waits for would occupy the
    /// shared ssh connection (#1128).
    pub(super) fn get_or_start_with(
        &mut self,
        key: &GeneratorKey,
        now: Instant,
        detached: bool,
        start: impl FnOnce(u64) -> SharedGeneratorRun,
    ) -> Result<GeneratorRunResult, SharedGeneratorRun> {
        self.get_or_start_stamped(key, now, detached, None, start)
    }

    /// [`Self::get_or_start_with`] for a lookup that reads only config files: with a `stamp`
    /// the finished result is kept for [`ALIAS_LOOKUP_CACHE_TTL`], survives
    /// [`Self::invalidate_session`], and is reused only while the stamp still matches.
    pub(super) fn get_or_start_stamped(
        &mut self,
        key: &GeneratorKey,
        now: Instant,
        detached: bool,
        stamp: Option<ConfigStamp>,
        start: impl FnOnce(u64) -> SharedGeneratorRun,
    ) -> Result<GeneratorRunResult, SharedGeneratorRun> {
        self.sweep(now);
        match self.slots.get(key) {
            Some(Slot::Done {
                at,
                result,
                stamp: done_stamp,
            }) if now.saturating_duration_since(*at) < Slot::done_ttl(done_stamp)
                && *done_stamp == stamp =>
            {
                return Ok(result.clone());
            }
            Some(Slot::InFlight { run, .. }) => {
                if let Some(run) = run.join() {
                    return Err(run);
                }
            }
            _ => {}
        }
        let run_id = self.next_run_id;
        self.next_run_id += 1;
        let run = start(run_id);
        let handle = if detached {
            RunHandle::Detached(run.clone())
        } else {
            match run.downgrade() {
                Some(weak) => RunHandle::Attached(weak),
                None => RunHandle::Detached(run.clone()),
            }
        };
        self.slots.insert(
            key.clone(),
            Slot::InFlight {
                run_id,
                run: handle,
                stamp,
            },
        );
        Err(run)
    }

    /// Records the end of run `run_id`. Ignored when the slot is no longer that run (it was
    /// invalidated, or replaced after an expiry).
    pub(super) fn finish(
        &mut self,
        key: &GeneratorKey,
        run_id: u64,
        result: &GeneratorRunResult,
        now: Instant,
    ) {
        let stamp = match self.slots.get(key) {
            Some(Slot::InFlight {
                run_id: id, stamp, ..
            }) if *id == run_id => stamp.clone(),
            _ => return,
        };
        if is_cacheable(result) {
            self.slots.insert(
                key.clone(),
                Slot::Done {
                    at: now,
                    result: result.clone(),
                    stamp,
                },
            );
        } else {
            self.slots.remove(key);
        }
    }

    pub(super) fn invalidate_session(&mut self, session_id: SessionId) {
        // A stamped result reads config files only and is checked against them on reuse, so a
        // submitted command does not make it stale.
        self.slots.retain(|key, slot| {
            key.session_id != session_id || matches!(slot, Slot::Done { stamp: Some(_), .. })
        });
    }

    fn sweep(&mut self, now: Instant) {
        self.slots.retain(|_, slot| match slot {
            Slot::Done { at, stamp, .. } => {
                now.saturating_duration_since(*at) < Slot::done_ttl(stamp)
            }
            // An attached run whose last waiter went away is dead: nothing will finish it.
            Slot::InFlight { run, .. } => run.join().is_some(),
        });
        if self.slots.len() >= GENERATOR_CACHE_MAX_SLOTS {
            self.slots
                .retain(|_, slot| matches!(slot, Slot::InFlight { .. }));
        }
    }
}

#[cfg(test)]
#[path = "generator_cache_tests.rs"]
mod tests;
