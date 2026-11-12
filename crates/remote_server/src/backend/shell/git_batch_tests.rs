#![cfg(unix)]

use std::process::Stdio;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures::future::BoxFuture;
use futures_lite::future::block_on;

use super::*;
use crate::backend::executor::{
    ChannelKind, ExecError, ExecOutput, ExecRequest, OutOfBandExecutor,
};
use crate::proto::GetDiffState;
use crate::shellcmd::ShellKind;
use crate::shellcmd::git::diff::{self, Mode};
use crate::shellcmd::git::status::compute_git_status_metadata;
use crate::shellcmd::git::test_runner::{git, git_available, repo_with};
use crate::shellcmd::{ParsedSection, git::git_text};

/// A local `sh` standing in for the host; it counts every exec (every ssh channel) and can
/// misbehave on the first one, or lose one framed section of every answer.
#[derive(Debug, Default)]
struct Sh {
    seen: Mutex<Vec<String>>,
    /// The first exec fails outright.
    fail_first: bool,
    /// The first exec reports its output truncated.
    truncate_first: bool,
    /// Every answer loses the framed section with this name.
    drop_section: Option<&'static str>,
    /// ... but only the first exec's (a fallback exec for that section then gets its answer).
    drop_first_only: bool,
}

impl Sh {
    fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn count(&self) -> usize {
        self.seen.lock().unwrap().len()
    }
}

/// `text` without the framed section `name` (its BEGIN line through its END line).
fn without_section(text: &str, name: &str) -> String {
    let mut out = String::new();
    let mut skipping = false;
    for line in text.split_inclusive('\n') {
        if line.trim_end().ends_with(&format!(" BEGIN {name}")) {
            skipping = true;
            continue;
        }
        if skipping {
            if line.contains(&format!(" END {name} exit=")) {
                skipping = false;
            }
            continue;
        }
        out.push_str(line);
    }
    out
}

impl OutOfBandExecutor for Sh {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::LocalSh
    }
    fn shell_kind(&self) -> ShellKind {
        ShellKind::Posix
    }
    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move {
            let nth = {
                let mut seen = self.seen.lock().unwrap();
                seen.push(request.command.clone());
                seen.len()
            };
            if self.fail_first && nth == 1 {
                return Err(ExecError::Failed("the channel dropped".into()));
            }
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&request.command)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .output()
                .map_err(|e| ExecError::Failed(e.to_string()))?;
            let mut stdout = out.stdout;
            if let Some(name) = self
                .drop_section
                .filter(|_| nth == 1 || !self.drop_first_only)
            {
                stdout = without_section(&String::from_utf8_lossy(&stdout), name).into_bytes();
            }
            Ok(ExecOutput {
                stdout,
                stderr: out.stderr,
                exit_code: out.status.code(),
                truncated: self.truncate_first && nth == 1,
            })
        })
    }
}

/// A backend over `sh` whose generations never expire inside a test (the join window is an hour),
/// so every producer of the test joins the first one.
fn backend(sh: &Arc<Sh>) -> ShellBackend {
    ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::new(GitBatchStore::with_join_window(
            Duration::from_secs(3600),
        )))
        .with_git_batching(true)
}

/// The same, but every request starts its own generation.
fn backend_no_join(sh: &Arc<Sh>) -> ShellBackend {
    ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::new(GitBatchStore::with_join_window(Duration::ZERO)))
        .with_git_batching(true)
}

/// The per-call answer: a plain runner over its own counting `sh`.
fn per_call() -> (ShellGitRunner, Arc<Sh>) {
    let sh = Sh::new();
    (ShellGitRunner::new(sh.clone()), sh)
}

struct Fixture {
    name: &'static str,
    repo: tempfile::TempDir,
    /// Keeps the bare remote alive.
    _remote: Option<tempfile::TempDir>,
    clean: bool,
}

fn fixtures() -> Vec<Fixture> {
    let plain = repo_with(&[("a.txt", "one\n"), ("src/b.rs", "fn main() {}\n")]);

    let dirty = repo_with(&[("a.txt", "one\n"), ("src/b.rs", "fn main() {}\n")]);
    std::fs::write(dirty.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    std::fs::write(dirty.path().join("new.txt"), "n\n").unwrap();

    // On a branch with an upstream, one commit ahead of it.
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    let tracking = repo_with(&[("a.txt", "one\n")]);
    let remote_path = remote.path().to_str().unwrap().to_owned();
    git(tracking.path(), &["remote", "add", "origin", &remote_path]);
    git(tracking.path(), &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(tracking.path().join("a.txt"), "one\nahead\n").unwrap();
    git(tracking.path(), &["add", "-A"]);
    git(tracking.path(), &["commit", "-q", "-m", "ahead"]);

    // A feature branch, so the main branch and the current branch differ.
    let feature = repo_with(&[("a.txt", "one\n")]);
    git(feature.path(), &["checkout", "-q", "-b", "feature"]);
    std::fs::write(feature.path().join("f.txt"), "f\n").unwrap();
    git(feature.path(), &["add", "-A"]);
    git(feature.path(), &["commit", "-q", "-m", "feature work"]);

    vec![
        Fixture {
            name: "plain",
            repo: plain,
            _remote: None,
            clean: true,
        },
        Fixture {
            name: "dirty",
            repo: dirty,
            _remote: None,
            clean: false,
        },
        Fixture {
            name: "upstream",
            repo: tracking,
            _remote: Some(remote),
            clean: true,
        },
        Fixture {
            name: "feature",
            repo: feature,
            _remote: None,
            clean: true,
        },
    ]
}

fn path_of(repo: &tempfile::TempDir) -> &str {
    repo.path().to_str().unwrap()
}

#[test]
fn the_script_is_valid_sh() {
    let runner = ShellGitRunner::new(Sh::new());
    let script = build_script(&runner, "/some/it's repo").unwrap();
    let source = script.render_posix();
    for shell in ["sh", "dash"] {
        let Ok(mut child) = std::process::Command::new(shell)
            .arg("-n")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        use std::io::Write as _;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{shell} -n: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn the_status_chip_is_the_per_call_answer_from_one_exec() {
    if !git_available() {
        return;
    }
    for f in fixtures() {
        let (plain, plain_sh) = per_call();
        let want = block_on(compute_git_status_metadata(&plain, f.repo.path(), None)).unwrap();
        assert!(plain_sh.count() > 3, "{}: the baseline is per-call", f.name);

        let sh = Sh::new();
        let runner = block_on(backend(&sh).batched_git_runner("t", path_of(&f.repo))).unwrap();
        let got = block_on(compute_git_status_metadata(&runner, f.repo.path(), None)).unwrap();
        assert_eq!(got, want, "{}", f.name);
        assert_eq!(sh.count(), 1, "{}: one exec for the whole refresh", f.name);
    }
}

#[test]
fn the_diff_state_is_the_per_call_answer_and_a_clean_repo_costs_one_exec() {
    if !git_available() {
        return;
    }
    for f in fixtures() {
        for mode in [Mode::Head, Mode::MainBranch] {
            let (plain, _) = per_call();
            let want = block_on(diff::compute_snapshot(&plain, f.repo.path(), "wire", &mode));

            let sh = Sh::new();
            let runner = block_on(backend(&sh).batched_git_runner("t", path_of(&f.repo))).unwrap();
            let got = block_on(diff::compute_snapshot(
                &runner,
                f.repo.path(),
                "wire",
                &mode,
            ));
            assert_eq!(got, want, "{} {mode:?}", f.name);
            if f.clean && mode == Mode::Head {
                assert_eq!(sh.count(), 1, "{}: one exec for the whole refresh", f.name);
            }
        }
    }
}

#[test]
fn the_producers_of_one_precmd_share_one_exec_and_the_kill_switch_restores_per_call() {
    if !git_available() {
        return;
    }
    for f in fixtures() {
        let path = path_of(&f.repo);
        let request = || GetDiffState {
            repo_path: path.to_owned(),
            ..Default::default()
        };

        let sh = Sh::new();
        let be = backend(&sh);
        let status = block_on(be.git_status_shell(path, None)).unwrap();
        // The branch has not moved (and nothing was forced): no gh, no network.
        let pr = block_on(be.pr_info_shell(path, Some(Some(branch_of(&f))), false)).unwrap();
        let diff = block_on(be.get_diff_state_shell(request())).unwrap();
        if f.clean {
            assert_eq!(sh.count(), 1, "{}: status + PR check + diff state", f.name);
        }

        // Kill switch: the same answers, one exec per git call.
        let off_sh = Sh::new();
        let off = ShellBackend::new(Some(off_sh.clone())).with_git_batching(false);
        assert_eq!(block_on(off.git_status_shell(path, None)).unwrap(), status);
        assert_eq!(
            block_on(off.pr_info_shell(path, Some(Some(branch_of(&f))), false)).unwrap(),
            pr
        );
        assert_eq!(block_on(off.get_diff_state_shell(request())).unwrap(), diff);
        assert!(
            off_sh.count() > 10,
            "{}: with batching off every call is its own exec ({})",
            f.name,
            off_sh.count()
        );
    }
}

fn branch_of(f: &Fixture) -> String {
    git(f.repo.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned()
}

#[test]
fn a_request_after_the_window_starts_its_own_generation_and_sees_the_new_state() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = path_of(&dir);
    let sh = Sh::new();
    let be = backend_no_join(&sh);

    let before = block_on(be.git_status_shell(path, None)).unwrap().unwrap();
    std::fs::write(dir.path().join("late.txt"), "x\n").unwrap();
    let after = block_on(be.git_status_shell(path, None)).unwrap().unwrap();

    let files = |p: &crate::proto::GitStatusPush| {
        p.metadata
            .as_ref()
            .and_then(|m| m.stats_against_head.as_ref())
            .map(|s| s.files_changed)
    };
    assert_eq!(files(&before), Some(0));
    assert_eq!(files(&after), Some(1), "a stale answer was reused");
    assert_eq!(sh.count(), 2, "one exec per generation");
}

#[test]
fn concurrent_producers_of_one_refresh_run_one_script() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = path_of(&dir);
    let sh = Sh::new();
    let be = backend(&sh);
    let (a, b, c) = block_on(futures::future::join3(
        be.git_status_shell(path, None),
        be.get_diff_state_shell(GetDiffState {
            repo_path: path.to_owned(),
            ..Default::default()
        }),
        be.pr_info_shell(path, Some(Some("main".into())), false),
    ));
    assert!(a.unwrap().is_some());
    assert!(b.is_ok() && c.is_ok());
    assert_eq!(sh.count(), 1);
}

#[test]
fn a_failed_script_falls_back_to_the_per_call_path_for_the_refresh() {
    if !git_available() {
        return;
    }
    for (what, fail_first, truncate_first) in
        [("exec error", true, false), ("truncated", false, true)]
    {
        for f in fixtures() {
            let (plain, _) = per_call();
            let want = block_on(compute_git_status_metadata(&plain, f.repo.path(), None)).unwrap();

            let sh = Arc::new(Sh {
                fail_first,
                truncate_first,
                ..Sh::default()
            });
            let runner = block_on(backend(&sh).batched_git_runner("t", path_of(&f.repo))).unwrap();
            let got = block_on(compute_git_status_metadata(&runner, f.repo.path(), None)).unwrap();
            assert_eq!(got, want, "{what}: {}", f.name);
            assert!(
                sh.count() > 3,
                "{what}: {}: per-call after the batch",
                f.name
            );
        }
    }
}

#[test]
fn a_missing_section_costs_only_its_own_calls() {
    if !git_available() {
        return;
    }
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    let dir = repo_with(&[("a.txt", "one\n")]);
    let remote_path = remote.path().to_str().unwrap().to_owned();
    git(dir.path(), &["remote", "add", "origin", &remote_path]);
    git(dir.path(), &["push", "-q", "-u", "origin", "main"]);

    let (plain, _) = per_call();
    let want = block_on(compute_git_status_metadata(&plain, dir.path(), None)).unwrap();

    let sh = Arc::new(Sh {
        drop_section: Some("upstream"),
        ..Sh::default()
    });
    let runner = block_on(backend(&sh).batched_git_runner("t", path_of(&dir))).unwrap();
    let got = block_on(compute_git_status_metadata(&runner, dir.path(), None)).unwrap();
    assert_eq!(got, want);
    // The batch, and the one upstream question; the counts came from the batch.
    assert_eq!(sh.count(), 2);
}

#[test]
fn a_call_the_script_does_not_cover_is_one_exec_and_stdin_never_hits_the_prefetch() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let sh = Sh::new();
    let runner = block_on(backend(&sh).batched_git_runner("t", path_of(&dir))).unwrap();
    assert_eq!(sh.count(), 1);

    // Covered: no exec.
    let head = block_on(git_text(&runner, dir.path(), &["rev-parse", "HEAD"])).unwrap();
    assert_eq!(head.trim(), git(dir.path(), &["rev-parse", "HEAD"]).trim());
    assert_eq!(sh.count(), 1);
    // Not covered: one exec.
    block_on(git_text(&runner, dir.path(), &["log", "-1", "--format=%s"])).unwrap();
    assert_eq!(sh.count(), 2);
    // Covered arguments but with stdin (a write's shape), or in another directory: exec.
    block_on(runner.run(&["rev-parse", "HEAD"], Some(&b""[..]), dir.path())).unwrap();
    assert_eq!(sh.count(), 3);
    let other = tempfile::tempdir().unwrap();
    let _ = block_on(runner.run(&["rev-parse", "HEAD"], None, other.path()));
    assert_eq!(sh.count(), 4);
}

#[test]
fn gh_is_answered_not_found_without_an_exec_only_when_the_script_saw_no_gh() {
    let dir = tempfile::tempdir().unwrap();
    let sh = Sh::new();
    let inner = ShellGitRunner::new(sh.clone());
    let data = |gh_available| {
        Some(Arc::new(Prefetched {
            entries: HashMap::new(),
            gh_available,
            ..Prefetched::default()
        }))
    };

    let missing = BatchedGitRunner::new(inner.clone(), path_of_dir(&dir), data(Some(false)));
    let out = block_on(missing.run_gh(&["pr", "view"], dir.path())).unwrap();
    assert_eq!(out.exit_code, Some(127));
    assert!(out.stderr_lossy().contains("not found"));
    assert_eq!(sh.count(), 0);

    // Present, or not known: the real call.
    for known in [Some(true), None] {
        let before = sh.count();
        let runner = BatchedGitRunner::new(inner.clone(), path_of_dir(&dir), data(known));
        let _ = block_on(runner.run_gh(&["--version"], dir.path()));
        assert_eq!(sh.count(), before + 1);
    }
}

fn path_of_dir(dir: &tempfile::TempDir) -> &str {
    dir.path().to_str().unwrap()
}

fn section(name: &str, exit: i32, output: &str) -> ParsedSection {
    ParsedSection {
        name: name.to_owned(),
        output: output.as_bytes().to_vec(),
        exit,
    }
}

#[test]
fn a_derived_section_is_filed_only_under_the_key_rust_computes() {
    let rev_list = |extra: &[&str]| {
        let mut key: Vec<String> = ["rev-list", "HEAD", "--not"].map(str::to_owned).to_vec();
        key.extend(extra.iter().map(|s| (*s).to_owned()));
        key
    };
    let frame = |header: &str| Frame {
        sections: vec![
            section("abbrev", 0, "main\n"),
            section("upstream", 128, ""),
            section("forkunique", 0, &format!("{header}\nabc123\n")),
        ],
    };
    let key = rev_list(&[
        "--exclude=main",
        "--branches",
        "--exclude=origin/main",
        "--remotes",
    ]);

    // The host shell and Rust agree on the current branch: filed, header removed.
    let agree = assemble(&frame("main"));
    assert_eq!(agree.entries[&key].stdout, b"abc123\n");
    // They do not: a miss, never a wrong answer.
    let disagree = assemble(&frame("something-else"));
    assert!(!disagree.entries.contains_key(&key));
}

#[test]
fn an_upstream_range_is_filed_when_the_header_matches_and_the_log_is_the_sections_output() {
    let frame = Frame {
        sections: vec![
            section("upstream", 0, "origin/main\n"),
            section("unpushed", 0, "origin/main..HEAD\nCOMMIT:abc\tsubject\n"),
        ],
    };
    let prefetched = assemble(&frame);
    let key = vec![
        "log".to_owned(),
        "origin/main..HEAD".to_owned(),
        UNPUSHED_LOG_FORMAT.to_owned(),
        "--numstat".to_owned(),
    ];
    assert_eq!(prefetched.entries[&key].stdout, b"COMMIT:abc\tsubject\n");
    assert_eq!(prefetched.gh_available, None);
}

// ---------------------------------------------------------------------------------------------
// Lever B: the file tree rides in the same script (#2156).

fn canonical(dir: &tempfile::TempDir) -> String {
    std::fs::canonicalize(dir.path())
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
}

fn backend_with_caps(sh: &Arc<Sh>, caps: TreeCaps) -> ShellBackend {
    ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::new(
            GitBatchStore::with_join_window(Duration::from_secs(3600)).with_tree_caps(caps),
        ))
        .with_git_batching(true)
}

/// The per-call answer of the tree: batching off, so it is the navigation probe and one list script.
fn tree_per_call(path: &str) -> (ShellBackend, Arc<Sh>) {
    let sh = Sh::new();
    let be = ShellBackend::new(Some(sh.clone())).with_git_batching(false);
    block_on(be.navigate_to_directory_shell(path.to_owned())).unwrap();
    (be, sh)
}

fn pushes(be: &ShellBackend) -> String {
    format!("{:?}", be.take_pushes())
}

#[test]
fn the_tree_sections_are_valid_sh() {
    let runner = ShellGitRunner::new(Sh::new());
    let script = build_script(&runner, "/some/it's repo")
        .and_then(|s| add_tree_sections(s, ShellKind::Posix, "/some/it's repo", TreeCaps::DEFAULT))
        .unwrap();
    for name in ["canon", "git", "files", "ignoredpaths"] {
        assert!(script.section_names().contains(&name), "{name}");
    }
    let source = script.render_posix();
    for shell in ["sh", "dash"] {
        let Ok(mut child) = std::process::Command::new(shell)
            .arg("-n")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
        else {
            continue;
        };
        use std::io::Write as _;
        child
            .stdin
            .take()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(
            out.status.success(),
            "{shell} -n: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
}

#[test]
fn a_cd_into_a_repository_costs_one_exec_for_tree_status_diff_and_pr_check() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n"), ("src/b.rs", "fn main() {}\n")]);
    let path = canonical(&dir);
    let branch = git(dir.path(), &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_owned();
    let sh = Sh::new();
    let be = backend(&sh);

    let nav = block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
    assert!(nav.is_git);
    assert_eq!(nav.indexed_path, path);
    let status = block_on(be.git_status_shell(&path, None)).unwrap();
    assert!(status.is_some());
    block_on(be.pr_info_shell(&path, Some(Some(branch)), false)).unwrap();
    block_on(be.get_diff_state_shell(GetDiffState {
        repo_path: path.clone(),
        ..Default::default()
    }))
    .unwrap();
    // The prompt's tree refresh joins the same generation.
    block_on(be.refresh_repo_tree(&path)).unwrap();
    assert_eq!(sh.count(), 1, "tree + status + diff + PR check");
    assert!(
        pushes(&be).contains("a.txt"),
        "the snapshot carries the tree"
    );
}

#[test]
fn the_batched_tree_is_the_per_call_tree_for_a_navigation_and_a_refresh() {
    if !git_available() {
        return;
    }
    for f in fixtures() {
        let path = canonical(&f.repo);
        let sh = Sh::new();
        let be = backend_no_join(&sh);
        block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
        let (off, off_sh) = tree_per_call(&path);
        assert_eq!(pushes(&be), pushes(&off), "{}: the snapshot", f.name);
        assert_eq!(sh.count(), 1, "{}: the navigation is one exec", f.name);
        assert_eq!(off_sh.count(), 2, "{}: probe + lists", f.name);

        // The tree changes; the refresh pushes the same difference either way.
        std::fs::write(f.repo.path().join("added.txt"), "x\n").unwrap();
        std::fs::create_dir_all(f.repo.path().join("newdir/inner")).unwrap();
        std::fs::write(f.repo.path().join("newdir/inner/c.txt"), "c\n").unwrap();
        block_on(be.refresh_repo_tree(&path)).unwrap();
        block_on(off.refresh_repo_tree(&path)).unwrap();
        let (batched, plain) = (pushes(&be), pushes(&off));
        assert!(batched.contains("added.txt"), "{}: {batched}", f.name);
        assert_eq!(batched, plain, "{}: the refresh", f.name);
        assert_eq!(sh.count(), 2, "{}: one exec per refresh", f.name);
    }
}

#[test]
fn a_capped_or_missing_tree_list_falls_back_to_its_own_exec_with_the_same_answer() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[
        ("a.txt", "one\n"),
        (".gitignore", "build_output_directory/\n"),
    ]);
    std::fs::create_dir(dir.path().join("build_output_directory")).unwrap();
    std::fs::write(dir.path().join("build_output_directory/o.bin"), "o\n").unwrap();
    let path = canonical(&dir);
    let (off, _) = tree_per_call(&path);
    let want = pushes(&off);
    assert!(want.contains("build_output_directory"), "{want}");

    // Control: the default caps take both lists from the batch.
    let sh = Sh::new();
    let be = backend(&sh);
    block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
    assert_eq!((pushes(&be), sh.count()), (want.clone(), 1));

    let big = 1024 * 1024;
    for (what, caps) in [
        (
            "both lists capped",
            TreeCaps {
                files: 16,
                ignored: 16,
            },
        ),
        (
            "only the files list capped",
            TreeCaps {
                files: 16,
                ignored: big,
            },
        ),
        (
            "only the ignored list capped",
            TreeCaps {
                files: big,
                ignored: 16,
            },
        ),
    ] {
        let sh = Sh::new();
        let be = backend_with_caps(&sh, caps);
        block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
        assert_eq!(pushes(&be), want, "{what}");
        // The batch, and ONE exec for whichever lists were capped.
        assert_eq!(sh.count(), 2, "{what}");
    }

    for section in ["files", "ignoredpaths"] {
        let sh = Arc::new(Sh {
            drop_section: Some(section),
            drop_first_only: true,
            ..Sh::default()
        });
        let be = backend(&sh);
        block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
        assert_eq!(pushes(&be), want, "{section} missing");
        assert_eq!(sh.count(), 2, "{section} missing costs only itself");
    }
}

#[test]
fn a_tree_list_at_its_cap_is_dropped_from_the_batch_and_the_others_stay() {
    let frame = Frame {
        sections: vec![
            section("canon", 0, "/r\n"),
            section("git", 0, "/r\n"),
            section("files", 0, &"x".repeat(100)),
            section("ignoredpaths", 0, "dir/\0"),
        ],
    };
    let tree = tree_frame(
        &frame,
        TreeCaps {
            files: 100,
            ignored: 1000,
        },
    )
    .unwrap();
    assert!(tree.section("files").is_none());
    for kept in ["canon", "git", "ignoredpaths"] {
        assert!(tree.section(kept).is_some(), "{kept}");
    }
    let all = tree_frame(&frame, TreeCaps::DEFAULT).unwrap();
    assert!(all.section("files").is_some());
}

#[test]
fn the_kill_switch_keeps_the_tree_on_its_own_commands() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let (_, sh) = tree_per_call(&path);
    // The probe, then the two lists: no batched script.
    assert_eq!(sh.count(), 2);
    assert!(
        sh.seen
            .lock()
            .unwrap()
            .iter()
            .all(|c| !c.contains("verify0")),
        "no git batch ran"
    );
}

// ---------------------------------------------------------------------------------------------
// One exec per PROMPT, however late its producers ask and however they spell the path (#2156).

/// A backend over `sh` whose store keys generations on the prompt counter, with the old 100 ms
/// window, so only the prompt keying can make a producer 150 or 400 ms late join.
fn prompt_backend(sh: &Arc<Sh>) -> (ShellBackend, Arc<GitBatchStore>) {
    let store = Arc::new(GitBatchStore::with_join_window(JOIN_WINDOW).with_prompt_keying(true));
    let be = ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::clone(&store))
        .with_git_batching(true);
    (be, store)
}

fn pause(ms: u64) {
    std::thread::sleep(Duration::from_millis(ms));
}

#[test]
fn join_key_collapses_spellings_of_one_directory() {
    assert_eq!(join_key("/a/b"), "/a/b");
    assert_eq!(join_key("/a/b/"), "/a/b");
    assert_eq!(join_key("/a//b/./"), "/a/b");
    assert_eq!(join_key("/"), "/");
    assert_eq!(join_key("/a/../b"), "/a/../b");
    assert_ne!(join_key("/a/b"), join_key("/a/c"));
}

#[test]
fn producers_of_one_prompt_share_one_exec_whenever_they_ask_and_however_they_spell_the_path() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let (be, store) = prompt_backend(&sh);
    store.begin_prompt();

    // The first producer at 0 ms (the path as the tree spells it), the others 150 ms and 400 ms
    // later, spelled with a trailing slash and a `.` component.
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 1);
    pause(150);
    block_on(be.get_diff_state_shell(GetDiffState {
        repo_path: format!("{path}/"),
        ..Default::default()
    }))
    .unwrap();
    pause(250);
    block_on(be.get_diff_state_shell(GetDiffState {
        repo_path: format!("{path}/./"),
        ..Default::default()
    }))
    .unwrap();
    block_on(be.refresh_repo_tree(&path)).unwrap();
    assert_eq!(sh.count(), 1, "one prompt, one exec");
}

#[test]
fn a_new_prompt_starts_a_new_exec() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let (be, store) = prompt_backend(&sh);
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 1);
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 2, "the next prompt asks the host again");
    // The window alone never joins across a prompt.
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 3);
}

#[test]
fn a_late_producer_of_an_unannounced_prompt_does_not_reuse_an_old_answer() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    // No begin_prompt: the pure 100 ms window of before.
    let store = Arc::new(GitBatchStore::with_join_window(JOIN_WINDOW).with_prompt_keying(true));
    let be = ShellBackend::new(Some(sh.clone()))
        .with_git_batch(store)
        .with_git_batching(true);
    block_on(be.git_status_shell(&path, None)).unwrap();
    pause(150);
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 2);
}

#[test]
fn the_refresh_after_a_navigation_of_the_same_prompt_adds_no_tree_exec() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n"), ("src/b.rs", "fn main() {}\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let (be, store) = prompt_backend(&sh);
    store.begin_prompt();
    block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
    pause(150);
    block_on(be.refresh_repo_tree(&path)).unwrap();
    assert_eq!(sh.count(), 1, "navigation + refresh of one prompt");
    store.begin_prompt();
    block_on(be.refresh_repo_tree(&path)).unwrap();
    assert_eq!(
        sh.count(),
        2,
        "the next prompt's refresh is its own single exec"
    );
}

#[test]
fn without_prompt_keying_the_old_window_applies() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let store = Arc::new(GitBatchStore::with_join_window(JOIN_WINDOW).with_prompt_keying(false));
    let be = ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::clone(&store))
        .with_git_batching(true);
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    pause(150);
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 2, "150 ms later is a new generation");
}

#[test]
fn the_kill_switch_runs_every_call_by_itself() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let store = Arc::new(GitBatchStore::with_join_window(JOIN_WINDOW).with_prompt_keying(true));
    let be = ShellBackend::new(Some(sh.clone()))
        .with_git_batch(Arc::clone(&store))
        .with_git_batching(false);
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    let first = sh.count();
    assert!(first > 1, "per-call: several execs, got {first}");
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), first * 2, "nothing is shared");
}

// The exact sequence of a `cd` in the r27b lab: the navigation's tree request arrives BEFORE the
// prompt's Precmd, the chips after it (#2156).

#[test]
fn a_cds_navigation_and_the_precmd_that_follows_it_share_one_exec() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "one\n"), ("src/b.rs", "fn main() {}\n")]);
    let path = canonical(&dir);
    let sh = Sh::new();
    let (be, store) = prompt_backend(&sh);
    store.begin_navigation();
    block_on(be.navigate_to_directory_shell(path.clone())).unwrap();
    // The Precmd claims the navigation's bump; chips and the refresh then join its generation.
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    block_on(be.get_diff_state_shell(GetDiffState {
        repo_path: path.clone(),
        ..Default::default()
    }))
    .unwrap();
    block_on(be.refresh_repo_tree(&path)).unwrap();
    assert_eq!(sh.count(), 1, "navigation + precmd producers of one cd");
    // The next prompt is a prompt of its own, and the claim is spent.
    store.begin_prompt();
    block_on(be.git_status_shell(&path, None)).unwrap();
    assert_eq!(sh.count(), 2);
}

#[test]
fn a_navigation_without_a_precmd_does_not_swallow_the_next_prompts_bump_forever() {
    let store = GitBatchStore::with_join_window(JOIN_WINDOW).with_prompt_keying(true);
    store.begin_navigation();
    store.begin_navigation();
    assert_eq!(
        store.prompt.load(Ordering::SeqCst),
        2,
        "two navigations, two generations"
    );
    store.begin_prompt();
    assert_eq!(
        store.prompt.load(Ordering::SeqCst),
        2,
        "the first precmd claims the bump"
    );
    store.begin_prompt();
    assert_eq!(store.prompt.load(Ordering::SeqCst), 3);
}
