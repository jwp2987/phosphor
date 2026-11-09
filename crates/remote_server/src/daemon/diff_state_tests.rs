use std::time::Duration;

use crate::proto::{
    GetDiffState, UnsubscribeDiffState, get_diff_state_response, notification, server_message,
    session_scoped_request,
};
use crate::shellcmd::git::diff::Mode;
use crate::shellcmd::git::test_runner::{git, git_available, repo_with};

use super::live_tests_support::Live;

fn get(repo: &str, mode: &Mode) -> session_scoped_request::Message {
    session_scoped_request::Message::GetDiffState(GetDiffState {
        repo_path: repo.to_string(),
        mode: Some(mode.to_proto()),
    })
}

fn snapshot_of(m: crate::proto::ServerMessage) -> Result<crate::proto::DiffStateSnapshot, String> {
    match m.message {
        Some(server_message::Message::GetDiffStateResponse(r)) => match r.result.unwrap() {
            get_diff_state_response::Result::Snapshot(s) => Ok(s),
            get_diff_state_response::Result::Error(e) => Err(e.message),
        },
        other => panic!("expected a GetDiffStateResponse, got {other:?}"),
    }
}

fn paths(s: &crate::proto::DiffStateSnapshot) -> Vec<String> {
    let mut p: Vec<_> = s
        .diffs
        .iter()
        .flat_map(|d| d.files.iter().map(|f| f.file_path.clone()))
        .collect();
    p.sort();
    p
}

#[test]
fn a_bad_path_is_a_diff_state_error_not_an_error_response() {
    let mut live = Live::new();
    live.send_session("r", get("/definitely/not/here", &Mode::Head));
    let err = snapshot_of(live.expect()).unwrap_err();
    assert!(err.starts_with("Invalid repo_path: "), "{err}");
}

#[test]
fn the_snapshot_echoes_the_wire_path_verbatim_even_when_it_is_not_canonical() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    std::fs::write(dir.path().join("a.txt"), "1\n2\n").unwrap();
    let spelled = format!("{}/.", dir.path().display());
    let mut live = Live::new();
    live.send_session("r", get(&spelled, &Mode::Head));
    let snap = snapshot_of(live.expect()).unwrap();
    assert_eq!(snap.repo_path, spelled);
    assert_eq!(paths(&snap), vec!["a.txt"]);
}

#[test]
fn requests_for_one_key_join_one_computation_and_each_gets_its_own_wire_path() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    std::fs::write(dir.path().join("a.txt"), "1\n2\n").unwrap();
    let one = dir.path().display().to_string();
    let two = format!("{one}/");
    let mut live = Live::new();
    live.send_session("r1", get(&one, &Mode::Head));
    live.send_session("r2", get(&two, &Mode::Head));
    let a = live.expect();
    let b = live.expect();
    let mut by_id = std::collections::HashMap::new();
    by_id.insert(a.request_id.clone(), snapshot_of(a).unwrap());
    by_id.insert(b.request_id.clone(), snapshot_of(b).unwrap());
    assert_eq!(by_id["r1"].repo_path, one);
    assert_eq!(by_id["r2"].repo_path, two);
    assert_eq!(paths(&by_id["r1"]), paths(&by_id["r2"]));
}

#[test]
fn an_edit_is_pushed_as_a_file_delta_with_fresh_metadata_and_an_ignored_file_is_not_news() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[
        ("a.txt", "1\n"),
        (".gitignore", "*.log\n"),
        ("sub/b.txt", "b\n"),
    ]);
    let root = dir.path();
    let wire = root.display().to_string();
    let mut live = Live::new();
    live.send_session("r", get(&wire, &Mode::Head));
    let snap = snapshot_of(live.expect()).unwrap();
    assert!(paths(&snap).is_empty(), "a clean repo has no diff");
    // Let the watch plan land before touching files.
    assert!(live.stays_quiet(Duration::from_millis(300)));

    // An ignored file changes: nothing is pushed for it. Then a tracked file in a subdirectory.
    std::fs::write(root.join("noise.log"), "x").unwrap();
    std::fs::write(root.join("sub/b.txt"), "b\nmore\n").unwrap();
    let delta = live.expect_where(|m| match &m.message {
        Some(server_message::Message::DiffStateFileDelta(d)) => Some(d.clone()),
        _ => None,
    });
    assert_eq!(delta.repo_path, wire);
    assert_eq!(delta.file_path, "sub/b.txt");
    assert_eq!(delta.diff.as_ref().unwrap().hunks.len(), 1);
    let stats = delta
        .metadata
        .as_ref()
        .unwrap()
        .against_head
        .as_ref()
        .unwrap()
        .aggregate_stats
        .as_ref()
        .unwrap();
    assert_eq!((stats.files_changed, stats.total_additions), (1, 1));
}

#[test]
fn a_commit_moves_head_and_pushes_a_whole_snapshot() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    let root = dir.path();
    std::fs::write(root.join("a.txt"), "1\n2\n").unwrap();
    let wire = root.display().to_string();
    let mut live = Live::new();
    live.send_session("r", get(&wire, &Mode::Head));
    assert_eq!(paths(&snapshot_of(live.expect()).unwrap()), vec!["a.txt"]);
    assert!(live.stays_quiet(Duration::from_millis(300)));

    git(root, &["commit", "-q", "-am", "second"]);
    let snap = live.expect_where(|m| match &m.message {
        Some(server_message::Message::DiffStateSnapshot(s))
            if s.diffs.as_ref().is_some_and(|d| d.files.is_empty()) =>
        {
            Some(s.clone())
        }
        _ => None,
    });
    assert_eq!(snap.repo_path, wire);
    let meta = snap.metadata.unwrap();
    assert!(meta.has_head_commit);
}

#[test]
fn unsubscribing_stops_the_pushes() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    let root = dir.path();
    let wire = root.display().to_string();
    let mut live = Live::new();
    live.send_session("r", get(&wire, &Mode::Head));
    snapshot_of(live.expect()).unwrap();
    assert!(live.stays_quiet(Duration::from_millis(300)));
    live.notify(notification::Message::UnsubscribeDiffState(
        UnsubscribeDiffState {
            repo_path: wire,
            mode: Some(Mode::Head.to_proto()),
        },
    ));
    std::fs::write(root.join("a.txt"), "1\n2\n").unwrap();
    assert!(
        live.stays_quiet(Duration::from_millis(800)),
        "a push arrived after Unsubscribe"
    );
}

#[test]
fn a_dropped_connection_releases_the_repo() {
    if !git_available() {
        return;
    }
    let dir = repo_with(&[("a.txt", "1\n")]);
    let wire = dir.path().display().to_string();
    let mut live = Live::new();
    live.send_session("r", get(&wire, &Mode::Head));
    snapshot_of(live.expect()).unwrap();
    let conn = live.conn;
    live.server
        .handle_event(crate::daemon::server::Event::Disconnected { conn });
    assert!(live.server.is_repo_watch_free_for_tests());
}
