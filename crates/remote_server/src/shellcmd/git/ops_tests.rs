//! Tests for the git algorithms against a scripted [`GitRunner`].

use std::path::Path;
use std::sync::Mutex;

use anyhow::Result;
use futures::future::BoxFuture;

use super::*;

type Responder = Box<dyn Fn(&[String], Option<&[u8]>) -> Option<GitOutput> + Send + Sync>;

/// Records every call; `respond` answers by argv (None = exit 128 "unscripted").
struct Fake {
    calls: Mutex<Vec<String>>,
    respond: Responder,
}

fn ok(stdout: &str) -> Option<GitOutput> {
    Some(GitOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: Vec::new(),
        exit_code: Some(0),
    })
}

fn fail(stderr: &str) -> Option<GitOutput> {
    Some(GitOutput {
        stdout: Vec::new(),
        stderr: stderr.as_bytes().to_vec(),
        exit_code: Some(128),
    })
}

impl Fake {
    fn new(
        respond: impl Fn(&[String], Option<&[u8]>) -> Option<GitOutput> + Send + Sync + 'static,
    ) -> Self {
        Self {
            calls: Mutex::new(Vec::new()),
            respond: Box::new(respond),
        }
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn answer(&self, prefix: &str, args: &[&str], stdin: Option<&[u8]>) -> GitOutput {
        let owned: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        self.calls
            .lock()
            .unwrap()
            .push(format!("{prefix}{}", owned.join(" ")));
        (self.respond)(&owned, stdin).unwrap_or(GitOutput {
            stdout: Vec::new(),
            stderr: b"unscripted".to_vec(),
            exit_code: Some(128),
        })
    }
}

impl GitRunner for Fake {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        _cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move { Ok(self.answer("", args, stdin)) })
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        _cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move { Ok(self.answer("gh ", args, None)) })
    }
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    futures_lite::future::block_on(f)
}

const REPO: &str = "/repo";

#[test]
fn git_text_exit_policy() {
    let one = |stdout: &str, code| {
        Fake::new({
            let stdout = stdout.to_string();
            move |_, _| {
                Some(GitOutput {
                    stdout: stdout.clone().into_bytes(),
                    stderr: b"err".to_vec(),
                    exit_code: code,
                })
            }
        })
    };
    // Exit 1 with output is a normal `git diff` result.
    assert_eq!(
        block(git_text(&one("d", Some(1)), Path::new(REPO), &["diff"])).unwrap(),
        "d"
    );
    // Exit 1 with no output, other codes and signals are failures with the app's message.
    let e = block(git_text(&one("", Some(1)), Path::new(REPO), &["diff"])).unwrap_err();
    assert_eq!(e.to_string(), "Git command failed: err, ");
    assert!(block(git_text(&one("o", Some(2)), Path::new(REPO), &["x"])).is_err());
    assert!(block(git_text(&one("o", None), Path::new(REPO), &["x"])).is_err());
    let e = block(gh_text(&one("o", Some(1)), Path::new(REPO), &["x"]));
    // gh: only exit 0 is success.
    assert_eq!(e.unwrap_err().to_string(), "gh command failed: err");
}

#[test]
fn main_branch_detection_order() {
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "symbolic-ref refs/remotes/origin/HEAD" => ok("refs/remotes/origin/trunk\n"),
        _ => None,
    });
    assert_eq!(
        block(detect_main_branch(&r, Path::new(REPO))).unwrap(),
        "origin/trunk"
    );

    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "rev-parse --verify master^{}" => ok("sha\n"),
        _ => None,
    });
    assert_eq!(
        block(detect_main_branch(&r, Path::new(REPO))).unwrap(),
        "master"
    );
    assert_eq!(
        r.calls(),
        [
            "symbolic-ref refs/remotes/origin/HEAD",
            "rev-parse --verify origin/main^{}",
            "rev-parse --verify origin/master^{}",
            "rev-parse --verify main^{}",
            "rev-parse --verify master^{}",
        ]
    );

    // Unborn repo: nothing resolves, falls through to `branch --show-current`.
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "branch --show-current" => ok("work\n"),
        _ => None,
    });
    assert_eq!(
        block(detect_main_branch(&r, Path::new(REPO))).unwrap(),
        "work\n"
    );
}

#[test]
fn current_branch_and_detached_display() {
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "rev-parse --abbrev-ref HEAD" => ok("HEAD\n"),
        "rev-parse --short HEAD" => ok("abc1234\n"),
        _ => None,
    });
    assert_eq!(
        block(detect_current_branch(&r, Path::new(REPO))).unwrap(),
        "HEAD"
    );
    assert_eq!(
        block(detect_current_branch_display(&r, Path::new(REPO))).unwrap(),
        "abc1234"
    );

    // Unborn: rev-parse fails, `branch --show-current` answers.
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "branch --show-current" => ok("main\n"),
        _ => None,
    });
    assert_eq!(
        block(detect_current_branch(&r, Path::new(REPO))).unwrap(),
        "main"
    );
}

#[test]
fn commit_chain_runs_each_stage_in_order() {
    let script = |a: &[String], _: Option<&[u8]>| match a.join(" ").as_str() {
        "symbolic-ref refs/remotes/origin/HEAD" => ok("refs/remotes/origin/main\n"),
        "rev-parse --abbrev-ref HEAD" => ok("feat\n"),
        "rev-parse --abbrev-ref --symbolic-full-name @{u}" => ok("origin/feat\n"),
        "log origin/feat..HEAD --format=COMMIT:%H\t%s --numstat" => {
            ok("COMMIT:aaa\tmsg\n1\t1\tf\n")
        }
        "pr create --base main --fill" => ok("https://github.com/o/r/pull/9\n"),
        _ => ok(""),
    };

    let r = Fake::new(script);
    let (commits, up, pr) = block(run_commit_chain(
        &r,
        Path::new(REPO),
        CommitChainMode::CommitOnly,
        "m",
        true,
        "feat",
    ))
    .unwrap();
    assert_eq!(pr, None);
    assert_eq!(up.as_deref(), Some("origin/feat"));
    assert_eq!(commits.len(), 1);
    assert_eq!(r.calls()[..2], ["add -A", "commit -m m"]);

    let r = Fake::new(script);
    block(run_commit_chain(
        &r,
        Path::new(REPO),
        CommitChainMode::CommitAndPush,
        "m",
        false,
        "feat",
    ))
    .unwrap();
    assert_eq!(
        r.calls()[..2],
        ["commit -m m", "push --set-upstream origin feat"]
    );

    let r = Fake::new(script);
    let (_, _, pr) = block(run_commit_chain(
        &r,
        Path::new(REPO),
        CommitChainMode::CommitAndCreatePr,
        "m",
        false,
        "feat",
    ))
    .unwrap();
    let pr = pr.unwrap();
    assert_eq!(
        (pr.number, pr.base_branch.as_str(), pr.state.as_str()),
        (9, "main", "OPEN")
    );
    assert!(
        r.calls()
            .contains(&"gh pr create --base main --fill".to_string())
    );
}

#[test]
fn commit_chain_stops_at_the_first_failure() {
    let r = Fake::new(|a, _| match a.first().map(String::as_str) {
        Some("push") => fail("could not read Username"),
        _ => ok(""),
    });
    let e = block(run_commit_chain(
        &r,
        Path::new(REPO),
        CommitChainMode::CommitAndCreatePr,
        "m",
        false,
        "b",
    ))
    .unwrap_err();
    assert!(e.to_string().contains("could not read Username"));
    assert!(!r.calls().iter().any(|c| c.starts_with("gh ")));
}

#[test]
fn stage_paths_falls_back_to_rm_cached_before_first_commit() {
    let r = Fake::new(|a, _| match a.first().map(String::as_str) {
        Some("restore") => fail("fatal: could not resolve 'HEAD'"),
        _ => ok(""),
    });
    let paths = vec!["a".to_string()];
    block(stage_paths(&r, Path::new(REPO), &paths, true)).unwrap();
    assert_eq!(r.calls(), ["restore --staged -- a", "rm --cached -- a"]);

    // A genuine failure is surfaced, not retried.
    let r = Fake::new(|_, _| fail("fatal: pathspec"));
    assert!(block(stage_paths(&r, Path::new(REPO), &paths, true)).is_err());
    assert_eq!(r.calls().len(), 1);
    assert!(block(stage_paths(&r, Path::new(REPO), &[], false)).is_err());
}

#[test]
fn patch_goes_to_stdin_with_recount() {
    let r = Fake::new(|_, stdin| {
        assert_eq!(stdin, Some(&b"diff"[..]));
        ok("")
    });
    block(apply_patch_cached(
        &r,
        Path::new(REPO),
        "-",
        Some(b"diff"),
        true,
        true,
    ))
    .unwrap();
    assert_eq!(r.calls(), ["apply --cached --reverse --recount -"]);
}

#[test]
fn discard_renamed_without_stash_removes_new_and_checks_out_old() {
    let r = Fake::new(|_, _| ok(""));
    let targets = vec![DiscardTarget {
        rel_path: "new.rs".into(),
        renamed_from: Some("old.rs".into()),
    }];
    block(discard_files(
        &r,
        Path::new(REPO),
        targets,
        false,
        "main",
        "App",
        &|_| {},
    ))
    .unwrap();
    assert_eq!(r.calls(), ["rm -f -- new.rs", "checkout main -- old.rs"]);
}

#[test]
fn discard_with_stash_on_head_stashes_then_restores_old_paths() {
    let r = Fake::new(|_, _| ok(""));
    let targets = vec![
        DiscardTarget {
            rel_path: "new.rs".into(),
            renamed_from: Some("old.rs".into()),
        },
        DiscardTarget {
            rel_path: "b.rs".into(),
            renamed_from: None,
        },
    ];
    block(discard_files(
        &r,
        Path::new(REPO),
        targets,
        true,
        "HEAD",
        "App",
        &|_| {},
    ))
    .unwrap();
    assert_eq!(
        r.calls(),
        [
            "stash push -u -m App: stash new.rs -- new.rs",
            "restore --staged --worktree -- old.rs",
            "stash push -u -m App: stash b.rs -- b.rs",
        ]
    );
}

#[test]
fn stash_with_nothing_to_save_is_not_an_error() {
    let r = Fake::new(|_, _| fail("No local changes to save"));
    block(stash_uncommitted_changes(
        &r,
        Path::new(REPO),
        "App",
        &["a".into()],
    ))
    .unwrap();
    let r = Fake::new(|_, _| fail("boom"));
    let e = block(stash_uncommitted_changes(
        &r,
        Path::new(REPO),
        "App",
        &["a".into(), "b".into()],
    ))
    .unwrap_err();
    assert!(
        e.to_string()
            .starts_with("Failed to stash changes for 2 files: ")
    );
}

#[test]
fn restore_and_clean_unknown_path_goes_through_rm_reset_and_fs() {
    let removed = Mutex::new(Vec::new());
    let r = Fake::new(|a, _| match a.first().map(String::as_str) {
        Some("restore") => fail("error: pathspec 'x' did not match any file(s) known to git"),
        Some("rm") => fail("fatal: pathspec 'x' did not match any files"),
        _ => ok(""),
    });
    block(git_restore_and_clean(
        &r,
        Path::new(REPO),
        &["x".to_string()],
        "main",
        &|p| removed.lock().unwrap().push(p.to_string()),
    ))
    .unwrap();
    assert_eq!(
        r.calls(),
        [
            "restore --staged --worktree --source=main -- x",
            "rm -f -- x",
            "reset -- x",
        ]
    );
    assert_eq!(*removed.lock().unwrap(), ["x"]);
}

#[test]
fn restore_before_first_commit_only_cleans() {
    let r = Fake::new(|a, _| match a.first().map(String::as_str) {
        Some("restore") => fail("fatal: could not resolve HEAD"),
        _ => ok(""),
    });
    block(git_restore_and_clean(
        &r,
        Path::new(REPO),
        &["x".to_string()],
        "HEAD",
        &|_| {},
    ))
    .unwrap();
    assert_eq!(r.calls().last().unwrap(), "clean -fd x");
}

#[test]
fn file_change_entries_split_tracked_from_untracked() {
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "diff --numstat HEAD" => ok("1\t2\ta.rs\n"),
        "ls-files --others --exclude-standard" => ok("new.txt\n\nother.txt\n"),
        _ => None,
    });
    let (tracked, untracked) = block(file_change_entries_tracked(&r, Path::new(REPO), true));
    assert_eq!(tracked.len(), 1);
    assert_eq!(untracked, ["new.txt", "other.txt"]);
    let (_, untracked) = block(file_change_entries_tracked(&r, Path::new(REPO), false));
    assert!(untracked.is_empty());
    assert!(r.calls().contains(&"diff --cached --numstat".to_string()));
}

#[test]
fn guard_probes_the_dot_git_dir() {
    let seen = block(guard_git_operation_in_progress(
        Path::new("/r"),
        |dir| async move {
            assert_eq!(dir, Path::new("/r/.git"));
            Ok(true)
        },
    ))
    .unwrap();
    assert!(seen);
}

#[test]
fn branch_tracking_reads_upstream_then_counts() {
    let r = Fake::new(|a, _| match a.join(" ").as_str() {
        "rev-parse --abbrev-ref --symbolic-full-name @{u}" => ok("origin/x\n"),
        "rev-list --left-right --cherry-mark --count HEAD...@{u}" => ok("1\t2\t0\n"),
        _ => None,
    });
    assert_eq!(
        block(read_branch_tracking(&r, Path::new(REPO))),
        (Some("origin/x".to_string()), Some((1, 2, 0)))
    );
    let r = Fake::new(|_, _| fail("no upstream"));
    assert_eq!(
        block(read_branch_tracking(&r, Path::new(REPO))),
        (None, None)
    );
    assert_eq!(r.calls().len(), 1);
}
