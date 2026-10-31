use std::cell::Cell;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures::FutureExt;
use warp_completer::completer::{CommandExitStatus, CommandOutput};
use warp_core::command::ExitCode;

use super::*;

fn key(session: u64, pwd: &str, script: &str) -> GeneratorKey {
    GeneratorKey {
        session_id: SessionId::from(session),
        pwd: pwd.to_owned(),
        script: script.to_owned(),
    }
}

fn output(status: CommandExitStatus, exit_code: Option<i32>, stdout: &str) -> GeneratorRunResult {
    Arc::new(Ok(CommandOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        status,
        exit_code: exit_code.map(ExitCode::from),
    }))
}

fn ok(stdout: &str) -> GeneratorRunResult {
    output(CommandExitStatus::Success, Some(0), stdout)
}

fn ready_run(result: GeneratorRunResult) -> SharedGeneratorRun {
    futures::future::ready(result).boxed().shared()
}

fn stdout_of(result: &GeneratorRunResult) -> String {
    match result.as_ref() {
        Ok(output) => String::from_utf8(output.stdout.clone()).unwrap(),
        Err(message) => panic!("unexpected error result: {message}"),
    }
}

/// The first caller starts a run; a caller arriving while it is in flight joins it instead of
/// starting a second one. This is what stops Tab, the autosuggestion and highlighting from each
/// spawning `systemctl list-units` for one keystroke.
#[test]
fn concurrent_callers_share_one_run() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "git branch");
    let starts = Cell::new(0);

    let first = cache.get_or_start(&k, now, |_| {
        starts.set(starts.get() + 1);
        ready_run(ok("main\n"))
    });
    assert!(first.is_err(), "the first caller must get a run to await");

    let second = cache.get_or_start(&k, now, |_| {
        starts.set(starts.get() + 1);
        ready_run(ok("other\n"))
    });
    assert!(second.is_err(), "the second caller joins the run in flight");
    assert_eq!(starts.get(), 1, "only one run may be started for one key");

    let joined = futures::executor::block_on(second.err().unwrap());
    assert_eq!(stdout_of(&joined), "main\n");
}

/// A finished result is reused inside the TTL: the second Tab, the ghost-text revalidation after
/// an inserted completion and the next highlight pass all skip the process.
#[test]
fn finished_result_is_reused_within_ttl() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "git branch");
    let starts = Cell::new(0);

    let run = cache
        .get_or_start(&k, now, |_| {
            starts.set(starts.get() + 1);
            ready_run(ok("main\n"))
        })
        .err()
        .unwrap();
    let result = futures::executor::block_on(run);
    cache.finish(&k, 0, &result, now);

    let later = now + GENERATOR_CACHE_TTL - Duration::from_millis(1);
    let hit = cache.get_or_start(&k, later, |_| {
        starts.set(starts.get() + 1);
        ready_run(ok("fresh\n"))
    });
    assert_eq!(
        stdout_of(&hit.ok().expect("a hit inside the TTL")),
        "main\n"
    );
    assert_eq!(starts.get(), 1);
}

#[test]
fn finished_result_expires_after_ttl() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "git branch");
    let starts = Cell::new(0);

    let run = cache
        .get_or_start(&k, now, |_| {
            starts.set(starts.get() + 1);
            ready_run(ok("main\n"))
        })
        .err()
        .unwrap();
    let result = futures::executor::block_on(run);
    cache.finish(&k, 0, &result, now);

    let expired = now + GENERATOR_CACHE_TTL;
    let again = cache.get_or_start(&k, expired, |_| {
        starts.set(starts.get() + 1);
        ready_run(ok("new-branch\n"))
    });
    assert!(again.is_err(), "an expired result must start a new run");
    assert_eq!(starts.get(), 2);
}

/// A run killed by `cancel_active_commands` has no exit code. It must reach its waiters (who
/// retry) but never be stored, or every Tab for the next five seconds would see an empty list.
#[test]
fn killed_run_is_not_cached() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "systemctl list-units");

    let killed = output(CommandExitStatus::Failure, None, "");
    assert!(is_cancelled_run(&killed));
    assert!(
        cache
            .get_or_start(&k, now, |_| ready_run(killed.clone()))
            .is_err()
    );
    cache.finish(&k, 0, &killed, now);

    let starts = Cell::new(0);
    let next = cache.get_or_start(&k, now, |_| {
        starts.set(starts.get() + 1);
        ready_run(ok("ssh.service\n"))
    });
    assert!(next.is_err());
    assert_eq!(
        starts.get(),
        1,
        "the killed run must not be served from the cache"
    );
}

/// An error (spawn failure, timeout) is not stored either.
#[test]
fn error_result_is_not_cached() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "docker ps");
    let failed: GeneratorRunResult = Arc::new(Err("generator timed out".to_owned()));
    assert!(!is_cancelled_run(&failed));

    assert!(
        cache
            .get_or_start(&k, now, |_| ready_run(failed.clone()))
            .is_err()
    );
    cache.finish(&k, 0, &failed, now);
    assert!(
        cache
            .get_or_start(&k, now, |_| ready_run(ok("web\n")))
            .is_err()
    );
}

/// `git config --get alias.checkout` exits 1 when the alias is not set. That is a real,
/// repeatable answer and is what removes a serial process spawn from every `git <sub><Tab>`.
#[test]
fn failure_with_exit_code_is_cached() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "git config --get alias.checkout");
    let no_alias = output(CommandExitStatus::Failure, Some(1), "");
    assert!(!is_cancelled_run(&no_alias));

    assert!(
        cache
            .get_or_start(&k, now, |_| ready_run(no_alias.clone()))
            .is_err()
    );
    cache.finish(&k, 0, &no_alias, now);
    assert!(
        cache
            .get_or_start(&k, now, |_| panic!("must be served from the cache"))
            .is_ok()
    );
}

/// Submitting a command or finishing a block drops the session's slots: the command may have
/// created the branch the next Tab should offer. Another session is untouched.
#[test]
fn invalidate_session_drops_only_that_session() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let mine = key(1, "/repo", "git branch");
    let other = key(2, "/repo", "git branch");
    for (run_id, k) in [&mine, &other].into_iter().enumerate() {
        let result = ok("main\n");
        assert!(
            cache
                .get_or_start(k, now, |_| ready_run(result.clone()))
                .is_err()
        );
        cache.finish(k, run_id as u64, &result, now);
    }

    cache.invalidate_session(mine.session_id);

    assert!(
        cache
            .get_or_start(&mine, now, |_| ready_run(ok("new\n")))
            .is_err()
    );
    assert!(
        cache
            .get_or_start(&other, now, |_| panic!("other session is kept"))
            .is_ok()
    );
}

/// A run that started before an invalidation finishes afterwards: its waiters get the result
/// but it must not repopulate the cache with a pre-command listing.
#[test]
fn run_finishing_after_invalidation_is_not_stored() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", "git branch");
    let stale = ok("main\n");

    let run_id = Cell::new(u64::MAX);
    assert!(
        cache
            .get_or_start(&k, now, |id| {
                run_id.set(id);
                ready_run(stale.clone())
            })
            .is_err()
    );
    cache.invalidate_session(k.session_id);
    cache.finish(&k, run_id.get(), &stale, now);

    assert!(
        cache
            .get_or_start(&k, now, |_| ready_run(ok("main\nnew\n")))
            .is_err()
    );
}

/// The pwd is part of the key: `git branch` in two repos must not share a result.
#[test]
fn different_pwd_is_a_different_key() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let a = key(1, "/repo-a", "git branch");
    let b = key(1, "/repo-b", "git branch");
    let result = ok("main\n");
    assert!(
        cache
            .get_or_start(&a, now, |_| ready_run(result.clone()))
            .is_err()
    );
    cache.finish(&a, 0, &result, now);

    assert!(
        cache
            .get_or_start(&b, now, |_| ready_run(ok("dev\n")))
            .is_err()
    );
    assert!(
        cache
            .get_or_start(&a, now, |_| panic!("a is cached"))
            .is_ok()
    );
}

/// A burst of distinct scripts cannot grow the map without bound once their results are done.
#[test]
fn slot_count_is_bounded() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let result = ok("x\n");
    for i in 0..(GENERATOR_CACHE_MAX_SLOTS * 3) {
        let k = key(1, "/repo", &format!("gen {i}"));
        assert!(
            cache
                .get_or_start(&k, now, |_| ready_run(result.clone()))
                .is_err()
        );
        cache.finish(&k, i as u64, &result, now);
    }
    assert!(cache.slots.len() <= GENERATOR_CACHE_MAX_SLOTS);
}
