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
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::future::{BoxFuture, Shared, WeakShared};
use lazy_static::lazy_static;
use warp_completer::completer::{CommandExitStatus, CommandOutput};

use crate::terminal::model::session::SessionId;

/// How long a finished generator result is reused.
pub(super) const GENERATOR_CACHE_TTL: Duration = Duration::from_secs(5);

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

enum Slot {
    InFlight {
        run_id: u64,
        run: RunHandle,
    },
    Done {
        at: Instant,
        result: GeneratorRunResult,
    },
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
        self.sweep(now);
        match self.slots.get(key) {
            Some(Slot::Done { at, result })
                if now.saturating_duration_since(*at) < GENERATOR_CACHE_TTL =>
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
        let is_this_run = matches!(
            self.slots.get(key),
            Some(Slot::InFlight { run_id: id, .. }) if *id == run_id
        );
        if !is_this_run {
            return;
        }
        if is_cacheable(result) {
            self.slots.insert(
                key.clone(),
                Slot::Done {
                    at: now,
                    result: result.clone(),
                },
            );
        } else {
            self.slots.remove(key);
        }
    }

    pub(super) fn invalidate_session(&mut self, session_id: SessionId) {
        self.slots.retain(|key, _| key.session_id != session_id);
    }

    fn sweep(&mut self, now: Instant) {
        self.slots.retain(|_, slot| match slot {
            Slot::Done { at, .. } => now.saturating_duration_since(*at) < GENERATOR_CACHE_TTL,
            Slot::InFlight { .. } => true,
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
