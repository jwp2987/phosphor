use crate::proto::{get_branches_response, get_committed_branch_files_response};

use super::*;

// The response-builder tests below are the app daemon's `get_branches_tests.rs` and
// `get_committed_branch_files_tests.rs`, ported with the builders.

#[test]
fn success_response_maps_name_and_is_main() {
    let response = branches_success_response(vec![
        ("main".to_string(), true),
        ("feature/x".to_string(), false),
    ]);

    let Some(get_branches_response::Result::Success(success)) = response.result else {
        panic!("expected success");
    };
    assert_eq!(success.branches.len(), 2);
    assert_eq!(success.branches[0].name, "main");
    assert!(success.branches[0].is_main);
    assert_eq!(success.branches[1].name, "feature/x");
    assert!(!success.branches[1].is_main);
}

#[test]
fn error_response_carries_message() {
    let response = branches_error_response("boom".to_string());

    let Some(get_branches_response::Result::Error(err)) = response.result else {
        panic!("expected error");
    };
    assert_eq!(err.message, "boom");
}

#[test]
fn branches_result_to_response_maps_ok_and_err() {
    let ok = branches_result_to_response(Ok(vec![("dev".to_string(), false)]));
    assert!(matches!(
        ok.result,
        Some(get_branches_response::Result::Success(_))
    ));

    let err = branches_result_to_response(Err(anyhow::anyhow!("nope")));
    let Some(get_branches_response::Result::Error(e)) = err.result else {
        panic!("expected error");
    };
    assert_eq!(e.message, "nope");
}

#[test]
fn files_success_response_maps_path_and_counts() {
    let response = files_success_response(vec![
        ("src/main.rs".to_string(), 10, 2),
        ("README.md".to_string(), 0, 5),
    ]);

    let Some(get_committed_branch_files_response::Result::Success(success)) = response.result
    else {
        panic!("expected success");
    };
    assert_eq!(success.files.len(), 2);
    assert_eq!(success.files[0].path, "src/main.rs");
    assert_eq!(success.files[0].additions, 10);
    assert_eq!(success.files[0].deletions, 2);
    assert_eq!(success.files[1].path, "README.md");
    assert_eq!(success.files[1].deletions, 5);
}

#[test]
fn files_result_to_response_maps_ok_and_err() {
    let ok = files_result_to_response(Ok(vec![("a.txt".to_string(), 1, 1)]));
    assert!(matches!(
        ok.result,
        Some(get_committed_branch_files_response::Result::Success(_))
    ));

    let err = files_result_to_response(Err(anyhow::anyhow!("nope")));
    let Some(get_committed_branch_files_response::Result::Error(e)) = err.result else {
        panic!("expected error");
    };
    assert_eq!(e.message, "nope");
}

#[test]
fn branch_list_skips_head_and_detached_and_dedups() {
    let out = "feature/x\norigin/HEAD\n(HEAD detached at abc)\nmain\n\nfeature/x\norigin/main\n";
    let parsed = parse_branch_list(out, "origin/main");
    assert_eq!(
        parsed,
        vec![
            ("feature/x".to_string(), false),
            ("main".to_string(), true),
            ("origin/main".to_string(), true),
        ]
    );
}

#[test]
fn numstat_parses_counts_binary_files_and_short_lines() {
    let out = "10\t2\tsrc/a.rs\n-\t-\timg.png\n\nbroken line\n3\t0\tdir/with space.txt\n";
    assert_eq!(
        parse_numstat(out),
        vec![
            ("src/a.rs".to_string(), 10, 2),
            ("img.png".to_string(), 0, 0),
            ("dir/with space.txt".to_string(), 3, 0),
        ]
    );
}

fn git_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Against a real repository: branches list newest first with main marked, and the committed
/// diff against the main branch lists the file the feature branch added.
#[test]
fn real_repository_branches_and_committed_files() {
    if !git_available() {
        eprintln!("git not installed; skipping");
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let git = |args: &[&str]| {
        let out = Command::new("git")
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {out:?}");
    };
    git(&["init", "-q", "-b", "main"]);
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "first"]);
    git(&["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("b.txt"), "x\ny\n").unwrap();
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "second"]);

    let response = handle_get_branches(GetBranches {
        repo_path: repo.to_string_lossy().into_owned(),
        max_branch_count: None,
        include_remotes: false,
    });
    let Some(get_branches_response::Result::Success(success)) = response.result else {
        panic!("expected success: {response:?}");
    };
    let main = success.branches.iter().find(|b| b.name == "main").unwrap();
    assert!(main.is_main);
    assert!(
        success
            .branches
            .iter()
            .any(|b| b.name == "feature" && !b.is_main)
    );

    let response = handle_get_committed_branch_files(GetCommittedBranchFilesRequest {
        repo_path: repo.to_string_lossy().into_owned(),
    });
    let Some(get_committed_branch_files_response::Result::Success(success)) = response.result
    else {
        panic!("expected success: {response:?}");
    };
    assert_eq!(success.files.len(), 1);
    assert_eq!(success.files[0].path, "b.txt");
    assert_eq!(success.files[0].additions, 2);
}

#[test]
fn a_missing_repo_path_is_an_invalid_repo_path_error() {
    let response = handle_get_branches(GetBranches {
        repo_path: "/definitely/not/here".to_string(),
        max_branch_count: None,
        include_remotes: false,
    });
    let Some(get_branches_response::Result::Error(e)) = response.result else {
        panic!("expected error");
    };
    assert!(e.message.contains("Invalid repo_path"), "{}", e.message);
}
