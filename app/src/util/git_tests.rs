use std::path::Path;

use command::r#async::Command;
use command::Stdio;
use tempfile::TempDir;

use super::{
    compute_unpushed_state, detect_current_branch, detect_current_branch_display,
    run_commit_chain, CommitChainMode,
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
