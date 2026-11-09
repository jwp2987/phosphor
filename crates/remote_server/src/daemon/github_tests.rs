use std::os::unix::fs::PermissionsExt as _;
use std::time::Duration;

use crate::proto::{
    GitHubPrInfoPush, GitHubRepositoryInfoPush, UpdateGitHubPrInfo, UpdateGitHubRepoInfo,
    notification, server_message,
};
use crate::shellcmd::git::test_runner::{git, git_available, repo_with};

use super::live_tests_support::Live;

/// A directory with a fake `gh` that answers `pr view` and `repo view`, put first on `PATH` for
/// the whole test process (the daemon's workers inherit it). Tests that use it hold the lock.
static PATH_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

struct FakeGh {
    _dir: tempfile::TempDir,
    old_path: Option<std::ffi::OsString>,
    _guard: std::sync::MutexGuard<'static, ()>,
}

impl FakeGh {
    fn install(script: &str) -> FakeGh {
        let guard = PATH_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let dir = tempfile::tempdir().unwrap();
        let gh = dir.path().join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let old_path = std::env::var_os("PATH");
        let mut paths = vec![dir.path().to_path_buf()];
        paths.extend(std::env::split_paths(
            old_path.as_deref().unwrap_or_default(),
        ));
        // SAFETY: serialised by PATH_LOCK; no other test thread reads PATH meanwhile.
        unsafe { std::env::set_var("PATH", std::env::join_paths(paths).unwrap()) };
        FakeGh {
            _dir: dir,
            old_path,
            _guard: guard,
        }
    }
}

impl Drop for FakeGh {
    fn drop(&mut self) {
        // SAFETY: still under PATH_LOCK.
        unsafe {
            match &self.old_path {
                Some(p) => std::env::set_var("PATH", p),
                None => std::env::remove_var("PATH"),
            }
        }
    }
}

const GH_OK: &str = r#"#!/bin/sh
case "$1 $2" in
  "--version "*) echo "gh version 2.0.0";;
  "pr view") printf '{"number":42,"url":"https://github.com/acme/widgets/pull/42","state":"OPEN","isDraft":false,"baseRefName":"main"}\n';;
  "repo view") printf '{"name":"widgets","owner":{"login":"acme"},"url":"https://github.com/acme/widgets"}\n';;
  *) echo "unexpected: $*" >&2; exit 1;;
esac
"#;

fn pr_push(m: &crate::proto::ServerMessage) -> Option<GitHubPrInfoPush> {
    match &m.message {
        Some(server_message::Message::GithubPrInfoPush(p)) => Some(p.clone()),
        _ => None,
    }
}

fn repo_push(m: &crate::proto::ServerMessage) -> Option<GitHubRepositoryInfoPush> {
    match &m.message {
        Some(server_message::Message::GithubRepositoryInfoPush(p)) => Some(p.clone()),
        _ => None,
    }
}

fn ask_pr(live: &mut Live, wire: &str) {
    live.notify(notification::Message::UpdateGithubPrInfo(
        UpdateGitHubPrInfo {
            repo_path: wire.to_string(),
        },
    ));
}

#[test]
fn pr_and_repository_info_come_from_gh_under_the_wire_path() {
    if !git_available() {
        return;
    }
    let _gh = FakeGh::install(GH_OK);
    let dir = repo_with(&[("a.txt", "1\n")]);
    let wire = dir.path().display().to_string();
    let mut live = Live::new();
    ask_pr(&mut live, &wire);
    let pr = live.expect_where(pr_push);
    assert_eq!(pr.repo_path, wire);
    let info = pr.pr_info.unwrap();
    assert_eq!(
        (info.number, info.state.as_str(), info.draft),
        (42, "OPEN", false)
    );
    assert_eq!(info.base_branch, "main");

    live.notify(notification::Message::UpdateGithubRepoInfo(
        UpdateGitHubRepoInfo {
            repo_path: wire.clone(),
        },
    ));
    let repo = live.expect_where(repo_push);
    let repo_info = repo.repository_info.unwrap();
    assert_eq!(repo_info.name, "widgets");
    assert_eq!(repo_info.owner.as_deref(), Some("acme"));
    assert_eq!(repo_info.host.as_deref(), Some("github.com"));
}

#[test]
fn a_gh_that_cannot_run_is_the_absent_shape_not_an_error() {
    if !git_available() {
        return;
    }
    // `gh` that fails even `--version`: the probe says it is unusable, so no lookup is tried.
    // (A binary that is not on PATH at all fails the same way, which the runner-level tests in
    // `shellcmd::git::gh` cover without touching the process environment.)
    let _gh = FakeGh::install("#!/bin/sh\necho 'gh: broken' >&2\nexit 127\n");
    let dir = repo_with(&[("a.txt", "1\n")]);
    let wire = dir.path().display().to_string();
    let mut live = Live::new();
    ask_pr(&mut live, &wire);
    let pr = live.expect_where(pr_push);
    live.notify(notification::Message::UpdateGithubRepoInfo(
        UpdateGitHubRepoInfo {
            repo_path: wire.clone(),
        },
    ));
    let repo = live.expect_where(repo_push);
    assert_eq!(pr.repo_path, wire);
    assert!(pr.pr_info.is_none());
    assert!(repo.repository_info.is_none());
}

#[test]
fn an_unauthenticated_gh_is_the_absent_shape() {
    if !git_available() {
        return;
    }
    let _gh = FakeGh::install(
        "#!/bin/sh\ncase \"$1\" in --version) echo gh;; *) echo 'To get started with GitHub CLI, please run:  gh auth login' >&2; exit 4;; esac\n",
    );
    let dir = repo_with(&[("a.txt", "1\n")]);
    let mut live = Live::new();
    ask_pr(&mut live, &dir.path().display().to_string());
    assert!(live.expect_where(pr_push).pr_info.is_none());
}

#[test]
fn the_branch_changing_refreshes_the_pr_and_a_plain_edit_does_not() {
    if !git_available() {
        return;
    }
    let counter = tempfile::tempdir().unwrap();
    let log = counter.path().join("calls");
    let script = format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"--version \"*) echo gh;;\n  \"pr view\") echo x >> '{}'; printf '{{\"number\":7,\"url\":\"u\",\"state\":\"OPEN\",\"isDraft\":false,\"baseRefName\":\"main\"}}\\n';;\n  *) exit 1;;\nesac\n",
        log.display()
    );
    let _gh = FakeGh::install(&script);
    let dir = repo_with(&[("a.txt", "1\n")]);
    let root = dir.path();
    let wire = root.display().to_string();
    let calls = || {
        std::fs::read_to_string(&log)
            .map(|s| s.lines().count())
            .unwrap_or(0)
    };
    let mut live = Live::new();
    ask_pr(&mut live, &wire);
    live.expect_where(pr_push);
    assert_eq!(calls(), 1);
    live.next(Duration::from_millis(300));

    // An edit and a commit on the same branch: no new gh lookup.
    std::fs::write(root.join("a.txt"), "1\n2\n").unwrap();
    git(root, &["commit", "-q", "-am", "same branch"]);
    live.next(Duration::from_millis(1500));
    assert_eq!(calls(), 1, "a plain edit or commit must not call gh again");

    // A new branch does.
    git(root, &["checkout", "-q", "-b", "feature"]);
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while calls() < 2 && std::time::Instant::now() < deadline {
        live.next(Duration::from_millis(100));
    }
    assert_eq!(calls(), 2, "the branch change refreshes the PR");
}
