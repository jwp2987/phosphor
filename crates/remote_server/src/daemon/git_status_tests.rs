use std::time::Duration;

use crate::proto::{GitStatusPush, UpdateGitStatus, notification, server_message};
use crate::shellcmd::git::test_runner::{git, git_available, repo_with};

use super::live_tests_support::Live;

fn ask(live: &mut Live, wire: &str) {
    live.notify(notification::Message::UpdateGitStatus(UpdateGitStatus {
        repo_path: wire.to_string(),
    }));
}

fn push(m: &crate::proto::ServerMessage) -> Option<GitStatusPush> {
    match &m.message {
        Some(server_message::Message::GitStatusPush(p)) => Some(p.clone()),
        _ => None,
    }
}

#[test]
fn a_request_pushes_the_current_status_under_the_wire_path_even_when_unchanged() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    std::fs::write(dir.path().join("a.txt"), "1\n2\n").unwrap();
    let wire = format!("{}/.", dir.path().display());
    let mut live = Live::new();
    ask(&mut live, &wire);
    let p = live.expect_where(push);
    assert_eq!(p.repo_path, wire);
    let meta = p.metadata.unwrap();
    assert_eq!(meta.current_branch_name, "main");
    let stats = meta.stats_against_head.unwrap();
    assert_eq!((stats.files_changed, stats.total_additions), (1, 1));
    assert!(!meta.tracking_counts_available);

    // Asking again forces another push of the same value.
    ask(&mut live, &wire);
    let again = live.expect_where(push);
    assert_eq!(again.metadata.unwrap().current_branch_name, "main");
}

#[test]
fn an_edit_and_a_branch_change_are_pushed_without_being_asked_and_a_non_change_is_not() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    let root = dir.path();
    let wire = root.display().to_string();
    let mut live = Live::new();
    ask(&mut live, &wire);
    live.expect_where(push);

    std::fs::write(root.join("a.txt"), "1\nchange\n").unwrap();
    let edited = live.expect_where(|m| {
        push(m).filter(|p| {
            p.metadata
                .as_ref()
                .unwrap()
                .stats_against_head
                .as_ref()
                .unwrap()
                .total_additions
                == 1
        })
    });
    assert_eq!(edited.repo_path, wire);

    git(root, &["checkout", "-q", "-b", "topic"]);
    let moved = live.expect_where(|m| {
        push(m).filter(|p| p.metadata.as_ref().unwrap().current_branch_name == "topic")
    });
    assert_eq!(moved.repo_path, wire);

    // Rewriting the same bytes changes nothing the chip shows: no new push.
    std::fs::write(root.join("a.txt"), "1\nchange\n").unwrap();
    while live
        .next(Duration::from_millis(800))
        .is_some_and(|m| push(&m).is_none())
    {}
    assert!(live.stays_quiet(Duration::from_millis(500)));
}

#[test]
fn a_path_that_is_not_a_repository_says_nothing() {
    let mut live = Live::new();
    ask(&mut live, "/definitely/not/here");
    assert!(live.stays_quiet(Duration::from_millis(300)));
}
