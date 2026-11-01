mod generator_cache;
#[cfg(feature = "completions_v2")]
mod js;

use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use futures::FutureExt;
use lazy_static::lazy_static;
use smol_str::SmolStr;
use typed_path::{TypedPath, TypedPathBuf};
use warp_completer::completer::{
    CommandExitStatus, CommandOutput, CompletionContext, EngineDirEntry, EngineFileType,
    GeneratorContext, PathCompletionContext, PathSeparators, TopLevelCommandCaseSensitivity,
};
use warp_completer::signatures::CommandRegistry;
use warp_core::features::FeatureFlag;
use warp_util::path::{EscapeChar, ShellDialect, ShellFamily};
use warpui::{AppContext, SingletonEntity};

use crate::safe_warn;
use crate::terminal::model::session::{ExecuteCommandOptions, Session, SessionId, SessionType};
use crate::util::AsciiDebug;
use crate::workflows::aliases::WorkflowAliases;

lazy_static! {
    pub static ref CURR_DIRECTORY_ENTRY: EngineDirEntry = EngineDirEntry {
        file_name: ".".to_owned(),
        file_type: EngineFileType::Directory,
    };
    pub static ref PARENT_DIRECTORY_ENTRY: EngineDirEntry = EngineDirEntry {
        file_name: "..".to_owned(),
        file_type: EngineFileType::Directory,
    };
    static ref EMPTY_COMMAND_REGISTRY: Arc<CommandRegistry> = Arc::new(CommandRegistry::empty());
}

/// The exit status `ssh` itself returns when it could not run the command (no usable
/// ControlMaster, connection lost) -- not a verdict from the remote shell.
const SSH_TRANSPORT_EXIT_CODE: i32 = 255;

/// Attempts at one remote directory listing before it is given up as empty (and not cached).
const REMOTE_LISTING_ATTEMPTS: usize = 2;

type DirectoryEntryMap = dashmap::DashMap<TypedPathBuf, Arc<Vec<EngineDirEntry>>>;

/// How long a non-local session's directory listings are reused across keystrokes (#1130).
const SHARED_DIRECTORY_CACHE_TTL: Duration = Duration::from_secs(2);

struct SharedDirectoryCacheSlot {
    cwd: TypedPathBuf,
    created: Instant,
    entries: Arc<DirectoryEntryMap>,
}

lazy_static! {
    /// One directory-listing cache per NON-LOCAL session, shared by every `SessionContext`
    /// built for it. `SessionContext::new` runs on every keystroke (autosuggestion, completions,
    /// highlighting), so a per-context cache never hit across keystrokes and each one
    /// re-listed the directory over the shared ssh connection (#1130, the #985 mechanism).
    /// A slot is reused only while the cwd matches and it is younger than
    /// [`SHARED_DIRECTORY_CACHE_TTL`]; block completion and command submit drop it so files
    /// created by a command are visible to the next completion.
    static ref SHARED_DIRECTORY_CACHES: parking_lot::Mutex<HashMap<SessionId, SharedDirectoryCacheSlot>> =
        parking_lot::Mutex::new(HashMap::new());
}

fn shared_directory_cache_at(
    session_id: SessionId,
    cwd: &TypedPathBuf,
    now: Instant,
) -> Arc<DirectoryEntryMap> {
    let mut caches = SHARED_DIRECTORY_CACHES.lock();
    // Expired slots (including those of closed sessions) are dead weight; sweep them.
    caches
        .retain(|_, slot| now.saturating_duration_since(slot.created) < SHARED_DIRECTORY_CACHE_TTL);
    if let Some(slot) = caches.get(&session_id)
        && slot.cwd == *cwd
    {
        return slot.entries.clone();
    }
    let entries = Arc::new(DirectoryEntryMap::default());
    caches.insert(
        session_id,
        SharedDirectoryCacheSlot {
            cwd: cwd.clone(),
            created: now,
            entries: entries.clone(),
        },
    );
    entries
}

/// How long a remote directory whose listing definitively failed (the shell ran the script and
/// it exited non-zero, e.g. the directory does not exist or is unreadable, or its output was
/// malformed) is not asked for again (#1295). Deliberately longer than
/// [`SHARED_DIRECTORY_CACHE_TTL`] and NOT dropped when a command runs: a directory that failed
/// to list is almost never fixed by the next command, and re-asking once per command is the
/// traffic #1295 measured. An explicit Tab forgets it ([`forget_failed_remote_listings`]).
const FAILED_LISTING_TTL: Duration = Duration::from_secs(15);

lazy_static! {
    /// Negative cache for remote listings, keyed by (session, absolute directory).
    static ref FAILED_REMOTE_LISTINGS: parking_lot::Mutex<HashMap<(SessionId, TypedPathBuf), Instant>> =
        parking_lot::Mutex::new(HashMap::new());
}

fn remote_listing_failed_recently_at(
    session_id: SessionId,
    directory: &TypedPathBuf,
    now: Instant,
) -> bool {
    let mut failed = FAILED_REMOTE_LISTINGS.lock();
    failed.retain(|_, at| now.saturating_duration_since(*at) < FAILED_LISTING_TTL);
    failed.contains_key(&(session_id, directory.clone()))
}

fn note_remote_listing_failed_at(session_id: SessionId, directory: TypedPathBuf, now: Instant) {
    FAILED_REMOTE_LISTINGS
        .lock()
        .insert((session_id, directory), now);
}

/// Forgets the session's failed-listing memory. Called for an explicit Tab, which must always
/// try the remote again (the user may have just created the directory).
pub fn forget_failed_remote_listings(session_id: SessionId) {
    FAILED_REMOTE_LISTINGS
        .lock()
        .retain(|(id, _), _| *id != session_id);
}

/// Drops the session's shared directory-listing cache. Called when a block completes or a
/// command is submitted: the command may have created or removed files.
pub fn invalidate_shared_directory_cache(session_id: SessionId) {
    SHARED_DIRECTORY_CACHES.lock().remove(&session_id);
    // The same command may have changed what a generator lists (a new branch, a started unit).
    generator_cache::invalidate_session(session_id);
}

/// Synchronous local directory listing. Only ever call this from a blocking-pool thread (see
/// [`list_local_dir_off_pool`]): on a hung NFS/sshfs/autofs mount `read_dir` can sit in the
/// kernel indefinitely.
fn read_local_dir_entries(dir: &Path) -> Vec<EngineDirEntry> {
    // Converting a `DirEntry` to `EngineDirEntry` can usually be
    // done without additional syscalls (though one is necessary if
    // the entry is a symlink).
    //
    // A failed listing and an empty directory are the same thing to
    // every caller above this point -- both are "no path
    // completions" -- so a failure here is invisible unless it is
    // logged. `NotFound` is the ordinary case while a path is
    // half-typed and is deliberately not logged; `PermissionDenied`
    // is the one worth a line, because it is how a session that
    // escalated to another user (`sudo su -`, `ksu`) presents.
    // Such a session keeps the same hostname, so
    // `determine_session_type` classifies it `Local` (see
    // `terminal/model/session.rs`, "Ensures subshells are treated as
    // local") and we read the directory with the Zap *process's*
    // uid, which is not the uid the session's shell now runs as.
    // Nothing downstream can recover from that; the point of the
    // log is that the user's log says so instead of the feature
    // silently doing nothing.
    let read_dir = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir,
        Err(err) => {
            if err.kind() != std::io::ErrorKind::NotFound {
                safe_warn!(
                    safe: (
                        "Failed to list a local directory for completions: {:?}",
                        err.kind()
                    ),
                    full: (
                        "Failed to list `{}` for completions: {err}",
                        dir.display()
                    )
                );
            }
            return vec![];
        }
    };

    read_dir
        .filter_map(|res| res.and_then(EngineDirEntry::try_from).ok())
        .collect::<Vec<_>>()
}

/// How long a local directory listing may take before completions give up on it.
const LOCAL_LISTING_TIMEOUT: std::time::Duration = std::time::Duration::from_millis(1500);

static LOCAL_LISTING_TIMEOUT_LOGGED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Runs `lister` on tokio's blocking pool and waits at most `timeout` for it. Returns `None`
/// on timeout (or if the listing task died).
///
/// The listing used to run inline on a shared async worker. Every keystroke starts a new
/// listing and one cannot be aborted mid-syscall, so a few keystrokes at `cd /mnt/hung/<tab>`
/// parked all `num_cpus` workers and stalled every `ctx.spawn` task in every tab. The blocking
/// pool is separate and large, so a stuck `read_dir` costs one idle thread, not the executor.
/// The abandoned thread finishes whenever the kernel lets it; its result is dropped.
async fn list_local_dir_off_pool<F>(
    dir: std::path::PathBuf,
    timeout: std::time::Duration,
    lister: F,
) -> Option<Vec<EngineDirEntry>>
where
    F: FnOnce(std::path::PathBuf) -> Vec<EngineDirEntry> + Send + 'static,
{
    // Outside a tokio runtime (some unit-test executors) there is no blocking pool.
    if tokio::runtime::Handle::try_current().is_err() {
        return Some(lister(dir));
    }
    let shown = dir.clone();
    let task = tokio::task::spawn_blocking(move || lister(dir));
    match tokio::time::timeout(timeout, task).await {
        Ok(Ok(entries)) => Some(entries),
        Ok(Err(err)) => {
            log::warn!("Local directory listing task failed: {err}");
            None
        }
        Err(_) => {
            if !LOCAL_LISTING_TIMEOUT_LOGGED.swap(true, AtomicOrdering::Relaxed) {
                safe_warn!(
                    safe: ("Local directory listing for completions timed out; returning no entries"),
                    full: (
                        "Listing `{}` for completions took over {timeout:?}; returning no entries (logged once)",
                        shown.display()
                    )
                );
            }
            None
        }
    }
}

/// Coalescing gate for one directory path, used by
/// [`SessionContext::refresh_directory_entries`].
///
/// `completions` counts refreshes that have finished for this path. A caller snapshots it
/// BEFORE queueing on `lock`; if it has moved by the time the lock is acquired, another
/// caller already produced a fresh listing while this one waited, and this caller can take
/// that result instead of issuing a second remote command.
struct RefreshGate {
    lock: tokio::sync::Mutex<()>,
    completions: AtomicU64,
}

#[derive(Clone)]
pub struct SessionContext {
    pub session: Arc<Session>,
    command_registry: Arc<CommandRegistry>,
    pub current_working_directory: TypedPathBuf,

    #[cfg(feature = "completions_v2")]
    js_ctx: Option<js::SessionJsExecutionContext>,

    /// Directory listings keyed by absolute path. Callers that must reflect a directory's
    /// current contents should use `refresh_directory_entries` to re-read from disk.
    ///
    /// **The `Arc` is load-bearing, and its absence is not a style question.** `SessionContext`
    /// is `Clone`, and the context-chip layer clones it freely — once per chip
    /// (`context_chips/display.rs:312`) and once per `DirectoryFetcher`
    /// (`context_chips/display_chip.rs:935`). `DashMap`'s `Clone` deep-copies, so without the
    /// `Arc` every clone gets a private map that starts empty and whose inserts are invisible
    /// to every other clone: the cache can never hit across consumers, and it never expires
    /// into correctness either — it is simply dead.
    ///
    /// On a local session that costs a duplicated `std::fs::read_dir`. On a legacy-SSH session
    /// `list_directory_entries_internal` shells out to `find` over the session's ControlMaster,
    /// so each clone opens its **own SSH channel** for the same directory. Observed 2026-08-15
    /// on a remote host: three identical
    /// `cd /home/winters/srv/scripts && find . -maxdepth 1 -type d -print0 ...` in flight at
    /// once, alongside the completer's `compgen -c` and two `git branch` callers. Past the
    /// host's `MaxSessions` (default 10) sshd refuses the channel and OpenSSH prints
    /// `channel N: open failed: connect failed: open failed` into the user's live shell.
    ///
    /// Upstream carries the `Arc` (pin `02b53fcd8`, same field). This fork descends from
    /// `0dbd3d56` ("Initial public release of Warp"), which predates that fix, so this is an
    /// unported upstream change rather than something the fork broke.
    cached_directory_entries: Arc<DirectoryEntryMap>,

    /// When set, a listing on a non-local session is answered from the cache only: a miss is an
    /// empty directory and issues no remote command (#1295). For background consumers that
    /// run per keystroke (autosuggestion, highlighting, classification, next-command), which
    /// must not put a command on the shared ssh connection; an explicit Tab or the
    /// completions-while-typing menu still lists.
    cached_listings_only: bool,

    /// Per-path coalescing gates for `refresh_directory_entries`. `Arc` for the same
    /// load-bearing reason as `cached_directory_entries` above: these are useless unless
    /// every clone of this context shares them.
    refresh_gates: Arc<dashmap::DashMap<TypedPathBuf, Arc<RefreshGate>>>,

    /// Snapshot of all Zap workflow aliases.
    workflow_aliases: HashMap<String, String>,

    /// When set, `generator_context()` is `None`: no completion generator or alias command is
    /// ever run. Used for the input-type classification parse (#1158); fix/1130 also uses it for
    /// highlighting on non-local sessions, where every generator is a command on the shared ssh
    /// connection.
    generators_disabled: bool,
}

impl SessionContext {
    /// A context for highlighting and input-type detection. On a non-local session it runs no
    /// generators (they would be remote commands, per token, per keystroke); highlighting of
    /// generator-only arguments may be slightly less precise there. Local sessions are
    /// returned unchanged.
    pub fn for_decoration(mut self) -> Self {
        if !self.session.is_local() {
            self.generators_disabled = true;
            self.cached_listings_only = true;
        }
        self
    }

    /// A context for background consumers (autosuggestion, next-command prediction): on a
    /// non-local session directory listings come from the cache only, never from a new remote
    /// command (#1295). Local sessions are returned unchanged.
    pub fn for_background(mut self) -> Self {
        if !self.session.is_local() {
            self.cached_listings_only = true;
        }
        self
    }

    /// A context for the input-type classification parse, on every session (#1158). The verdict
    /// Enter may be waiting on must come from local signals only (command word, aliases,
    /// history, syntax): a generator is a process spawn locally and a command on the shared ssh
    /// connection remotely, seconds for `systemctl`, and classification must be fast and
    /// deterministic whichever session the line is typed in.
    pub fn for_classification(mut self) -> Self {
        self.generators_disabled = true;
        if !self.session.is_local() {
            self.cached_listings_only = true;
        }
        self
    }

    /// Lists `directory` fresh from disk and caches the results, coalescing concurrent
    /// refreshes of the same path so only one listing is actually performed.
    ///
    /// The coalescing is not an optimisation; on a legacy-SSH session it is a correctness
    /// concern. This method deliberately does NOT read `cached_directory_entries` -- callers
    /// use it precisely when they must see current contents -- so the shared cache added for
    /// `list_directory_entries` cannot help here. Each call runs
    /// `list_directory_entries_internal`, which on a remote session shells out to `find` over
    /// the session's ControlMaster: one SSH channel per call.
    ///
    /// Measured 2026-08-15 against a remote host, on a build that ALREADY had the shared
    /// cache: three identical `cd <dir> && find -L . -maxdepth 1 ...` in flight at once for a
    /// single directory, part of a peak of seven concurrent channels alongside the completer's
    /// `compgen -c`, a `cat` of the shell history, `git branch`, and
    /// `kubectl config current-context`. `DirectoryFetcher` cancels its OWN previous fetch via
    /// `fetch_handle`, but nothing dedupes across fetchers, so N working-directory chips on one
    /// path meant N concurrent remote commands. Past the host's `MaxSessions` (default 10) sshd
    /// refuses the channel and OpenSSH prints `channel N: open failed` into the user's live
    /// shell.
    ///
    /// Double-checked: snapshot the gate's completion count, queue on its lock, and if the
    /// count moved while waiting then a peer finished a refresh for this exact path in the
    /// meantime -- take its result rather than issue a redundant command. A burst of N callers
    /// therefore costs one listing, and every one of them still observes a listing no older
    /// than its own request.
    /// Deliberately NOT an `async fn`. The completion snapshot has to be taken when the caller
    /// ISSUES the request, not when the returned future is first polled -- an `async fn` body
    /// does not run until first poll, so a caller whose future is created during a burst but
    /// polled after a peer has already finished would snapshot the peer's own result and
    /// re-list anyway. That is precisely the case a burst of context chips produces, and the
    /// coalescing test caught it: with an eager snapshot the burst collapses to one listing,
    /// with a lazy one it does not.
    pub(crate) fn refresh_directory_entries(
        &self,
        directory: TypedPathBuf,
    ) -> impl std::future::Future<Output = Arc<Vec<EngineDirEntry>>> + '_ {
        let gate = self
            .refresh_gates
            .entry(directory.clone())
            .or_insert_with(|| {
                Arc::new(RefreshGate {
                    lock: tokio::sync::Mutex::new(()),
                    completions: AtomicU64::new(0),
                })
            })
            .clone();
        // Eager, at call time -- see the note above.
        let completions_at_request = gate.completions.load(AtomicOrdering::Acquire);

        async move {
            let _guard = gate.lock.lock().await;

            if gate.completions.load(AtomicOrdering::Acquire) != completions_at_request
                && let Some(entries) = self.cached_directory_entries.get(&directory)
            {
                return entries.clone();
            }

            let (entries, cacheable) = self
                .list_directory_entries_internal(&directory.to_path())
                .await;
            let result = Arc::new(entries);
            if cacheable {
                self.cached_directory_entries
                    .insert(directory, result.clone());
            }
            gate.completions.fetch_add(1, AtomicOrdering::Release);
            result
        }
    }

    /// Returns the entries and whether the result may be cached. A timed-out local listing is
    /// transient (the mount may recover), so it is returned empty and NOT cached.
    async fn list_directory_entries_internal(
        &self,
        directory: &TypedPath<'_>,
    ) -> (Vec<EngineDirEntry>, bool) {
        if matches!(self.session.session_type(), SessionType::Local) {
            let dir = match self.session.maybe_convert_to_native_path(directory) {
                Ok(dir) => dir,
                Err(err) => {
                    log::warn!("Failed to convert path: {err:#}");
                    return (Vec::new(), true);
                }
            };
            return match list_local_dir_off_pool(
                dir.as_path().to_path_buf(),
                LOCAL_LISTING_TIMEOUT,
                |dir| read_local_dir_entries(&dir),
            )
            .await
            {
                Some(entries) => (entries, true),
                None => (Vec::new(), false),
            };
        }
        self.list_remote_directory_entries(directory).await
    }

    /// Returns the entries and whether the result may be cached. Only a listing the remote shell
    /// actually produced (success, or a failure with an exit code such as a half-typed path that
    /// does not exist yet) is cacheable. A transport error (the remote-server connection dropped
    /// under a request in flight, a ControlMaster timeout), a killed command and a truncated
    /// listing say nothing about the directory, and caching their empty result made Tab show no
    /// menu for the whole cache window after a proxy restart (#1289).
    async fn list_remote_directory_entries(
        &self,
        directory: &TypedPath<'_>,
    ) -> (Vec<EngineDirEntry>, bool) {
        match self.session.session_type() {
            // Local listings never reach here: `list_directory_entries_internal` routes
            // them through `list_local_directory_entries` (off the shared worker pool).
            SessionType::Local => (Vec::new(), true),
            SessionType::WarpifiedRemote { .. } => {
                let env_vars = self
                    .session
                    .path()
                    .as_deref()
                    .map(|path| HashMap::from_iter([("PATH".to_string(), path.to_string())]));

                let Some(ls_command) = ls_script_for_dir(directory, self.session.shell_dialect())
                else {
                    return (vec![], true);
                };

                // The in-band command executor doesn't support executing from
                // from an arbitrary directory, so we need to cd into the
                // directory we want within the ls script.
                // One retry on a transport error: the session swaps its executor when the
                // remote-server proxy dies or comes back (#1171), so a request caught by the
                // swap fails once and the second attempt runs on the current executor.
                let mut command_output_result = Err(anyhow::anyhow!("not run"));
                for _ in 0..REMOTE_LISTING_ATTEMPTS {
                    command_output_result = self
                        .session
                        .execute_command(
                            &ls_command,
                            None,
                            env_vars.clone(),
                            ExecuteCommandOptions::default(),
                        )
                        .await;
                    if command_output_result.is_ok() {
                        break;
                    }
                }

                match command_output_result {
                    Ok(command_output) => match command_output.status {
                        CommandExitStatus::Success => {
                            match parse_ls_script_output(command_output.output()) {
                                Some(entries) => (entries, true),
                                None => {
                                    log::warn!(
                                        "Executing `ls` on remote box returned malformed or truncated output: `{:?}`",
                                        AsciiDebug(command_output.output())
                                    );
                                    note_remote_listing_failed_at(
                                        self.session.id(),
                                        directory.to_path_buf(),
                                        Instant::now(),
                                    );
                                    (vec![], false)
                                }
                            }
                        }
                        CommandExitStatus::Failure => {
                            safe_warn!(
                                safe: ("Executing `ls` on remote box failed with non-zero status code {:?}.", command_output.exit_code),
                                full: ("Executing `ls` on remote box failed with error: {}", &String::from_utf8_lossy(command_output.output()))
                            );
                            // An exit code is the shell's own verdict on this directory; no
                            // exit code means the command was killed from outside. 255 is ssh's
                            // own failure (no usable master), which says nothing about the
                            // directory.
                            let shell_verdict = command_output
                                .exit_code
                                .is_some_and(|code| code.value() != SSH_TRANSPORT_EXIT_CODE);
                            if shell_verdict {
                                note_remote_listing_failed_at(
                                    self.session.id(),
                                    directory.to_path_buf(),
                                    Instant::now(),
                                );
                            }
                            (vec![], shell_verdict)
                        }
                    },
                    Err(err) => {
                        log::warn!("Executing `ls` on remote box failed with error {err:?}");
                        (vec![], false)
                    }
                }
            }
        }
    }
}

#[async_trait]
impl PathCompletionContext for SessionContext {
    fn home_directory(&self) -> Option<&str> {
        self.session.home_dir()
    }

    fn pwd(&self) -> TypedPath<'_> {
        self.current_working_directory.to_path()
    }

    fn shell_family(&self) -> ShellFamily {
        self.session.shell_family()
    }

    fn shell_dialect(&self) -> ShellDialect {
        self.session.shell_dialect()
    }

    async fn list_directory_entries(&self, directory: TypedPathBuf) -> Arc<Vec<EngineDirEntry>> {
        if let Some(entries) = self.cached_directory_entries.get(&directory) {
            return entries.clone();
        }

        // When the remote server feature is enabled but the session hasn't
        // connected yet, the command executor may be an InBand fallback that
        // sends escape sequences to a raw remote shell. Return empty without
        // caching so we retry after the remote server handshake finishes.
        //
        // Not for a bootstrapped subshell (#1239): a root shell Phosphorized inside an ssh
        // session is `WarpifiedRemote` too, and its in-band executor is its final one, so
        // waiting for a handshake that is not coming withheld every path completion from it.
        if let SessionType::WarpifiedRemote { host_id: None } = self.session.session_type() {
            if FeatureFlag::SshRemoteServer.is_enabled()
                && !self.session.is_legacy_ssh_session()
                && self.session.subshell_info().is_none()
            {
                return Arc::new(vec![]);
            }
        }

        if !self.session.is_local() {
            // A per-keystroke consumer never costs a remote command (#1295).
            if self.cached_listings_only {
                return Arc::new(vec![]);
            }
            // A directory the shell already said it cannot list is not asked again for a while.
            if remote_listing_failed_recently_at(self.session.id(), &directory, Instant::now()) {
                return Arc::new(vec![]);
            }
        }

        let (entries, cacheable) = self
            .list_directory_entries_internal(&directory.to_path())
            .await;

        let result = Arc::new(entries);
        if cacheable {
            self.cached_directory_entries
                .insert(directory, result.clone());
        }
        result
    }

    fn path_separators(&self) -> PathSeparators {
        self.session.path_separators()
    }
}

impl SessionContext {
    /// The environment a generator runs with. `PATH` is set explicitly even with session env vars:
    /// if the user opened Zap through a parent process that didn't have the PATH var set (i.e.
    /// outside of a shell, for example opening the app via Finder), the subshell won't inherit
    /// the PATH var, but we need it to reference executables we might run as part of generators.
    fn generator_env_vars(
        &self,
        session_env_vars: Option<HashMap<String, String>>,
    ) -> Option<HashMap<String, String>> {
        let mut env_vars = session_env_vars.unwrap_or_default();
        if let Some(path) = self.session.path().as_deref() {
            env_vars.insert("PATH".to_string(), path.to_string());
        }

        if env_vars.is_empty() {
            None
        } else {
            Some(env_vars)
        }
    }

    fn generator_execute_options() -> ExecuteCommandOptions {
        ExecuteCommandOptions {
            run_command_in_same_shell_as_session: !FeatureFlag::RunGeneratorsWithCmdExe
                .is_enabled(),
        }
    }

    /// Starts `script` on the runtime as its own task, so a caller that is aborted (the next
    /// keystroke) does not discard a run the next caller can use, and returns a future every
    /// interested caller can wait on. The task stores its result in the generator cache.
    fn start_generator_run(
        &self,
        handle: &tokio::runtime::Handle,
        key: generator_cache::GeneratorKey,
        run_id: u64,
        env_vars: Option<HashMap<String, String>>,
    ) -> generator_cache::SharedGeneratorRun {
        let session = self.session.clone();
        let pwd = self.pwd().to_str().map(str::to_owned);
        let script = key.script.clone();
        let finish_key = key.clone();
        let run = async move {
            let outcome = tokio::time::timeout(
                generator_cache::GENERATOR_RUN_TIMEOUT,
                session.execute_command(
                    &script,
                    pwd.as_deref(),
                    env_vars,
                    Self::generator_execute_options(),
                ),
            )
            .await;
            let result: generator_cache::GeneratorRunResult = Arc::new(match outcome {
                Ok(Ok(output)) => Ok(output),
                Ok(Err(err)) => Err(format!("{err:#}")),
                Err(_) => Err(format!(
                    "generator timed out after {:?}",
                    generator_cache::GENERATOR_RUN_TIMEOUT
                )),
            });
            generator_cache::GENERATOR_CACHE
                .lock()
                .finish(&key, run_id, &result, Instant::now());
            result
        };
        if !self.session.is_local() {
            // A remote generator is a command on the shared ssh connection. It is polled only by
            // its waiters and aborted when the last one is dropped (#1128); never detach it.
            return run.boxed().shared();
        }
        let join = handle.spawn(run);
        async move {
            match join.await {
                Ok(result) => result,
                Err(err) => {
                    // The task panicked or was cancelled with the runtime: free the slot.
                    let result: generator_cache::GeneratorRunResult =
                        Arc::new(Err(format!("generator task failed: {err}")));
                    generator_cache::GENERATOR_CACHE.lock().finish(
                        &finish_key,
                        run_id,
                        &result,
                        Instant::now(),
                    );
                    result
                }
            }
        }
        .boxed()
        .shared()
    }

    /// One shared generator run for `key`: a fresh cached result, the run already in flight, or
    /// a new run.
    async fn shared_generator_run(
        &self,
        handle: &tokio::runtime::Handle,
        key: &generator_cache::GeneratorKey,
        env_vars: &Option<HashMap<String, String>>,
    ) -> generator_cache::GeneratorRunResult {
        let detached = self.session.is_local();
        let outcome = generator_cache::GENERATOR_CACHE.lock().get_or_start_with(
            key,
            Instant::now(),
            detached,
            |run_id| self.start_generator_run(handle, key.clone(), run_id, env_vars.clone()),
        );
        match outcome {
            Ok(result) => result,
            Err(run) => run.await,
        }
    }
}

#[async_trait]
impl GeneratorContext for SessionContext {
    async fn execute_command_at_pwd(
        &self,
        shell_command: &str,
        session_env_vars: Option<HashMap<String, String>>,
    ) -> Result<CommandOutput> {
        let env_vars_option = self.generator_env_vars(session_env_vars);

        self.session
            .execute_command(
                shell_command,
                self.pwd().to_str(),
                env_vars_option,
                Self::generator_execute_options(),
            )
            .await
    }

    async fn execute_generator_at_pwd(
        &self,
        shell_command: &str,
        session_env_vars: Option<HashMap<String, String>>,
    ) -> Result<CommandOutput> {
        // Outside a tokio runtime (some unit-test executors) there is no task to detach.
        let Ok(handle) = tokio::runtime::Handle::try_current() else {
            return self
                .execute_command_at_pwd(shell_command, session_env_vars)
                .await;
        };
        let env_vars = self.generator_env_vars(session_env_vars);
        let key = generator_cache::GeneratorKey {
            session_id: self.session.id(),
            pwd: self.pwd().to_string_lossy().into_owned(),
            script: shell_command.to_owned(),
        };

        let mut result = self.shared_generator_run(&handle, &key, &env_vars).await;
        if generator_cache::is_cancelled_run(&result) {
            // The run was killed for a keystroke that is gone (`cancel_active_commands` kills
            // every generator of the session); this caller is still waiting, so run it again.
            result = self.shared_generator_run(&handle, &key, &env_vars).await;
        }
        match result.as_ref() {
            Ok(output) => Ok(output.clone()),
            Err(message) => Err(anyhow::anyhow!("{message}")),
        }
    }

    fn supports_parallel_execution(&self) -> bool {
        self.session.supports_parallel_command_execution()
    }
}

impl CompletionContext for SessionContext {
    fn generator_context(&self) -> Option<&dyn GeneratorContext> {
        if self.generators_disabled {
            None
        } else {
            Some(self)
        }
    }

    fn path_completion_context(&self) -> Option<&dyn PathCompletionContext> {
        Some(self)
    }

    fn top_level_commands(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(
            self.session
                .top_level_commands()
                .chain(self.workflow_aliases.keys().map(String::as_str)),
        )
    }

    fn command_case_sensitivity(&self) -> TopLevelCommandCaseSensitivity {
        self.session.command_case_sensitivity()
    }

    fn escape_char(&self) -> EscapeChar {
        self.session.shell_family().escape_char()
    }

    fn aliases(&self) -> Box<dyn Iterator<Item = (&str, &str)> + '_> {
        let session_aliases = self
            .session
            .aliases()
            .iter()
            .map(|(alias, command)| (alias.as_str(), command.as_str()));
        let workflow_aliases = self
            .workflow_aliases
            .iter()
            .map(|(alias, command)| (alias.as_str(), command.as_str()));
        Box::new(workflow_aliases.chain(session_aliases))
    }

    fn alias_command(&self, alias: &str) -> Option<&str> {
        self.workflow_aliases
            .get(alias)
            .or_else(|| self.session.aliases().get(alias))
            .map(Deref::deref)
    }

    fn abbreviations(&self) -> Option<&HashMap<SmolStr, String>> {
        Some(self.session.abbreviations())
    }

    fn functions(&self) -> Option<&HashSet<SmolStr>> {
        Some(self.session.functions())
    }

    fn builtins(&self) -> Option<&HashSet<SmolStr>> {
        Some(self.session.builtins())
    }

    fn command_registry(&self) -> &CommandRegistry {
        &self.command_registry
    }

    fn environment_variable_names(&self) -> Option<&HashSet<SmolStr>> {
        Some(self.session.environment_variable_names())
    }

    fn shell_supports_autocd(&self) -> Option<bool> {
        Some(self.session.shell().supports_autocd())
    }

    #[cfg(feature = "completions_v2")]
    fn js_context(&self) -> Option<&dyn warp_completer::completer::JsExecutionContext> {
        self.js_ctx
            .as_ref()
            .map(|ctx| -> &dyn warp_completer::completer::JsExecutionContext { ctx })
    }

    fn shell_family(&self) -> Option<ShellFamily> {
        Some(self.session.shell_family())
    }

    fn top_level_commands_fully_loaded(&self) -> bool {
        self.session.has_finished_loading_external_commands()
    }

    fn top_level_command_data_is_current(&self) -> bool {
        self.session.command_data_is_current()
    }
}

impl SessionContext {
    pub fn new(
        session: impl Into<Arc<Session>>,
        command_registry: Arc<CommandRegistry>,
        current_working_directory: TypedPathBuf,
        #[allow(unused_variables)] ctx: &AppContext,
    ) -> Self {
        let workflow_aliases = if FeatureFlag::WorkflowAliases.is_enabled() {
            WorkflowAliases::as_ref(ctx).autocomplete_data(ctx)
        } else {
            Default::default()
        };

        let session: Arc<Session> = session.into();
        // Non-local listings cost a remote command, so they outlive this context (per cwd).
        let cached_directory_entries = if session.is_local() {
            Arc::new(Default::default())
        } else {
            shared_directory_cache_at(session.id(), &current_working_directory, Instant::now())
        };

        cfg_if::cfg_if! {
            if #[cfg(feature = "completions_v2")] {
                use crate::plugin::{PluginHost, service::CallJsFunctionService};

                let js_function_caller = PluginHost::handle(ctx)
                    .as_ref(ctx)
                    .plugin_service_caller::<CallJsFunctionService>();
                Self {
                    session,
                    command_registry,
                    current_working_directory,
                    js_ctx: js_function_caller.map(js::SessionJsExecutionContext::new),
                    cached_directory_entries,
                    cached_listings_only: false,
                    refresh_gates: Arc::new(Default::default()),
                    workflow_aliases,
                    generators_disabled: false,
                }
            } else {
                Self {
                    session,
                    command_registry,
                    current_working_directory,
                    cached_directory_entries,
                    cached_listings_only: false,
                    refresh_gates: Arc::new(Default::default()),
                    workflow_aliases,
                    generators_disabled: false,
                }
            }
        }
    }
}

/// `CompletionContext` implementation for "global" completions, that provide completions on all
/// commands in the `command_registry` rather than providing session-specific completions.
///
/// This `CompletionContext` is not coupled to a specific session and thus does not provide path or
/// generator execution, which wouldn't have clear semantics without being coupled to a session.
#[derive(Clone)]
pub struct SessionAgnosticContext {
    command_registry: Arc<CommandRegistry>,
}

impl SessionAgnosticContext {
    pub fn new(command_registry: Arc<CommandRegistry>) -> Self {
        Self { command_registry }
    }
}

impl CompletionContext for SessionAgnosticContext {
    fn top_level_commands(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(self.command_registry.registered_commands())
    }

    fn command_registry(&self) -> &CommandRegistry {
        &self.command_registry
    }

    fn environment_variable_names(&self) -> Option<&HashSet<SmolStr>> {
        None
    }

    fn shell_supports_autocd(&self) -> Option<bool> {
        None
    }

    fn path_completion_context(&self) -> Option<&dyn PathCompletionContext> {
        None
    }

    fn generator_context(&self) -> Option<&dyn GeneratorContext> {
        None
    }
}

/// Empty `CompletionContext` used in places without a live shell session
/// (i.e. shared session viewers without a real terminal instance).
#[derive(Clone)]
pub struct EmptyCompletionContext;
impl EmptyCompletionContext {
    pub fn new() -> Self {
        Self
    }
}
impl CompletionContext for EmptyCompletionContext {
    fn top_level_commands(&self) -> Box<dyn Iterator<Item = &str> + '_> {
        Box::new(std::iter::empty())
    }

    fn command_registry(&self) -> &CommandRegistry {
        &EMPTY_COMMAND_REGISTRY
    }

    fn environment_variable_names(&self) -> Option<&HashSet<SmolStr>> {
        None
    }

    fn shell_supports_autocd(&self) -> Option<bool> {
        None
    }

    fn path_completion_context(&self) -> Option<&dyn PathCompletionContext> {
        None
    }

    fn generator_context(&self) -> Option<&dyn GeneratorContext> {
        None
    }

    fn top_level_commands_fully_loaded(&self) -> bool {
        // There is no session here to ever finish loading anything -- unlike a real session
        // between bootstrap and its `$PATH` scan landing, this context will never gain more
        // command knowledge, but callers that need to tell "definitely not a command" apart
        // from "can't tell" (e.g. the Agent-input safety gate, #696) must treat this the same
        // as an in-flight probe: absence from `top_level_commands()` here proves nothing.
        false
    }
}

/// List files and directories in a directory; used for completions on a remote machine.
/// This uses `find` instead of `ls` due to the challenges of parsing `ls` output for
/// unusual file names (e.g.: ones including newlines).
/// We intentionally ignore '.' and '..' here as we add those suggestions manually.
///
/// The script is written to the session's PTY by the in-band executor, so the directory is
/// escaped for the session's exact shell: a raw control character in it would be acted on by
/// the line editor (#867).
fn ls_script_for_dir(directory: &TypedPath, dialect: ShellDialect) -> Option<String> {
    // We need to cd into the directory we want completions for
    let Some(dir_str) = directory.to_str() else {
        log::warn!("Non-unicode character found in path: `{directory:?}`");
        return None;
    };
    let escaped_dir = dialect.shell_escape(dir_str);

    // Get all directories with -print0, which makes all items end in `\0` (null character)
    // Get all files with -print0, which makes all items end in `\0`
    // Separate the two lists with `\0`
    // Ex: `a\0b\0\c\0\0d.txt\0e.txt\0f.txt\0`
    // Then do the same for anything that is not a directory, and call it a 'File'.
    //
    // Follow symlinks when classifying entries, so a symlink to a directory completes as a
    // directory (like a standard terminal). Without `-L`, `find` does not dereference when
    // evaluating `-type`, so a symlink pointing at a directory fails `-type d`, lands in the
    // *file* bucket, and is offered without a trailing separator and excluded from `cd`
    // completions. Local sessions were never affected — they go through `std::fs::read_dir`,
    // which resolves the link. Ported from upstream `1b65a8b9` (#14746).
    let command = format!(
        r#"
cd {escaped_dir} && 
find -L . -maxdepth 1 -type d -print0 &&
printf '%b' '\0' &&
find -L . -maxdepth 1 -not -type d -print0
            "#
    )
    // Ensure all newlines are escaped, and that the command is a single line.
    // ls_script_for_dir should not contain newlines, as we need to run it as a
    // single line for TMUX control mode at this time.
    .replace("\n", " ");

    Some(command)
}

/// Parses the null-delimited output of the script `ls_script_for_dir` builds into directory and
/// file entries, or `None` if the output doesn't match that script's guaranteed wire format --
/// a dirs list, a `\0` separator, and a files list, with every entry (including the separator
/// and, when the files list is non-empty, its own entries) `\0`-terminated.
fn parse_ls_script_output(output: &[u8]) -> Option<Vec<EngineDirEntry>> {
    let mut entries = Vec::new();
    let mut segments = output.split(|&byte| byte == b'\0');

    let dirs = segments
        .by_ref()
        // We use two consecutive null characters to separate files and folders, so detect that
        // here. Note that take_while consumes the first entry that returns false -- the
        // dirs/files separator itself.
        .take_while(|segment| !segment.is_empty())
        .filter_map(|segment| dir_entry_from_segment(segment, EngineFileType::Directory));
    entries.extend(dirs);

    // What's left after the separator is the files list followed by its own trailing `\0`,
    // which `find -print0` guarantees on every complete pass, empty or not (an empty pass
    // contributes no bytes of its own, but the separator's `\0` is still the final byte). If
    // nothing is left at all, the separator was never reached: the output was truncated before
    // the second `find` pass ran, or was empty to begin with. If the last remaining segment
    // isn't empty, the files list itself was cut off mid-entry. Either way, this isn't a real,
    // complete listing.
    let remaining: Vec<&[u8]> = segments.collect();
    let (last, files) = remaining.split_last()?;
    if !last.is_empty() {
        return None;
    }

    entries.extend(
        files
            .iter()
            .filter_map(|segment| dir_entry_from_segment(segment, EngineFileType::File)),
    );

    Some(entries)
}

/// Converts one `\0`-delimited `find -print0` segment into an entry, or `None` if it's the `.`
/// entry `find` itself emits, or the segment isn't valid UTF-8.
fn dir_entry_from_segment(segment: &[u8], file_type: EngineFileType) -> Option<EngineDirEntry> {
    let path_str = std::str::from_utf8(segment).ok()?;
    if path_str == "." {
        return None;
    }
    let file_name = Path::new(path_str).file_name()?.to_str()?.to_owned();
    Some(EngineDirEntry {
        file_name,
        file_type,
    })
}

#[cfg(test)]
#[path = "test.rs"]
mod tests;
