use std::path::Path;

use command::r#async::Command;
use command::Stdio;
use tempfile::TempDir;

use super::{
    compute_unpushed_state, detect_current_branch, detect_current_branch_display,
    get_pr_for_branch, git_operation_in_progress, is_gh_missing_error, run_commit_chain,
    CommitChainMode, PrInfo, RepositoryInfo,
};

/// Helper: run a git command inside the given repo directory.
async fn git(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .expect("failed to run git");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

/// Creates a temp git repo with one commit and returns `(dir_handle, repo_path)`.
async fn init_repo() -> (TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().to_path_buf();

    git(&path, &["init", "-b", "main"]).await;
    git(&path, &["config", "user.email", "test@test.com"]).await;
    git(&path, &["config", "user.name", "Test"]).await;
    git(&path, &["commit", "--allow-empty", "-m", "initial"]).await;

    (dir, path)
}

#[tokio::test]
async fn on_normal_branch_returns_branch_name() {
    let (_dir, repo) = init_repo().await;
    git(&repo, &["checkout", "-b", "feature-xyz"]).await;

    assert_eq!(detect_current_branch(&repo).await.unwrap(), "feature-xyz");
    assert_eq!(
        detect_current_branch_display(&repo).await.unwrap(),
        "feature-xyz"
    );
}

#[tokio::test]
async fn detached_head_raw_returns_head() {
    let (_dir, repo) = init_repo().await;
    git(&repo, &["checkout", "--detach", "HEAD"]).await;

    assert_eq!(detect_current_branch(&repo).await.unwrap(), "HEAD");
}

#[tokio::test]
async fn detached_head_display_returns_short_sha() {
    let (_dir, repo) = init_repo().await;
    let full_sha = git(&repo, &["rev-parse", "HEAD"]).await;
    git(&repo, &["checkout", "--detach", "HEAD"]).await;

    let result = detect_current_branch_display(&repo).await.unwrap();

    assert_ne!(
        result, "HEAD",
        "display variant should not return literal HEAD"
    );
    assert!(
        full_sha.starts_with(&result),
        "expected {full_sha} to start with {result}"
    );
}

#[tokio::test]
async fn detached_tag_display_returns_short_sha() {
    let (_dir, repo) = init_repo().await;
    git(&repo, &["tag", "v1.0"]).await;
    git(&repo, &["checkout", "v1.0"]).await;

    let full_sha = git(&repo, &["rev-parse", "HEAD"]).await;
    let result = detect_current_branch_display(&repo).await.unwrap();

    assert_ne!(result, "HEAD");
    assert!(
        full_sha.starts_with(&result),
        "expected {full_sha} to start with {result}"
    );
}

/// Creates a bare repo to act as `origin` and wires `repo` up to push to it,
/// setting upstream tracking on the current branch. Returns the bare-repo dir
/// handle (kept alive by the caller). Fully offline (local file remote).
async fn add_bare_origin(repo: &Path) -> TempDir {
    let bare = tempfile::tempdir().expect("failed to create bare temp dir");
    git(&bare.path().to_path_buf(), &["init", "--bare"]).await;
    let bare_url = bare.path().to_string_lossy().to_string();
    git(repo, &["remote", "add", "origin", &bare_url]).await;
    // `-u` sets the upstream tracking ref (origin/<branch>).
    git(repo, &["push", "-u", "origin", "main"]).await;
    bare
}

#[tokio::test]
async fn commit_chain_commit_only_creates_a_commit_and_reports_delta() {
    let (_dir, repo) = init_repo().await;
    tokio::fs::write(repo.join("file.txt"), "hello\n")
        .await
        .expect("write file");

    let (commits, upstream_ref, pr_info) = run_commit_chain(
        &repo,
        CommitChainMode::CommitOnly,
        "add file.txt",
        true, // include_unstaged: stages the new file via `git add -A`
        "main",
        None,
    )
    .await
    .expect("commit chain should succeed");

    // The commit landed: HEAD's subject is the message we passed.
    let subject = git(&repo, &["log", "-1", "--format=%s"]).await;
    assert_eq!(subject, "add file.txt");
    // Commit-only never opens a PR.
    assert!(pr_info.is_none());
    // No upstream configured, so the delta reports no tracking ref. With no
    // upstream, the unpushed set falls back to the branch's own commits, so the
    // just-created commit is reported.
    assert!(upstream_ref.is_none());
    assert!(
        commits.iter().any(|c| c.subject == "add file.txt"),
        "expected the new commit in the unpushed delta, got {commits:?}"
    );
}

#[tokio::test]
async fn compute_unpushed_state_tracks_upstream_and_unpushed_commits() {
    let (_dir, repo) = init_repo().await;
    let _bare = add_bare_origin(&repo).await;

    // Freshly pushed: upstream is origin/main and nothing is unpushed.
    let (commits, upstream_ref) = compute_unpushed_state(&repo).await;
    assert_eq!(upstream_ref.as_deref(), Some("origin/main"));
    assert!(
        commits.is_empty(),
        "expected nothing unpushed, got {commits:?}"
    );

    // A new local commit is now ahead of the upstream.
    tokio::fs::write(repo.join("new.txt"), "content\n")
        .await
        .expect("write file");
    git(&repo, &["add", "-A"]).await;
    git(&repo, &["commit", "-m", "local change"]).await;

    let (commits, upstream_ref) = compute_unpushed_state(&repo).await;
    assert_eq!(upstream_ref.as_deref(), Some("origin/main"));
    assert_eq!(commits.len(), 1, "expected one unpushed commit");
    assert_eq!(commits[0].subject, "local change");
}

/// Each sentinel git writes under `.git/` while an operation is in flight, and
/// the name of the state it represents. A commit/push issued in any of these
/// states would behave surprisingly (e.g. a commit would silently complete an
/// in-progress merge) or fail outright, so all of them must block.
const IN_PROGRESS_SENTINELS: &[(&str, &str)] = &[
    ("MERGE_HEAD", "merge"),
    ("CHERRY_PICK_HEAD", "cherry-pick"),
    ("REVERT_HEAD", "revert"),
    ("rebase-merge", "interactive/merge rebase"),
    ("rebase-apply", "am-style rebase"),
    ("index.lock", "held index lock"),
];

#[tokio::test]
async fn git_operation_in_progress_false_on_clean_repo() {
    let (_dir, repo) = init_repo().await;
    assert!(
        !git_operation_in_progress(&repo),
        "a freshly initialized repo has no operation in progress"
    );
}

#[tokio::test]
async fn git_operation_in_progress_detects_every_sentinel() {
    for (sentinel, description) in IN_PROGRESS_SENTINELS {
        let (_dir, repo) = init_repo().await;
        let path = repo.join(".git").join(sentinel);

        // `rebase-merge` / `rebase-apply` are directories, the rest are files.
        // Both are detected by existence, so create whichever kind matches.
        if sentinel.starts_with("rebase-") {
            tokio::fs::create_dir(&path)
                .await
                .expect("create rebase state dir");
        } else {
            tokio::fs::write(&path, "").await.expect("write sentinel");
        }

        assert!(
            git_operation_in_progress(&repo),
            "expected {description} ({sentinel}) to block git operations"
        );

        // Clearing the sentinel unblocks again, so the check is not sticky.
        if sentinel.starts_with("rebase-") {
            tokio::fs::remove_dir(&path).await.expect("remove state dir");
        } else {
            tokio::fs::remove_file(&path).await.expect("remove sentinel");
        }
        assert!(
            !git_operation_in_progress(&repo),
            "expected clearing {sentinel} to unblock git operations"
        );
    }
}

#[tokio::test]
async fn git_operation_in_progress_false_for_nonexistent_repo() {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let missing = dir.path().join("not-a-repo");
    assert!(
        !git_operation_in_progress(&missing),
        "a path with no .git directory reports no operation in progress"
    );
}
// ─── Ported from Warp: `warp/master:app/src/util/git_tests.rs` ───────────────
//
// Warp-mirrored coverage for `get_pr_for_branch` (AGENTS.md §5.10). The tests
// below are Warp's, verbatim except for the shape adaptations noted in each
// one; no assertion has been relaxed. Warp inlines the fake-`gh` shim in every
// test — the only structural change is hoisting it into `fake_gh` below, which
// is byte-for-byte the same shim, written to a temp dir and prepended to
// `PATH`, so the tests stay hermetic and never reach a real GitHub.

/// Writes an executable `gh` shim running `script` into a fresh temp dir and
/// returns `(dir_handle, path_env)`, where `path_env` prepends that dir to the
/// inherited `PATH`. The handle must be kept alive for the shim to exist.
#[cfg(unix)]
fn fake_gh(script: &str) -> (TempDir, String) {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    let fake_bin = tempfile::tempdir().expect("failed to create fake bin dir");
    let gh_path = fake_bin.path().join("gh");
    fs::write(&gh_path, script).expect("failed to write fake gh");
    let mut permissions = fs::metadata(&gh_path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&gh_path, permissions).unwrap();

    let path_env = format!(
        "{}:{}",
        fake_bin.path().display(),
        std::env::var("PATH").unwrap_or_default()
    );
    (fake_bin, path_env)
}

#[tokio::test]
async fn get_pr_for_branch_returns_none_for_detached_head() {
    let (_dir, repo) = init_repo().await;
    git(&repo, &["checkout", "--detach", "HEAD"]).await;
    assert_eq!(get_pr_for_branch(&repo, None).await.unwrap(), None);
}

#[cfg(unix)]
#[tokio::test]
async fn get_pr_for_branch_does_not_require_origin_remote() {
    let (_dir, repo) = init_repo().await;
    let (_fake_bin, path_env) = fake_gh(
        "#!/bin/sh\nprintf '{\"number\":123,\"url\":\"https://github.com/warp/warp/pull/123\",\"state\":\"OPEN\",\"isDraft\":true,\"baseRefName\":\"main\"}\\n'\n",
    );

    // Shape adaptation: Warp's `PrInfo` also carries `state`, `draft` and
    // `base_branch`, which the fork's `PrInfo` does not have — the fork never
    // requests those `gh` fields. Warp's assertions on the two fields the fork
    // does have are unchanged; the absent payload is a separate parity gap
    // tracked under issue #2, not a weakening of this test.
    assert_eq!(
        get_pr_for_branch(&repo, Some(&path_env)).await.unwrap(),
        Some(PrInfo {
            number: 123,
            url: "https://github.com/warp/warp/pull/123".to_string(),
        })
    );
}

#[cfg(unix)]
#[tokio::test]
async fn get_pr_for_branch_returns_none_when_gh_finds_no_pr() {
    let (_dir, repo) = init_repo().await;
    let (_fake_bin, path_env) = fake_gh(
        "#!/bin/sh\nprintf 'no pull requests found for branch \"main\"\\n' >&2\nexit 1\n",
    );

    assert_eq!(
        get_pr_for_branch(&repo, Some(&path_env)).await.unwrap(),
        None
    );
}

#[cfg(unix)]
#[tokio::test]
async fn get_pr_for_branch_returns_none_when_gh_cannot_resolve_github_repo() {
    let (_dir, repo) = init_repo().await;
    let (_fake_bin, path_env) = fake_gh(
        "#!/bin/sh\nprintf 'none of the git remotes configured for this repository point to a known GitHub host\\n' >&2\nexit 1\n",
    );

    assert_eq!(
        get_pr_for_branch(&repo, Some(&path_env)).await.unwrap(),
        None
    );
}

// ─── Ported from Warp: `gh repo view` / error-classification coverage ────────
//
// These target helpers that did not exist in the fork until issue #135 was
// fixed (`RepositoryInfo`, `repository_info_from_gh_output`,
// `get_repository_info`, `is_gh_missing_error`, `is_no_pr_for_branch_error`,
// `is_repository_lookup_not_applicable_error`). Warp's assertions are verbatim;
// the only change is reusing the `fake_gh` shim above instead of re-inlining
// Warp's identical copy in each test.

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_parses_name_and_owner() {
    // No url in the output => host is absent.
    assert_eq!(
        super::repository_info_from_gh_output(
            r#"{"name":"warp-internal","owner":{"login":"warpdotdev"}}"#
        )
        .unwrap(),
        RepositoryInfo {
            name: "warp-internal".to_owned(),
            owner: Some("warpdotdev".to_owned()),
            host: None,
        }
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_parses_host_from_url() {
    assert_eq!(
        super::repository_info_from_gh_output(
            r#"{"name":"warp-internal","owner":{"login":"warpdotdev"},"url":"https://github.com/warpdotdev/warp-internal"}"#
        )
        .unwrap(),
        RepositoryInfo {
            name: "warp-internal".to_owned(),
            owner: Some("warpdotdev".to_owned()),
            host: Some("github.com".to_owned()),
        }
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_rejects_missing_name() {
    assert!(super::repository_info_from_gh_output(r#"{"owner":{"login":"warpdotdev"}}"#).is_err());
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_rejects_missing_owner_login() {
    assert!(
        super::repository_info_from_gh_output(r#"{"name":"warp-internal","owner":{}}"#).is_err()
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_rejects_empty_fields() {
    assert!(
        super::repository_info_from_gh_output(r#"{"name":"","owner":{"login":"warpdotdev"}}"#)
            .is_err()
    );
    assert!(
        super::repository_info_from_gh_output(r#"{"name":"warp-internal","owner":{"login":""}}"#)
            .is_err()
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn repository_info_from_gh_output_rejects_malformed_json() {
    assert!(super::repository_info_from_gh_output("not json").is_err());
}

#[cfg(all(feature = "local_fs", unix))]
#[tokio::test]
async fn get_repository_info_reads_gh_repo_view() {
    let (_dir, repo) = init_repo().await;
    let (_fake_bin, path_env) = fake_gh(
        "#!/bin/sh\nprintf '{\"name\":\"warp-internal\",\"owner\":{\"login\":\"warpdotdev\"}}\\n'\n",
    );

    assert_eq!(
        super::get_repository_info(&repo, Some(&path_env))
            .await
            .unwrap(),
        Some(RepositoryInfo {
            name: "warp-internal".to_owned(),
            owner: Some("warpdotdev".to_owned()),
            host: None,
        })
    );
}

#[cfg(all(feature = "local_fs", unix))]
#[tokio::test]
async fn get_repository_info_returns_none_when_gh_cannot_resolve_github_repo() {
    let (_dir, repo) = init_repo().await;
    let (_fake_bin, path_env) = fake_gh(
        "#!/bin/sh\nprintf 'none of the git remotes configured for this repository point to a known GitHub host\\n' >&2\nexit 1\n",
    );

    assert_eq!(
        super::get_repository_info(&repo, Some(&path_env))
            .await
            .unwrap(),
        None
    );
}

#[test]
fn detects_missing_gh_errors() {
    assert!(is_gh_missing_error(
        "Failed to execute gh command: No such file or directory (os error 2)"
    ));
    assert!(is_gh_missing_error(
        "Failed to execute gh command: program not found"
    ));

    assert!(!is_gh_missing_error(
        "gh command failed: GraphQL: authentication required; run gh auth login"
    ));
    assert!(!is_gh_missing_error(
        "Post \"https://api.github.com/graphql\": dial tcp: lookup api.github.com: no such host"
    ));
}

#[cfg(feature = "local_fs")]
#[test]
fn detects_no_pr_for_branch_errors() {
    assert!(super::is_no_pr_for_branch_error(
        "gh command failed: no pull requests found for branch \"feature-a\""
    ));
    assert!(super::is_no_pr_for_branch_error(
        "gh command failed: no open pull requests found for branch \"feature-a\""
    ));
    assert!(super::is_no_pr_for_branch_error(
        "GraphQL: NO OPEN PULL REQUESTS FOUND FOR BRANCH feature-a"
    ));
    assert!(!super::is_no_pr_for_branch_error("authentication required"));
    assert!(!super::is_no_pr_for_branch_error("repository not found"));
}

#[cfg(feature = "local_fs")]
#[test]
fn detects_repository_lookup_not_applicable_errors() {
    assert!(super::is_repository_lookup_not_applicable_error(
        "gh command failed: none of the git remotes configured for this repository point to a known GitHub host"
    ));
    assert!(super::is_repository_lookup_not_applicable_error(
        "gh command failed: no GitHub remotes"
    ));
    assert!(super::is_repository_lookup_not_applicable_error(
        "gh command failed: not a GitHub repository"
    ));
    assert!(!super::is_repository_lookup_not_applicable_error(
        "authentication required"
    ));
    assert!(!super::is_repository_lookup_not_applicable_error(
        "repository not found"
    ));
}
