use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use command::r#async::Command;
use command::Stdio;
use tempfile::TempDir;
use super::*;

#[test]
fn test_parse_range_with_comma() {
    let (start, count) = LocalDiffStateModel::parse_range("10,5")
        .expect("parse_range should succeed for range with count");
    assert_eq!(start, 10);
    assert_eq!(count, 5);
}

#[test]
fn test_parse_range_without_comma() {
    let (start, count) = LocalDiffStateModel::parse_range("10")
        .expect("parse_range should succeed for range without count");
    assert_eq!(start, 10);
    assert_eq!(count, 1);
}

#[test]
fn test_parse_unified_diff_header_basic() {
    let header = "@@ -10,5 +12,7 @@";
    let parsed = LocalDiffStateModel::parse_unified_diff_header(header)
        .expect("parse_unified_diff_header should succeed for basic header");
    assert_eq!(parsed.old_start_line, 10);
    assert_eq!(parsed.old_line_count, 5);
    assert_eq!(parsed.new_start_line, 12);
    assert_eq!(parsed.new_line_count, 7);
}

#[test]
fn test_parse_unified_diff_header_with_context() {
    let header = "@@ -4978,33 +4978,43 @@ impl TerminalView {";
    let parsed = LocalDiffStateModel::parse_unified_diff_header(header)
        .expect("parse_unified_diff_header should succeed for header with context");
    assert_eq!(parsed.old_start_line, 4978);
    assert_eq!(parsed.old_line_count, 33);
    assert_eq!(parsed.new_start_line, 4978);
    assert_eq!(parsed.new_line_count, 43);
}

#[test]
fn test_parse_unified_diff_header_single_line() {
    let header = "@@ -10 +12,3 @@";
    let parsed = LocalDiffStateModel::parse_unified_diff_header(header)
        .expect("parse_unified_diff_header should succeed for single line header");
    assert_eq!(parsed.old_start_line, 10);
    assert_eq!(parsed.old_line_count, 1);
    assert_eq!(parsed.new_start_line, 12);
    assert_eq!(parsed.new_line_count, 3);
}

#[test]
fn test_sort_branches_main_first_empty() {
    let branches: Vec<(String, bool)> = vec![];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches).collect();
    assert!(result.is_empty());
}

#[test]
fn test_sort_branches_main_first_no_main() {
    let branches = vec![
        ("feature-a".to_string(), false),
        ("feature-b".to_string(), false),
        ("feature-c".to_string(), false),
    ];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches).collect();
    // No main branches — order should be unchanged.
    assert_eq!(result, branches.iter().collect::<Vec<_>>());
}

#[test]
fn test_sort_branches_main_first_promotes_main() {
    let branches = vec![
        ("feature-a".to_string(), false),
        ("main".to_string(), true),
        ("feature-b".to_string(), false),
    ];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(result, vec!["main", "feature-a", "feature-b"]);
}

#[test]
fn test_sort_branches_main_first_main_already_first() {
    let branches = vec![
        ("main".to_string(), true),
        ("feature-a".to_string(), false),
        ("feature-b".to_string(), false),
    ];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(result, vec!["main", "feature-a", "feature-b"]);
}

#[test]
fn test_sort_branches_main_first_preserves_recency_order_for_non_main() {
    // Non-main branches should remain in their original (recency) order.
    let branches = vec![
        ("recent-feature".to_string(), false),
        ("main".to_string(), true),
        ("older-feature".to_string(), false),
        ("oldest-feature".to_string(), false),
    ];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches)
        .map(|(name, _)| name.as_str())
        .collect();
    assert_eq!(
        result,
        vec!["main", "recent-feature", "older-feature", "oldest-feature"]
    );
}

#[test]
fn test_sort_branches_main_first_multiple_main_flags() {
    // Defensive: both flagged as main (shouldn't happen in practice, but
    // sort_branches_main_first should handle it gracefully).
    let branches = vec![
        ("feature".to_string(), false),
        ("main".to_string(), true),
        ("master".to_string(), true),
    ];
    let result: Vec<_> = LocalDiffStateModel::sort_branches_main_first(&branches)
        .map(|(name, _)| name.as_str())
        .collect();
    // Both main-flagged entries appear first, non-main last.
    assert_eq!(result, vec!["main", "master", "feature"]);
}

#[test]
fn test_parse_unified_diff_header_malformed() {
    let header = "not a diff header";
    let result = LocalDiffStateModel::parse_unified_diff_header(header);
    assert!(result.is_err());

    let header2 = "@@ incomplete";
    let result2 = LocalDiffStateModel::parse_unified_diff_header(header2);
    assert!(result2.is_err());
}

#[test]
fn test_parse_git_status_modified_file_with_spaces() {
    // Porcelain v2 output for a modified file with spaces in the name.
    // Format: 1 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <path>
    let status_output = "1 .M N... 100644 100644 100644 abc1234 def5678 test file.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, std::path::PathBuf::from("test file.txt"));
    assert_eq!(result[0].1, GitFileStatus::Modified);
}

#[test]
fn test_parse_git_status_modified_file_with_multiple_spaces() {
    // Filename with multiple spaces.
    let status_output = "1 .M N... 100644 100644 100644 abc1234 def5678 path to/my test file.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].0,
        std::path::PathBuf::from("path to/my test file.txt")
    );
    assert_eq!(result[0].1, GitFileStatus::Modified);
}

#[test]
fn test_parse_git_status_new_file_with_spaces() {
    let status_output = "1 A. N... 000000 100644 100644 0000000 abc1234 new file name.rs";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, std::path::PathBuf::from("new file name.rs"));
    assert_eq!(result[0].1, GitFileStatus::New);
}

#[test]
fn test_parse_git_status_renamed_file_with_spaces() {
    // Porcelain v2 renamed entry (type 2) with spaces in the new path.
    // Format: 2 <XY> <sub> <mH> <mI> <mW> <hH> <hI> <X><score> <path>\0<origPath>
    let status_output =
        "2 R. N... 100644 100644 100644 abc1234 def5678 R100 new name.txt\0old name.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, std::path::PathBuf::from("new name.txt"));
    assert!(matches!(
        &result[0].1,
        GitFileStatus::Renamed { old_path } if old_path == "old name.txt"
    ));
}

#[test]
fn test_parse_git_status_untracked_file_with_spaces() {
    let status_output = "? my untracked file.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(
        result[0].0,
        std::path::PathBuf::from("my untracked file.txt")
    );
    assert_eq!(result[0].1, GitFileStatus::Untracked);
}

#[test]
fn test_parse_git_status_unmerged_file_with_spaces() {
    // Porcelain v2 unmerged entry (type u) with spaces in the path.
    // Format: u <xy> <sub> <m1> <m2> <m3> <mW> <h1> <h2> <h3> <path>
    let status_output =
        "u UU N... 100644 100644 100644 100644 abc1234 def5678 ghi9012 conflict file.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, std::path::PathBuf::from("conflict file.txt"));
    assert_eq!(result[0].1, GitFileStatus::Conflicted);
}

#[test]
fn test_parse_git_status_mixed_entries_with_spaces() {
    // Multiple entries separated by NUL, mixing files with and without spaces.
    let status_output = "1 .M N... 100644 100644 100644 abc1234 def5678 test file.txt\0\
         1 .M N... 100644 100644 100644 abc1234 def5678 normal.txt\0\
         ? another file with spaces.rs";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 3);
    assert_eq!(result[0].0, std::path::PathBuf::from("test file.txt"));
    assert_eq!(result[1].0, std::path::PathBuf::from("normal.txt"));
    assert_eq!(
        result[2].0,
        std::path::PathBuf::from("another file with spaces.rs")
    );
}

#[test]
fn test_parse_git_status_file_without_spaces_still_works() {
    // Ensure the splitn change doesn't break files without spaces.
    let status_output = "1 .M N... 100644 100644 100644 abc1234 def5678 simple.txt";
    let result = LocalDiffStateModel::parse_git_status(status_output).unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].0, std::path::PathBuf::from("simple.txt"));
    assert_eq!(result[0].1, GitFileStatus::Modified);
}

// ─── Ported from Warp: `warp/master:app/src/util/git_tests.rs` ───────────────
//
// Warp keeps `committed_branch_files_excludes_uncommitted_and_untracked` next
// to `util::git::get_committed_branch_file_entries`. The fork carries the same
// implementation as `LocalDiffStateModel::get_committed_branch_file_entries`,
// returning `(path, additions, deletions)` tuples instead of Warp's
// `FileChangeEntry` structs, so the test lives here and reads the tuple fields
// positionally. Only that shape changed — the assertions are Warp's.

/// Runs a git command inside `repo`, panicking on failure.
#[cfg(feature = "local_fs")]
async fn run_git(repo: &std::path::Path, args: &[&str]) -> String {
    run_git_command(repo, args)
        .await
        .unwrap_or_else(|e| panic!("git {args:?} failed: {e}"))
}

/// Creates a temp git repo with one commit and returns `(dir_handle, repo_path)`.
#[cfg(feature = "local_fs")]
async fn init_test_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().to_path_buf();

    run_git(&path, &["init", "-b", "main"]).await;
    run_git(&path, &["config", "user.email", "test@test.com"]).await;
    run_git(&path, &["config", "user.name", "Test"]).await;
    run_git(&path, &["commit", "--allow-empty", "-m", "initial"]).await;

    (dir, path)
}

#[cfg(feature = "local_fs")]
#[tokio::test]
async fn committed_branch_files_excludes_uncommitted_and_untracked() {
    let (_dir, repo) = init_test_repo().await;
    // Branch off main; the merge base is main's initial commit.
    run_git(&repo, &["checkout", "-b", "feature"]).await;

    // Commit a new file on the feature branch — this SHOULD appear in the
    // committed branch diff.
    std::fs::write(repo.join("committed.txt"), "line1\nline2\n").expect("write committed.txt");
    run_git(&repo, &["add", "committed.txt"]).await;
    run_git(&repo, &["commit", "-m", "add committed.txt"]).await;

    // Further-modify the committed file in the working tree (uncommitted) and
    // add an untracked file. Neither is part of the PR's committed history, so
    // neither should appear, and the committed file's counts must reflect only
    // the committed change (2 added lines, not 3).
    std::fs::write(repo.join("committed.txt"), "line1\nline2\nline3\n")
        .expect("modify committed.txt");
    std::fs::write(repo.join("untracked.txt"), "new\n").expect("write untracked.txt");

    let entries = LocalDiffStateModel::get_committed_branch_file_entries(&repo)
        .await
        .expect("committed branch files");

    assert_eq!(
        entries.len(),
        1,
        "expected only the committed file: {entries:?}"
    );
    assert_eq!(entries[0].0, "committed.txt");
    assert_eq!(entries[0].1, 2);
    assert_eq!(entries[0].2, 0);
    assert!(
        !entries.iter().any(|e| e.0 == "untracked.txt"),
        "untracked files must be excluded: {entries:?}"
    );
}

// ── Branch listing ───────────────────────────────────────────────
// Coverage for the branch dropdown's data source: the local backend's git
// listing, and the wrapper's dispatch/forwarding that delivers the result to
// the code-review view for both backends.

/// Runs a git command inside `repo` and returns its trimmed stdout.
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

/// Creates a temp repo on `main` with two extra branches, back on `main`.
async fn init_repo_with_branches() -> (TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("failed to create temp dir");
    let path = dir.path().to_path_buf();

    git(&path, &["init", "-b", "main"]).await;
    git(&path, &["config", "user.email", "test@test.com"]).await;
    git(&path, &["config", "user.name", "Test"]).await;
    git(&path, &["commit", "--allow-empty", "-m", "initial"]).await;
    git(&path, &["checkout", "-b", "feature/one"]).await;
    git(&path, &["checkout", "-b", "feature/two"]).await;
    git(&path, &["checkout", "main"]).await;

    (dir, path)
}

#[tokio::test]
async fn get_all_branches_lists_local_branches_with_main_flagged() {
    let (_dir, repo) = init_repo_with_branches().await;

    let branches =
        LocalDiffStateModel::get_all_branches(&repo, None, false /* include_remotes */)
            .await
            .expect("get_all_branches should succeed for a valid repo");

    let names: Vec<&str> = branches.iter().map(|(name, _)| name.as_str()).collect();
    assert!(names.contains(&"main"), "expected main in {names:?}");
    assert!(
        names.contains(&"feature/one"),
        "expected feature/one in {names:?}"
    );
    assert!(
        names.contains(&"feature/two"),
        "expected feature/two in {names:?}"
    );

    let main_flags: Vec<bool> = branches
        .iter()
        .filter(|(name, _)| name == "main")
        .map(|(_, is_main)| *is_main)
        .collect();
    assert_eq!(main_flags, vec![true]);
    assert!(branches
        .iter()
        .filter(|(name, _)| name.starts_with("feature/"))
        .all(|(_, is_main)| !*is_main));
}

#[tokio::test]
async fn get_all_branches_excludes_remote_tracking_branches_by_default() {
    let (_dir, repo) = init_repo_with_branches().await;
    // Fabricate a remote-tracking ref so the `include_remotes` flag is
    // observable without a real remote.
    let head = git(&repo, &["rev-parse", "HEAD"]).await;
    git(&repo, &["update-ref", "refs/remotes/origin/main", &head]).await;

    let local_only = LocalDiffStateModel::get_all_branches(&repo, None, false)
        .await
        .expect("get_all_branches should succeed for a valid repo");
    assert!(local_only
        .iter()
        .all(|(name, _)| !name.starts_with("origin/")));

    let with_remotes = LocalDiffStateModel::get_all_branches(&repo, None, true)
        .await
        .expect("get_all_branches should succeed for a valid repo");
    assert!(with_remotes.iter().any(|(name, _)| name == "origin/main"));
}

/// Collects every `BranchesReceived` payload emitted by `handle`.
fn subscribe_to_branches(
    app: &mut warpui::App,
    handle: &ModelHandle<DiffStateModel>,
) -> Arc<Mutex<Vec<Vec<(String, bool)>>>> {
    let received = Arc::new(Mutex::new(Vec::new()));
    let received_for_subscription = received.clone();
    app.update(|ctx| {
        ctx.subscribe_to_model(handle, move |_, event, _| {
            if let DiffStateModelEvent::BranchesReceived(branches) = event {
                received_for_subscription
                    .lock()
                    .expect("branches mutex should not be poisoned")
                    .push(branches.clone());
            }
        });
    });
    received
}

#[test]
fn wrapper_forwards_branches_received_from_the_local_backend() {
    warpui::App::test((), |mut app| async move {
        let wrapper = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let received = subscribe_to_branches(&mut app, &wrapper);

        let local = wrapper.read(&app, |model, _| match model {
            DiffStateModel::Local(handle) => handle.clone(),
            #[cfg(not(target_family = "wasm"))]
            DiffStateModel::Remote(_) => {
                panic!("DiffStateModel::new should build a local backend")
            }
        });
        local.update(&mut app, |_, ctx| {
            ctx.emit(DiffStateModelEvent::BranchesReceived(vec![
                ("main".to_string(), true),
                ("feature/one".to_string(), false),
            ]));
        });

        let received = received
            .lock()
            .expect("branches mutex should not be poisoned");
        assert_eq!(
            *received,
            vec![vec![
                ("main".to_string(), true),
                ("feature/one".to_string(), false),
            ]]
        );
    });
}

#[test]
fn fetch_branches_on_local_backend_without_a_repository_emits_nothing() {
    warpui::App::test((), |mut app| async move {
        let wrapper = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let received = subscribe_to_branches(&mut app, &wrapper);

        wrapper.update(&mut app, |model, ctx| model.fetch_branches(ctx));

        let received = received
            .lock()
            .expect("branches mutex should not be poisoned");
        assert!(received.is_empty());
    });
}

// ── Ported from the pinned oracle
// (warp/master:app/src/code_review/diff_state/local_tests.rs @ 02b53fcd8) ──

#[tokio::test]
async fn untracked_directory_diff_is_empty_and_non_binary() {
    let repo_dir = tempfile::tempdir().expect("create temp repo dir");
    std::fs::create_dir(repo_dir.path().join("nested-repo")).expect("create nested dir");

    // `git status` reports a nested repo/worktree as a single untracked
    // directory entry (with a trailing slash). It must short-circuit to an
    // empty non-binary diff — the error fallback would otherwise mislabel it
    // as binary and the view would render "Binary file - no diff available"
    // instead of "New empty file".
    let diff = LocalDiffStateModel::get_file_diff(
        repo_dir.path(),
        &PathBuf::from("nested-repo/"),
        &GitFileStatus::Untracked,
        false,
        None,
    )
    .await
    .expect("get_file_diff should succeed for an untracked directory");

    assert!(!diff.is_binary);
    assert_eq!(diff.hunks.len(), 0);
    assert_eq!(diff.status, GitFileStatus::Untracked);
}

#[tokio::test]
async fn untracked_directory_has_no_baseline_content() {
    let repo_dir = tempfile::tempdir().expect("create temp repo dir");
    std::fs::create_dir(repo_dir.path().join("nested-repo")).expect("create nested dir");
    std::fs::write(repo_dir.path().join("new-file.txt"), "hello\n").expect("write file");

    // No baseline for a directory entry, so no editor is constructed for it.
    let dir_content = LocalDiffStateModel::get_file_content_at_head(
        repo_dir.path(),
        Path::new("nested-repo/"),
        &GitFileStatus::Untracked,
    )
    .await;
    assert_eq!(dir_content, None);

    // Regular untracked files keep their empty baseline.
    let file_content = LocalDiffStateModel::get_file_content_at_head(
        repo_dir.path(),
        Path::new("new-file.txt"),
        &GitFileStatus::Untracked,
    )
    .await;
    assert_eq!(file_content, Some(String::new()));
}

#[tokio::test]
async fn renamed_file_content_at_head_reads_old_path() {
    let repo_dir = tempfile::tempdir().expect("create temp repo dir");
    let repo_path = repo_dir.path();

    // Set up a real git repo with one committed file, then rename it in the working tree
    // (without committing the rename) so HEAD only knows about the old path.
    run_git_command(repo_path, &["init", "-b", "main"])
        .await
        .expect("git init");
    run_git_command(repo_path, &["config", "user.email", "test@test.com"])
        .await
        .expect("git config email");
    run_git_command(repo_path, &["config", "user.name", "Test"])
        .await
        .expect("git config name");
    std::fs::write(repo_path.join("old.txt"), "hello world\n").expect("write old.txt");
    run_git_command(repo_path, &["add", "old.txt"])
        .await
        .expect("git add");
    run_git_command(repo_path, &["commit", "-m", "initial"])
        .await
        .expect("git commit");

    // Rename in the working tree only — `old.txt` no longer exists at this path, so `git
    // show HEAD:new.txt` would fail.
    std::fs::rename(repo_path.join("old.txt"), repo_path.join("new.txt"))
        .expect("rename old.txt to new.txt");

    let content = LocalDiffStateModel::get_file_content_at_head(
        repo_path,
        Path::new("new.txt"),
        &GitFileStatus::Renamed {
            old_path: "old.txt".to_string(),
        },
    )
    .await;

    // The baseline content at HEAD must come from the old path, not the new one, so the code
    // review pane can render a diff instead of "Unable to load file content".
    assert_eq!(content, Some("hello world\n".to_string()));
}

#[tokio::test]
async fn staged_rename_and_modify_produces_non_empty_diff() {
    let repo_dir = tempfile::tempdir().expect("create temp repo dir");
    let repo_path = repo_dir.path();

    run_git_command(repo_path, &["init", "-b", "main"])
        .await
        .expect("git init");
    run_git_command(repo_path, &["config", "user.email", "test@test.com"])
        .await
        .expect("git config email");
    run_git_command(repo_path, &["config", "user.name", "Test"])
        .await
        .expect("git config name");
    std::fs::write(
        repo_path.join("old.txt"),
        "line one\nline two\nline three\n",
    )
    .expect("write old.txt");
    run_git_command(repo_path, &["add", "old.txt"])
        .await
        .expect("git add");
    run_git_command(repo_path, &["commit", "-m", "initial"])
        .await
        .expect("git commit");

    // Stage both the rename and a content edit, so nothing is left unstaged (git status
    // reports this as a plain "R " entry with no unstaged component).
    run_git_command(repo_path, &["mv", "old.txt", "new.txt"])
        .await
        .expect("git mv");
    std::fs::write(
        repo_path.join("new.txt"),
        "line one\nline two changed\nline three\n",
    )
    .expect("write new.txt");
    run_git_command(repo_path, &["add", "new.txt"])
        .await
        .expect("git add new.txt");

    let diff = LocalDiffStateModel::get_file_diff(
        repo_path,
        &PathBuf::from("new.txt"),
        &GitFileStatus::Renamed {
            old_path: "old.txt".to_string(),
        },
        false,
        None,
    )
    .await
    .expect("get_file_diff should succeed for a fully staged rename+modify");

    // A fully staged rename with a staged content edit must still render an inline diff
    // instead of falling through to "File renamed without changes": comparing only the
    // index against the working tree (as before the fix) produced an empty diff here,
    // since both changes were already staged.
    assert!(
        !diff.is_empty(),
        "expected a non-empty diff for a fully staged rename+modify"
    );
}

#[tokio::test]
async fn num_lines_in_file_if_non_binary_counts_lines_in_text_file() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let file_path = dir.path().join("file.txt");
    std::fs::write(&file_path, "one\ntwo\nthree\n").expect("write file");

    let num_lines = LocalDiffStateModel::num_lines_in_file_if_non_binary(&file_path)
        .await
        .expect("counting a regular file should succeed");
    assert_eq!(num_lines, Some(3));
}

#[tokio::test]
async fn num_lines_in_file_if_non_binary_errors_for_directory() {
    let dir = tempfile::tempdir().expect("create temp dir");

    // Directories aren't countable. The metadata callers degrade this error
    // to a 0-line contribution per entry instead of failing the whole
    // metadata computation.
    let result = LocalDiffStateModel::num_lines_in_file_if_non_binary(dir.path()).await;
    assert!(result.is_err());
}

// ── Ported from the pinned oracle
// (warp/master:app/src/code_review/diff_state/mod_tests.rs @ 02b53fcd8) ──
//
// The pin builds these against `DiffStateModel::new_for_test`, a dedicated
// no-watcher test constructor. The fork has no such helper, but
// `DiffStateModel::new(None, ctx)` already produces the same no-repository
// local-backed wrapper (repo_path: None short-circuits before any watcher /
// `DetectedRepositories` lookup is spawned), matching the pattern the fork's
// own `fetch_branches_on_local_backend_without_a_repository_emits_nothing`
// test above already relies on.

#[test]
fn new_creates_local_variant() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        handle.read(&app, |model, _ctx| {
            assert!(matches!(model, DiffStateModel::Local(_)));
        });
    });
}

#[test]
fn get_returns_not_in_repository_for_test_model() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let state = handle.read(&app, |model, ctx| model.get(ctx));
        assert!(matches!(state, DiffState::NotInRepository));
    });
}

#[test]
fn diff_mode_defaults_to_head() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let mode = handle.read(&app, |model, ctx| model.diff_mode(ctx));
        assert!(matches!(mode, DiffMode::Head));
    });
}

#[test]
fn has_head_false_for_test_model() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let has_head = handle.read(&app, |model, ctx| model.has_head(ctx));
        assert!(!has_head);
    });
}

#[test]
fn branch_info_none_for_test_model() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        handle.read(&app, |model, ctx| {
            assert_eq!(model.get_main_branch_name(ctx), None);
            assert_eq!(model.get_current_branch_name(ctx), None);
            assert!(!model.is_on_main_branch(ctx));
            assert!(model.unpushed_commits(ctx).is_empty());
            assert_eq!(model.upstream_ref(ctx), None);
            assert!(!model.upstream_differs_from_main(ctx));
            assert!(!model.is_git_operation_blocked(ctx));
        });
    });
}

#[test]
fn uncommitted_stats_none_for_test_model() {
    warpui::App::test((), |mut app| async move {
        let handle = app.add_model(|ctx| DiffStateModel::new(None, ctx));
        let stats = handle.read(&app, |model, ctx| model.get_uncommitted_stats(ctx));
        assert!(stats.is_none());
    });
}

// ── classify_repository_update ───────────────────────────────────────────────
//
// These rules had no direct coverage while `LocalDiffStateModel` was their only
// consumer. The remote-server daemon is now a second consumer (#577), and it
// depends on the `Files` case specifically — that is what lets it push a
// per-file `DiffStateFileDelta` instead of the whole repo. A silent change to
// any rule below would either send wrong deltas or quietly collapse every
// change back to a full snapshot, so pin them.

#[cfg(feature = "local_fs")]
fn target(path: &str, is_ignored: bool) -> repo_metadata::watcher::TargetFile {
    repo_metadata::watcher::TargetFile::new(PathBuf::from(path), is_ignored)
}

#[cfg(feature = "local_fs")]
fn update_with_modified(
    files: Vec<repo_metadata::watcher::TargetFile>,
) -> repo_metadata::RepositoryUpdate {
    repo_metadata::RepositoryUpdate {
        modified: files.into_iter().collect(),
        ..Default::default()
    }
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_reports_plain_edits_per_file() {
    let behavior = classify_repository_update(update_with_modified(vec![
        target("/repo/src/main.rs", false),
        target("/repo/src/lib.rs", false),
    ]))
    .expect("a non-ignored edit is a change worth reacting to");

    let InvalidationBehavior::Files(mut files) = behavior else {
        panic!("plain file edits must stay per-file, not escalate to a full reload");
    };
    files.sort();
    assert_eq!(
        files,
        vec![
            PathBuf::from("/repo/src/lib.rs"),
            PathBuf::from("/repo/src/main.rs")
        ]
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_ignores_gitignored_files() {
    // Every changed file is ignored, so there is nothing to invalidate — not an
    // empty `Files` list, which would still cost a round of per-file diffs.
    assert!(
        classify_repository_update(update_with_modified(vec![
            target("/repo/target/debug/build", true),
            target("/repo/node_modules/x", true),
        ]))
        .is_none()
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_escalates_a_touched_gitignore_to_full_reload() {
    // A changed .gitignore can change which files belong in the diff at all, so
    // per-file invalidation of just that path would leave the rest stale.
    let behavior = classify_repository_update(update_with_modified(vec![target(
        "/repo/.gitignore",
        false,
    )]))
    .expect("a .gitignore edit is a change");

    assert!(
        matches!(
            behavior,
            InvalidationBehavior::All(InvalidationSource::MetadataChange)
        ),
        "a touched .gitignore must force a full reload, got {behavior:?}"
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_treats_commit_and_remote_ref_moves_as_metadata() {
    // #294: a tracked-remote-ref move is metadata (it shifts ahead/behind
    // counts), not file content, so it takes the same path as a commit.
    for update in [
        repo_metadata::RepositoryUpdate {
            commit_updated: true,
            ..Default::default()
        },
        repo_metadata::RepositoryUpdate {
            remote_ref_updated: true,
            ..Default::default()
        },
    ] {
        let behavior = classify_repository_update(update).expect("ref moves are changes");
        assert!(
            matches!(
                behavior,
                InvalidationBehavior::All(InvalidationSource::MetadataChange)
            ),
            "ref moves must be metadata invalidations, got {behavior:?}"
        );
    }
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_flags_a_locked_index_distinctly() {
    // The index lock gets its own source so consumers can decline to reload
    // against half-written state rather than treating it as a normal change.
    let behavior = repo_metadata::RepositoryUpdate {
        index_lock_detected: true,
        ..Default::default()
    };
    let behavior = classify_repository_update(behavior).expect("a lock change is a change");
    assert!(
        matches!(
            behavior,
            InvalidationBehavior::All(InvalidationSource::IndexLockChange)
        ),
        "a locked index must be distinguishable from an ordinary metadata change, got {behavior:?}"
    );
}

#[cfg(feature = "local_fs")]
#[test]
fn classify_repository_update_returns_none_for_an_empty_update() {
    assert!(classify_repository_update(repo_metadata::RepositoryUpdate::default()).is_none());
}

fn test_line(line_type: DiffLineType, text: &str) -> DiffLine {
    DiffLine {
        line_type,
        old_line_number: None,
        new_line_number: None,
        text: text.to_string(),
        no_trailing_newline: false,
    }
}

fn test_hunk(lines: Vec<DiffLine>) -> DiffHunk {
    DiffHunk {
        old_start_line: 10,
        old_line_count: 3,
        new_start_line: 10,
        new_line_count: 4,
        lines,
        unified_diff_start: 0,
        unified_diff_end: 0,
    }
}

#[test]
fn hunk_to_patch_emits_an_appliable_unified_diff() {
    let hunk = test_hunk(vec![
        test_line(DiffLineType::Context, "unchanged"),
        test_line(DiffLineType::Delete, "gone"),
        test_line(DiffLineType::Add, "added"),
        test_line(DiffLineType::Add, "also added"),
    ]);
    let patch = hunk_to_patch(Path::new("src/main.rs"), &hunk);

    assert_eq!(
        patch,
        "diff --git a/src/main.rs b/src/main.rs\n\
         --- a/src/main.rs\n\
         +++ b/src/main.rs\n\
         @@ -10,3 +10,4 @@\n\
         \x20unchanged\n\
         -gone\n\
         +added\n\
         +also added\n",
        "patch must be a self-contained unified diff git apply can consume"
    );
}

#[test]
fn hunk_to_patch_skips_parsed_hunk_header_lines() {
    // A parsed DiffHunk can carry the `@@` line as a body line. The header is
    // rebuilt from the hunk's own counts, so emitting it again would produce a
    // patch with two headers and git would reject it.
    let hunk = test_hunk(vec![
        test_line(DiffLineType::HunkHeader, "@@ -10,3 +10,4 @@"),
        test_line(DiffLineType::Context, "kept"),
    ]);
    let patch = hunk_to_patch(Path::new("a.txt"), &hunk);

    assert_eq!(
        patch.matches("@@").count(),
        2,
        "exactly one @@ header line (two @@ tokens), not the parsed one as well"
    );
    assert!(patch.ends_with(" kept\n"));
}

#[test]
fn hunk_to_patch_preserves_missing_trailing_newline() {
    // Without git's `\ No newline at end of file` marker, applying the patch
    // silently appends a newline the user never wrote.
    let mut line = test_line(DiffLineType::Add, "no newline here");
    line.no_trailing_newline = true;
    let patch = hunk_to_patch(Path::new("a.txt"), &test_hunk(vec![line]));

    assert!(
        patch.contains("\\ No newline at end of file"),
        "missing-newline marker must survive into the patch, got:\n{patch}"
    );
}
