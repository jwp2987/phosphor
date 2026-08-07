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
