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

/// #1128: a non-local run is not kept alive by the cache. When the last waiter goes away the
/// run is dropped (which aborts the remote command) and the next caller starts a new one; a
/// local run in the same position keeps going.
#[test]
fn attached_run_is_dropped_when_no_caller_waits() {
    struct DropFlag(Arc<std::sync::atomic::AtomicBool>);
    impl Drop for DropFlag {
        fn drop(&mut self) {
            self.0.store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let make = |dropped: Arc<std::sync::atomic::AtomicBool>| -> SharedGeneratorRun {
        let flag = DropFlag(dropped);
        async move {
            let _flag = flag;
            futures::future::pending::<()>().await;
            ok("never\n")
        }
        .boxed()
        .shared()
    };

    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/home/me", "systemctl list-units");

    let remote_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let waiter = cache
        .get_or_start_with(&k, now, false, |_| make(remote_dropped.clone()))
        .err()
        .unwrap();
    // A second caller joins while the first waits, and keeps it alive.
    let joiner = cache
        .get_or_start_with(&k, now, false, |_| panic!("must join the live run"))
        .err()
        .unwrap();
    drop(waiter);
    assert!(!remote_dropped.load(std::sync::atomic::Ordering::SeqCst));
    drop(joiner);
    assert!(
        remote_dropped.load(std::sync::atomic::Ordering::SeqCst),
        "a remote run with no waiter must be aborted"
    );
    let restarted = Cell::new(false);
    let fresh = cache.get_or_start_with(&k, now, false, |_| {
        restarted.set(true);
        ready_run(ok("ssh.service\n"))
    });
    assert!(fresh.is_err());
    assert!(restarted.get(), "a dead remote run must not be joined");

    let local_dropped = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let k2 = key(2, "/home/me", "git branch");
    let local = cache
        .get_or_start_with(&k2, now, true, |_| make(local_dropped.clone()))
        .err()
        .unwrap();
    drop(local);
    assert!(
        !local_dropped.load(std::sync::atomic::Ordering::SeqCst),
        "a local run stays alive for later callers"
    );
}

/// An attached (remote) run whose last waiter dropped is dead: nothing polls it and nothing
/// will call `finish`, so the sweep drops its slot instead of keeping it until the session ends.
#[test]
fn sweep_drops_an_in_flight_slot_whose_run_is_gone() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let dead = key(1, "/repo", "git branch");
    let alive = key(1, "/repo", "git tag");
    let waited_on = ready_run(ok("v1"));
    let weak_alive = waited_on
        .downgrade()
        .expect("an unpolled shared run can be downgraded");
    let gone = ready_run(ok("v2"));
    let weak_gone = gone
        .downgrade()
        .expect("an unpolled shared run can be downgraded");
    drop(gone);
    cache.slots.insert(
        dead.clone(),
        Slot::InFlight {
            run_id: 1,
            run: RunHandle::Attached(weak_gone),
            stamp: None,
        },
    );
    cache.slots.insert(
        alive.clone(),
        Slot::InFlight {
            run_id: 2,
            run: RunHandle::Attached(weak_alive),
            stamp: None,
        },
    );
    cache.sweep(now);
    assert!(
        !cache.slots.contains_key(&dead),
        "the dead run's slot is swept"
    );
    assert!(
        cache.slots.contains_key(&alive),
        "a run somebody waits on stays"
    );
    drop(waited_on);
}

fn stamp(len: u64) -> ConfigStamp {
    ConfigStamp::from_entries(vec![(
        PathBuf::from("/home/u/.gitconfig"),
        Some((SystemTime::UNIX_EPOCH, len)),
    )])
}

const ALIAS_SCRIPT: &str = "git --no-optional-locks config --get-regexp '^alias.'";

/// #1342: the alias lookup is recognised, ordinary generators are not.
#[test]
fn alias_lookup_scripts_are_recognised() {
    assert!(is_alias_lookup_script(ALIAS_SCRIPT));
    assert!(is_alias_lookup_script("git config --get alias.checkout"));
    assert!(!is_alias_lookup_script("git branch --no-color"));
    assert!(!is_alias_lookup_script("git config --get-regexp '.*'"));
}

/// #1342: a stamped alias lookup survives command submission and is reused past the ordinary
/// TTL; on old code the submit dropped it and every `git commit` spawned the lookup again.
#[test]
fn stamped_alias_lookup_survives_invalidation_and_ttl() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", ALIAS_SCRIPT);
    let starts = Cell::new(0);
    let run = |c: &mut GeneratorCache, at: Instant| {
        c.get_or_start_stamped(&k, at, true, Some(stamp(5)), |id| {
            starts.set(starts.get() + 1);
            let r = ready_run(ok("alias.co checkout\n"));
            let _ = id;
            r
        })
    };
    let first = run(&mut cache, now);
    let res = first.err().expect("first call starts a run");
    let result = futures::executor::block_on(res);
    cache.finish(&k, 0, &result, now);

    cache.invalidate_session(SessionId::from(1));
    let later = now + GENERATOR_CACHE_TTL * 4;
    let second = run(&mut cache, later);
    assert!(second.is_ok(), "reused after submit and past the 5 s TTL");
    assert_eq!(starts.get(), 1);
}

/// #1342: a changed config file (`git config alias.x y`, an edited .gitconfig) is a miss, so a
/// real change is never answered with the old aliases.
#[test]
fn changed_config_stamp_forces_a_new_alias_lookup() {
    let mut cache = GeneratorCache::default();
    let now = Instant::now();
    let k = key(1, "/repo", ALIAS_SCRIPT);
    let first = cache.get_or_start_stamped(&k, now, true, Some(stamp(5)), |_| {
        ready_run(ok("alias.co checkout\n"))
    });
    let result = futures::executor::block_on(first.err().unwrap());
    cache.finish(&k, 0, &result, now);

    let second = cache.get_or_start_stamped(&k, now, true, Some(stamp(9)), |_| {
        ready_run(ok("alias.co checkout\nalias.st status\n"))
    });
    assert!(second.is_err(), "a changed stamp must start a new run");
}

/// #1342: the stamp reflects the file on disk.
#[test]
fn config_stamp_changes_when_gitconfig_changes() {
    let dir = std::env::temp_dir().join(format!("phosphor-stamp-{}", std::process::id()));
    let git = dir.join("repo").join(".git");
    std::fs::create_dir_all(&git).unwrap();
    let cfg = git.join("config");
    std::fs::write(&cfg, "[alias]\n co = checkout\n").unwrap();
    let pwd = dir.join("repo").to_string_lossy().into_owned();
    let before = config_stamp(&pwd);
    std::fs::write(&cfg, "[alias]\n co = checkout\n st = status\n").unwrap();
    let after = config_stamp(&pwd);
    std::fs::remove_dir_all(&dir).ok();
    assert_ne!(before, after);
}
