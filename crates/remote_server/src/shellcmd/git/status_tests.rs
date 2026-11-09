use futures_lite::future::block_on;

use super::*;
use crate::shellcmd::git::test_runner::{StdRunner, git, git_available, repo_with};

#[test]
fn chip_stats_count_tracked_lines_and_untracked_files_as_files_only() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n2\n3\n")]);
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "1\n2\n3\n4\n5\n").unwrap();
    std::fs::write(root.join("u.txt"), "u\nu\nu\nu\n").unwrap();
    let stats = block_on(chip_stats_against_head(&StdRunner::new(), root)).unwrap();
    assert_eq!(
        (
            stats.files_changed,
            stats.total_additions,
            stats.total_deletions
        ),
        (2, 2, 0),
        "the untracked file counts as a file, not by its lines"
    );
}

#[test]
fn no_upstream_means_no_counts_and_an_upstream_gives_ahead_and_behind() {
    if !git_available() {
        return;
    }
    let remote = tempfile::tempdir().unwrap();
    git(remote.path(), &["init", "-q", "--bare"]);
    let dir = repo_with(&[("a.txt", "1\n")]);
    let root = dir.path();
    let runner = StdRunner::new();

    let none = block_on(compute_git_status_metadata(&runner, root, None)).unwrap();
    assert_eq!(none.current_branch_name, "main");
    assert_eq!(none.tracking_upstream, None);
    assert!(!none.tracking_counts_available);

    git(
        root,
        &["remote", "add", "origin", remote.path().to_str().unwrap()],
    );
    git(root, &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(root.join("a.txt"), "1\n2\n").unwrap();
    git(root, &["commit", "-q", "-am", "ahead"]);
    let ahead = block_on(compute_git_status_metadata(
        &runner,
        root,
        Some("origin/main".into()),
    ))
    .unwrap();
    assert_eq!(
        ahead.main_branch_name, "origin/main",
        "a cached main branch is used as given"
    );
    assert_eq!(ahead.tracking_upstream.as_deref(), Some("origin/main"));
    assert_eq!((ahead.tracking_ahead, ahead.tracking_behind), (1, 0));
    assert!(ahead.tracking_counts_available && !ahead.tracking_rebased);
}

#[test]
fn a_detached_head_shows_the_short_sha() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    git(dir.path(), &["checkout", "-q", "--detach"]);
    let meta = block_on(compute_git_status_metadata(
        &StdRunner::new(),
        dir.path(),
        None,
    ))
    .unwrap();
    assert_ne!(meta.current_branch_name, "HEAD");
    assert!(meta.current_branch_name.len() >= 7);
}

#[test]
fn pushes_use_the_wire_path_and_an_absent_value_means_none() {
    let push = pr_info_push("/wire", None);
    assert_eq!((push.repo_path.as_str(), push.pr_info), ("/wire", None));
    let pr = crate::shellcmd::git::PrInfo {
        number: 3,
        url: "u".into(),
        state: "OPEN".into(),
        draft: false,
        base_branch: "main".into(),
    };
    assert_eq!(pr_info_push("/wire", Some(&pr)).pr_info.unwrap().number, 3);
    assert!(
        repository_info_push("/wire", None)
            .repository_info
            .is_none()
    );
}
