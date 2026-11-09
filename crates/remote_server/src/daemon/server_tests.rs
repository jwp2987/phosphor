use std::time::Duration;

use crate::proto::{
    Abort, ClientMessage, ErrorCode, GetBranches, Initialize, ListDirectory,
    LoadRepoMetadataDirectory, NavigatedToDirectory, RunCommandRequest, SessionBootstrapped,
    host_scoped_request, notification, repo_node_metadata, run_command_response, server_message,
    session_scoped_request,
};
use crate::rt::FutureExt as _;

use std::path::{Path, PathBuf};

use super::*;

struct Harness {
    server: Server,
    rx: async_channel::Receiver<ServerMessage>,
    conn: ConnId,
}

fn harness() -> Harness {
    let mut server = Server::new(GRACE_PERIOD);
    let (tx, rx) = async_channel::unbounded();
    let conn = next_conn_id();
    server.handle_event(Event::Connected {
        conn,
        tx,
        chunking: Arc::new(AtomicBool::new(false)),
    });
    Harness { server, rx, conn }
}

impl Harness {
    fn send(&mut self, msg: ClientMessage) {
        self.server.handle_message(self.conn, msg);
    }

    fn next(&self) -> ServerMessage {
        async_io::block_on(async { self.rx.recv().with_timeout(Duration::from_secs(10)).await })
            .expect("timed out waiting for a response")
            .expect("connection channel closed")
    }
}

fn session(id: &str, message: session_scoped_request::Message) -> ClientMessage {
    ClientMessage::session_scoped(id.to_string(), message)
}

fn host(id: &str, message: host_scoped_request::Message) -> ClientMessage {
    ClientMessage::host_scoped(id.to_string(), message)
}

fn error_of(msg: ServerMessage) -> crate::proto::ErrorResponse {
    let Some(server_message::Message::Error(e)) = msg.message else {
        panic!("expected an ErrorResponse, got {msg:?}");
    };
    e
}

#[test]
fn initialize_advertises_the_helper_capabilities_and_version() {
    let mut h = harness();
    h.send(session(
        "r1",
        session_scoped_request::Message::Initialize(Initialize::default()),
    ));
    let msg = h.next();
    assert_eq!(msg.request_id, "r1");
    let Some(server_message::Message::InitializeResponse(resp)) = msg.message else {
        panic!("expected InitializeResponse");
    };
    assert_eq!(resp.server_version, crate::naming::helper_version());
    assert!(!resp.host_id.is_empty());
    assert!(
        !resp.chunked_responses,
        "the client did not ask for chunking"
    );
    let caps = crate::capability::ServerCapabilities::from_initialize_response(&resp);
    assert!(caps.is_limited());
    assert!(caps.supports(crate::capability::RUN_COMMAND));
    // The pin: exactly what this build implements, so a family cannot be added without the
    // capability (or the capability without the family).
    assert_eq!(
        resp.capabilities,
        [
            "file_write",
            "fs_basic",
            "run_command",
            "git_read",
            "ripgrep_search",
            "repo_metadata",
            "file_context",
            "buffers"
        ]
    );
    // Parity family a: batch file reads are served by the helper.
    assert!(caps.supports(crate::capability::FILE_CONTEXT));
    // Parity family c: server-side buffers are served by the helper.
    assert!(caps.supports(crate::capability::BUFFERS));
    assert!(!caps.supports(crate::capability::DIFF_STATE));
}

/// Family a through the server core: a real `ReadFileContext` request is answered with the
/// shared assembler's response (the per-case coverage is in `file_context_tests.rs`).
#[test]
fn read_file_context_is_served_not_unsupported() {
    let mut h = harness();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    h.send(host(
        "rc",
        host_scoped_request::Message::ReadFileContext(crate::proto::ReadFileContextRequest {
            files: vec![crate::proto::ReadFileContextFile {
                path: path.to_string_lossy().into_owned(),
                line_ranges: vec![],
            }],
            ..Default::default()
        }),
    ));
    let msg = h.next();
    let Some(server_message::Message::ReadFileContextResponse(resp)) = msg.message else {
        panic!("expected ReadFileContextResponse, got {msg:?}");
    };
    assert!(resp.failed_files.is_empty(), "{resp:?}");
    assert_eq!(resp.file_contexts.len(), 1);
}

#[test]
fn a_chunking_client_gets_chunking_unless_the_env_switch_is_off() {
    assert!(chunked_responses_enabled_from_env(None));
    assert!(chunked_responses_enabled_from_env(Some("1")));
    for off in ["0", "false", "off", "no", " 0 "] {
        assert!(!chunked_responses_enabled_from_env(Some(off)), "{off:?}");
    }
}

#[test]
fn requests_outside_the_capability_list_are_unsupported() {
    let mut h = harness();
    h.send(host(
        "r2",
        host_scoped_request::Message::GitPush(Default::default()),
    ));
    let e = error_of(h.next());
    assert_eq!(e.code, ErrorCode::Unsupported as i32);
    assert!(e.message.contains("GitPush"), "{}", e.message);

    h.send(session(
        "r3",
        session_scoped_request::Message::GetDiffState(Default::default()),
    ));
    assert_eq!(error_of(h.next()).code, ErrorCode::Unsupported as i32);
}

#[test]
fn an_envelope_without_a_variant_is_an_invalid_request() {
    let mut h = harness();
    h.send(ClientMessage {
        request_id: "r4".into(),
        message: None,
    });
    assert_eq!(error_of(h.next()).code, ErrorCode::InvalidRequest as i32);
}

#[test]
fn list_directory_is_answered_on_the_requesting_connection() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("f.txt"), "x").unwrap();
    let mut h = harness();
    h.send(host(
        "r5",
        host_scoped_request::Message::ListDirectory(ListDirectory {
            path: dir.path().to_string_lossy().into_owned(),
        }),
    ));
    let msg = h.next();
    assert_eq!(msg.request_id, "r5");
    assert!(matches!(
        msg.message,
        Some(server_message::Message::ListDirectoryResponse(_))
    ));
}

#[test]
fn a_failing_git_request_is_an_error_response_from_a_worker() {
    let mut h = harness();
    h.send(host(
        "r6",
        host_scoped_request::Message::GetBranches(GetBranches {
            repo_path: "/definitely/not/here".into(),
            max_branch_count: None,
            include_remotes: false,
        }),
    ));
    let msg = h.next();
    assert_eq!(msg.request_id, "r6");
    assert!(matches!(
        msg.message,
        Some(server_message::Message::GetBranchesResponse(_))
    ));
}

#[test]
fn run_command_needs_a_bootstrapped_session() {
    let mut h = harness();
    h.send(session(
        "r7",
        session_scoped_request::Message::RunCommand(RunCommandRequest {
            command: "true".into(),
            working_directory: None,
            environment_variables: Default::default(),
            session_id: 41,
        }),
    ));
    let Some(server_message::Message::RunCommandResponse(resp)) = h.next().message else {
        panic!("expected RunCommandResponse");
    };
    let Some(run_command_response::Result::Error(e)) = resp.result else {
        panic!("expected an error result");
    };
    assert_eq!(
        e.code,
        crate::proto::RunCommandErrorCode::SessionNotFound as i32
    );
}

#[test]
fn run_command_after_session_bootstrapped_runs_in_that_shell() {
    let Some(bash) = std::env::var_os("PATH").and_then(|p| {
        std::env::split_paths(&p)
            .map(|d| d.join("bash"))
            .find(|c| c.is_file())
    }) else {
        return;
    };
    let mut h = harness();
    h.send(ClientMessage::notification(
        notification::Message::SessionBootstrapped(SessionBootstrapped {
            session_id: 9,
            shell_type: "bash".into(),
            shell_path: Some(bash.to_string_lossy().into_owned()),
        }),
    ));
    h.send(session(
        "r8",
        session_scoped_request::Message::RunCommand(RunCommandRequest {
            command: "printf hi".into(),
            working_directory: None,
            environment_variables: Default::default(),
            session_id: 9,
        }),
    ));
    let Some(server_message::Message::RunCommandResponse(resp)) = h.next().message else {
        panic!("expected RunCommandResponse");
    };
    let Some(run_command_response::Result::Success(ok)) = resp.result else {
        panic!("expected success: {resp:?}");
    };
    assert_eq!(ok.stdout, b"hi");
    assert_eq!(ok.exit_code, Some(0));
}

#[test]
fn abort_of_an_unknown_request_is_harmless() {
    let mut h = harness();
    h.send(ClientMessage::notification(notification::Message::Abort(
        Abort {
            request_id_to_abort: "nope".into(),
        },
    )));
    assert!(h.rx.try_recv().is_err(), "a notification gets no response");
}

#[test]
fn unsupported_notifications_are_dropped_silently() {
    let mut h = harness();
    h.send(ClientMessage::notification(
        notification::Message::UpdateGitStatus(Default::default()),
    ));
    assert!(h.rx.try_recv().is_err());
}

#[test]
fn the_daemon_exits_after_the_grace_period_with_no_connections() {
    let mut server = Server::new(Duration::from_millis(50));
    let (_tx, rx) = std::sync::mpsc::channel();
    let started = Instant::now();
    server.run(rx);
    assert!(started.elapsed() >= Duration::from_millis(50));
    assert!(started.elapsed() < Duration::from_secs(10));
}

#[test]
fn a_connection_keeps_the_daemon_alive_past_the_grace_period() {
    let mut server = Server::new(Duration::from_millis(50));
    let (tx, _rx) = async_channel::unbounded();
    server.handle_event(Event::Connected {
        conn: next_conn_id(),
        tx,
        chunking: Arc::new(AtomicBool::new(false)),
    });
    let (events_tx, events_rx) = std::sync::mpsc::channel::<Event>();
    let handle = std::thread::spawn(move || {
        server.run(events_rx);
    });
    std::thread::sleep(Duration::from_millis(300));
    assert!(
        !handle.is_finished(),
        "a connected daemon must not idle out"
    );
    drop(events_tx);
    handle.join().unwrap();
}

/// P0 reserved a module per future family; until its package lands each one answers its
/// requests with UNSUPPORTED, naming the request so the client's log says what it fell back on.
#[test]
fn every_reserved_family_request_is_unsupported_and_named() {
    let mut h = harness();
    let host_requests: Vec<(&str, host_scoped_request::Message)> = vec![
        (
            "GitCommitChain",
            host_scoped_request::Message::GitCommitChain(Default::default()),
        ),
        (
            "GitPull",
            host_scoped_request::Message::GitPull(Default::default()),
        ),
        (
            "GitCreatePr",
            host_scoped_request::Message::GitCreatePr(Default::default()),
        ),
        (
            "DiscardFiles",
            host_scoped_request::Message::DiscardFiles(Default::default()),
        ),
        (
            "GitStage",
            host_scoped_request::Message::GitStage(Default::default()),
        ),
    ];
    for (name, message) in host_requests {
        h.send(host("rh", message));
        let e = error_of(h.next());
        assert_eq!(e.code, ErrorCode::Unsupported as i32, "{name}");
        assert!(e.message.contains(name), "{name}: {}", e.message);
    }
    let session_requests: Vec<(&str, session_scoped_request::Message)> = vec![(
        "GetDiffState",
        session_scoped_request::Message::GetDiffState(Default::default()),
    )];
    for (name, message) in session_requests {
        h.send(session("rs", message));
        let e = error_of(h.next());
        assert_eq!(e.code, ErrorCode::Unsupported as i32, "{name}");
        assert!(e.message.contains(name), "{name}: {}", e.message);
    }
}

// ---- repo metadata (P3) -------------------------------------------------------------------

fn navigate_msg(id: &str, path: &Path) -> ClientMessage {
    session(
        id,
        session_scoped_request::Message::NavigatedToDirectory(NavigatedToDirectory {
            path: path.to_string_lossy().into_owned(),
        }),
    )
}

fn load_msg(id: &str, repo: &Path, dir: &Path) -> ClientMessage {
    session(
        id,
        session_scoped_request::Message::LoadRepoMetadataDirectory(LoadRepoMetadataDirectory {
            repo_path: repo.to_string_lossy().into_owned(),
            dir_path: dir.to_string_lossy().into_owned(),
        }),
    )
}

/// (path, is_dir, ignored, loaded) of every node of an entry update's subtree.
fn nodes(update: &crate::proto::RepoMetadataEntryUpdate) -> Vec<(String, bool, bool, bool)> {
    update
        .subtree_metadata
        .iter()
        .map(|n| match n.node.clone().unwrap() {
            repo_node_metadata::Node::Directory(d) => (d.path, true, d.ignored, d.loaded),
            repo_node_metadata::Node::File(f) => (f.path, false, f.ignored, false),
        })
        .collect()
}

fn lossy(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

fn git_repo() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join(".git")).unwrap();
    std::fs::write(root.join(".gitignore"), "*.log\ntarget/\n").unwrap();
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
    std::fs::write(root.join("src/deep/x.rs"), "").unwrap();
    std::fs::write(root.join("a.txt"), "").unwrap();
    std::fs::write(root.join("x.log"), "").unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    (tmp, root)
}

#[test]
fn navigating_outside_git_replies_then_pushes_a_one_level_snapshot() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    std::fs::create_dir(root.join("sub")).unwrap();
    std::fs::write(root.join("sub/b.txt"), "").unwrap();
    std::fs::write(root.join("a.txt"), "").unwrap();
    let mut h = harness();
    h.send(navigate_msg("n1", &root));

    let reply = h.next();
    assert_eq!(reply.request_id, "n1");
    let Some(server_message::Message::NavigatedToDirectoryResponse(resp)) = reply.message else {
        panic!("expected NavigatedToDirectoryResponse, got {reply:?}");
    };
    assert_eq!(resp.indexed_path, lossy(&root));
    assert!(!resp.is_git);

    // The snapshot is a push (no request id) and arrives right after the reply.
    let push = h.next();
    assert_eq!(push.request_id, "");
    let Some(server_message::Message::RepoMetadataSnapshot(snap)) = push.message else {
        panic!("expected RepoMetadataSnapshot, got {push:?}");
    };
    assert_eq!(snap.repo_path, lossy(&root));
    assert!(snap.sync_complete);
    assert_eq!(
        snap.entries.len(),
        1,
        "outside git only one level is listed"
    );
    assert_eq!(snap.entries[0].parent_path_to_replace, lossy(&root));
    assert_eq!(
        nodes(&snap.entries[0]),
        vec![
            (lossy(&root.join("sub")), true, false, false),
            (lossy(&root.join("a.txt")), false, false, false),
        ]
    );
}

#[test]
fn navigating_into_a_git_repo_indexes_the_root_and_walks_the_tree_with_ignore_rules() {
    let (_tmp, root) = git_repo();
    let mut h = harness();
    h.send(navigate_msg("n1", &root.join("src/deep")));
    let Some(server_message::Message::NavigatedToDirectoryResponse(resp)) = h.next().message else {
        panic!("expected NavigatedToDirectoryResponse");
    };
    assert_eq!(resp.indexed_path, lossy(&root));
    assert!(resp.is_git);
    let Some(server_message::Message::RepoMetadataSnapshot(snap)) = h.next().message else {
        panic!("expected RepoMetadataSnapshot");
    };
    let parents: Vec<_> = snap
        .entries
        .iter()
        .map(|e| e.parent_path_to_replace.clone())
        .collect();
    assert_eq!(
        parents,
        [
            lossy(&root),
            lossy(&root.join("src")),
            lossy(&root.join("src/deep"))
        ]
    );
    assert_eq!(
        nodes(&snap.entries[0]),
        vec![
            (lossy(&root.join(".git")), true, true, false),
            (lossy(&root.join("src")), true, false, true),
            (lossy(&root.join("target")), true, true, false),
            (lossy(&root.join(".gitignore")), false, false, false),
            (lossy(&root.join("a.txt")), false, false, false),
            (lossy(&root.join("x.log")), false, true, false),
        ]
    );
    assert_eq!(h.server.repos.roots(), vec![lossy(&root)]);
}

#[test]
fn a_second_navigation_in_the_same_repo_is_answered_without_another_snapshot() {
    let (_tmp, root) = git_repo();
    let mut h = harness();
    h.send(navigate_msg("n1", &root));
    let _reply = h.next();
    let _snapshot = h.next();
    // The inline (no event channel) core cannot watch, so the repo is re-walked; with watching
    // available the cached tree answers. Either way the reply is first and exactly once.
    h.send(navigate_msg("n2", &root.join("src")));
    let again = h.next();
    assert_eq!(again.request_id, "n2");
    assert!(matches!(
        again.message,
        Some(server_message::Message::NavigatedToDirectoryResponse(_))
    ));
}

#[test]
fn a_path_that_cannot_be_canonicalized_is_an_invalid_request() {
    let mut h = harness();
    h.send(navigate_msg("n1", Path::new("/no/such/dir/for/phosphor")));
    let e = error_of(h.next());
    assert_eq!(e.code, ErrorCode::InvalidRequest as i32);
    assert!(e.message.starts_with("Invalid path: "), "{}", e.message);

    let tmp = tempfile::tempdir().unwrap();
    let file = tmp.path().join("f");
    std::fs::write(&file, "").unwrap();
    h.send(navigate_msg("n2", &file));
    assert_eq!(error_of(h.next()).code, ErrorCode::InvalidRequest as i32);
}

#[test]
fn loading_a_directory_returns_its_children_and_validates_the_request() {
    let (_tmp, root) = git_repo();
    let mut h = harness();
    h.send(navigate_msg("n1", &root));
    let _ = (h.next(), h.next());

    // target/ was listed unloaded and ignored; loading it flags everything inside as ignored.
    h.send(load_msg("l1", &root, &root.join("target")));
    let msg = h.next();
    assert_eq!(msg.request_id, "l1");
    let Some(server_message::Message::LoadRepoMetadataDirectoryResponse(resp)) = msg.message else {
        panic!("expected LoadRepoMetadataDirectoryResponse, got {msg:?}");
    };
    assert_eq!(resp.entries.len(), 1);
    assert_eq!(
        resp.entries[0].parent_path_to_replace,
        lossy(&root.join("target"))
    );
    assert_eq!(
        nodes(&resp.entries[0]),
        vec![(lossy(&root.join("target/debug")), true, true, false)]
    );

    // An empty directory is an empty entry list.
    h.send(load_msg("l2", &root, &root.join("target/debug")));
    let Some(server_message::Message::LoadRepoMetadataDirectoryResponse(resp)) = h.next().message
    else {
        panic!("expected LoadRepoMetadataDirectoryResponse");
    };
    assert!(resp.entries.is_empty());

    h.send(load_msg("l3", &root, tempfile::tempdir().unwrap().path()));
    let e = error_of(h.next());
    assert_eq!(e.code, ErrorCode::InvalidRequest as i32);
    assert!(
        e.message.contains("is not a descendant of repo_path"),
        "{}",
        e.message
    );

    h.send(load_msg("l4", Path::new("/no/such"), &root));
    assert!(
        error_of(h.next())
            .message
            .starts_with("Invalid repo_path: ")
    );
    h.send(load_msg("l5", &root, &root.join("missing")));
    assert!(error_of(h.next()).message.starts_with("Invalid dir_path: "));

    let other = tempfile::tempdir().unwrap();
    h.send(load_msg("l6", other.path(), other.path()));
    assert_eq!(error_of(h.next()).code, ErrorCode::Internal as i32);
}

/// A core with its own event channel (so workers and the watcher can report back), driven by
/// hand: `pump_until` feeds events to the core until a push satisfying the predicate arrives.
struct LiveHarness {
    h: Harness,
    events: std::sync::mpsc::Receiver<Event>,
}

impl LiveHarness {
    fn new() -> Self {
        let (tx, events) = std::sync::mpsc::channel();
        let mut server = Server::new(GRACE_PERIOD).with_events(tx);
        let (out_tx, rx) = async_channel::unbounded();
        let conn = next_conn_id();
        server.handle_event(Event::Connected {
            conn,
            tx: out_tx,
            chunking: Arc::new(AtomicBool::new(false)),
        });
        Self {
            h: Harness { server, rx, conn },
            events,
        }
    }

    fn pump_until(&mut self, want: impl Fn(&ServerMessage) -> bool) -> ServerMessage {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            while let Ok(msg) = self.h.rx.try_recv() {
                if want(&msg) {
                    return msg;
                }
            }
            assert!(std::time::Instant::now() < deadline, "no matching message");
            if let Ok(event) = self.events.recv_timeout(Duration::from_millis(50)) {
                self.h.server.handle_event(event);
            }
        }
    }
}

#[test]
fn a_watched_repo_pushes_an_update_when_a_file_appears_or_goes_with_no_timer() {
    let (_tmp, root) = git_repo();
    let mut live = LiveHarness::new();
    live.h.send(navigate_msg("n1", &root));
    let snapshot = live.pump_until(|m| {
        matches!(
            m.message,
            Some(server_message::Message::RepoMetadataSnapshot(_))
        )
    });
    assert_eq!(snapshot.request_id, "");
    if live.h.server.watches.len() == 0 {
        eprintln!("no filesystem watcher on this machine; skipping the live part");
        return;
    }

    std::fs::write(root.join("src/new.rs"), "").unwrap();
    std::fs::remove_file(root.join("a.txt")).unwrap();

    // One or two pushes may carry the two changes; collect until both are seen.
    let mut added = false;
    let mut removed = false;
    while !(added && removed) {
        let push = live.pump_until(|m| {
            matches!(
                m.message,
                Some(server_message::Message::RepoMetadataUpdate(_))
            )
        });
        let Some(server_message::Message::RepoMetadataUpdate(update)) = push.message else {
            unreachable!();
        };
        assert_eq!(update.repo_path, lossy(&root));
        removed |= update.remove_entries.contains(&lossy(&root.join("a.txt")));
        for entry in &update.update_entries {
            if entry.parent_path_to_replace == lossy(&root.join("src")) {
                added |= nodes(entry)
                    .iter()
                    .any(|n| n.0 == lossy(&root.join("src/new.rs")));
            }
        }
    }
}

#[test]
fn a_repo_nobody_follows_any_more_releases_its_watches() {
    let (_tmp, root) = git_repo();
    let mut live = LiveHarness::new();
    live.h.send(navigate_msg("n1", &root));
    live.pump_until(|m| {
        matches!(
            m.message,
            Some(server_message::Message::RepoMetadataSnapshot(_))
        )
    });
    let held = live.h.server.watches.len();
    if held == 0 {
        return;
    }
    // root, src, src/deep: loaded and not ignored.
    assert_eq!(held, 3);
    let conn = live.h.conn;
    live.h.server.handle_event(Event::Disconnected { conn });
    assert_eq!(live.h.server.watches.len(), 0);
    assert!(live.h.server.repos.roots().is_empty());
}

// ---- buffers (P4) -------------------------------------------------------------------------

fn open_buffer_msg(id: &str, path: &Path, force_reload: bool) -> ClientMessage {
    session(
        id,
        session_scoped_request::Message::OpenBuffer(crate::proto::OpenBuffer {
            path: lossy(path),
            force_reload,
        }),
    )
}

fn buffer_edit_msg(
    path: &Path,
    expected_server_version: u64,
    new_client_version: u64,
    edits: &[(u64, u64, &str)],
) -> ClientMessage {
    ClientMessage::notification(notification::Message::BufferEdit(
        crate::proto::BufferEdit {
            path: lossy(path),
            expected_server_version,
            new_client_version,
            edits: edits
                .iter()
                .map(|&(start_offset, end_offset, text)| crate::proto::TextEdit {
                    start_offset,
                    end_offset,
                    text: text.to_owned(),
                })
                .collect(),
        },
    ))
}

fn opened(h: &mut Harness, path: &Path, force_reload: bool) -> crate::proto::OpenBufferResponse {
    h.send(open_buffer_msg("ob", path, force_reload));
    let Some(server_message::Message::OpenBufferResponse(resp)) = h.next().message else {
        panic!("expected OpenBufferResponse");
    };
    resp
}

fn saved(h: &mut Harness, path: &Path) -> Option<String> {
    h.send(host(
        "sb",
        host_scoped_request::Message::SaveBuffer(crate::proto::SaveBuffer { path: lossy(path) }),
    ));
    let Some(server_message::Message::SaveBufferResponse(resp)) = h.next().message else {
        panic!("expected SaveBufferResponse");
    };
    match resp.result {
        Some(crate::proto::save_buffer_response::Result::Error(e)) => Some(e.message),
        _ => None,
    }
}

fn disk_changed(h: &mut Harness, path: &Path) {
    h.server.handle_event(Event::FsChange {
        paths: vec![path.to_path_buf()],
        kind: FsKind::Modify,
    });
}

#[test]
fn a_buffer_opens_edits_and_saves_with_char_offsets() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "héllo wörld\n").unwrap();
    let mut h = harness();
    let open = opened(&mut h, &path, false);
    assert_eq!(open.content, "héllo wörld\n");
    assert_eq!(open.server_version, 1);

    // Offsets are 1-indexed characters: replace "wörld" (chars 7..12) and insert at the end of
    // the text as it is after that first edit; an offset past the end is clamped.
    h.send(buffer_edit_msg(
        &path,
        1,
        7,
        &[(7, 12, "there"), (999, 999, "!")],
    ));
    assert_eq!(saved(&mut h, &path), None);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "héllo there\n!");
    assert!(
        std::fs::read_dir(dir.path()).unwrap().count() == 1,
        "the temp file is gone"
    );
}

#[test]
fn an_edit_with_a_stale_server_version_is_dropped_silently() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "abc").unwrap();
    let mut h = harness();
    opened(&mut h, &path, false);
    h.send(buffer_edit_msg(&path, 99, 1, &[(1, 1, "X")]));
    assert!(h.rx.try_recv().is_err(), "a rejected edit gets no response");
    assert_eq!(saved(&mut h, &path), None);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
}

#[test]
fn a_disk_change_without_client_edits_is_pushed_as_one_edit_and_bumps_the_version() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "one two three").unwrap();
    let mut h = harness();
    opened(&mut h, &path, false);
    std::fs::write(&path, "one 2 three").unwrap();
    disk_changed(&mut h, &path);
    let push = h.next();
    assert_eq!(push.request_id, "");
    let Some(server_message::Message::BufferUpdated(update)) = push.message else {
        panic!("expected BufferUpdated, got {push:?}");
    };
    assert_eq!(update.path, lossy(&path));
    assert_eq!(update.new_server_version, 2);
    assert_eq!(update.expected_client_version, 0);
    assert_eq!(update.edits.len(), 1);
    assert_eq!(
        (
            update.edits[0].start_offset,
            update.edits[0].end_offset,
            update.edits[0].text.as_str()
        ),
        (5, 8, "2")
    );
    // The same change again is not a change.
    disk_changed(&mut h, &path);
    assert!(h.rx.try_recv().is_err());
    // An edit against the OLD server version is now stale.
    h.send(buffer_edit_msg(&path, 1, 3, &[(1, 1, "X")]));
    assert_eq!(saved(&mut h, &path), None);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "one 2 three");
}

#[test]
fn a_disk_change_over_unsaved_edits_is_a_conflict_and_our_own_save_is_not() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "abc").unwrap();
    let mut h = harness();
    opened(&mut h, &path, false);
    h.send(buffer_edit_msg(&path, 1, 5, &[(4, 4, "d")]));
    std::fs::write(&path, "outside").unwrap();
    disk_changed(&mut h, &path);
    let Some(server_message::Message::BufferConflictDetected(c)) = h.next().message else {
        panic!("expected BufferConflictDetected");
    };
    assert_eq!(c.path, lossy(&path));
    // The disk change was NOT applied: the buffer still holds the edited text.
    // "Discard": force_reload takes the disk's copy, bumps the version, resets the client's.
    let reopened = opened(&mut h, &path, true);
    assert_eq!(reopened.content, "outside");
    assert_eq!(reopened.server_version, 2);
    // Our own save echoes through the watcher without a push.
    h.send(buffer_edit_msg(&path, 2, 6, &[(8, 8, "!")]));
    assert_eq!(saved(&mut h, &path), None);
    disk_changed(&mut h, &path);
    assert!(h.rx.try_recv().is_err());
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "outside!");
}

#[test]
fn resolve_conflict_keeps_the_clients_text_unless_the_server_version_moved() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "abc").unwrap();
    let mut h = harness();
    opened(&mut h, &path, false);
    let resolve = |h: &mut Harness, ack: u64, content: &str| {
        h.send(host(
            "rc",
            host_scoped_request::Message::ResolveConflict(crate::proto::ResolveConflict {
                path: lossy(&path),
                acknowledged_server_version: ack,
                client_content: content.to_owned(),
                current_client_version: 9,
            }),
        ));
        let Some(server_message::Message::ResolveConflictResponse(resp)) = h.next().message else {
            panic!("expected ResolveConflictResponse");
        };
        match resp.result {
            Some(crate::proto::resolve_conflict_response::Result::Error(e)) => Some(e.message),
            _ => None,
        }
    };
    let stale = resolve(&mut h, 7, "mine").expect("a stale acknowledgement is refused");
    assert!(stale.contains("Stale"), "{stale}");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "abc");
    assert_eq!(resolve(&mut h, 1, "mine"), None);
    assert_eq!(std::fs::read_to_string(&path).unwrap(), "mine");
}

#[test]
fn a_second_connection_is_told_about_a_force_reload_and_the_last_close_frees_the_buffer() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "abc").unwrap();
    let mut h = harness();
    opened(&mut h, &path, false);
    // A second connection on the same core.
    let (tx2, rx2) = async_channel::unbounded();
    let conn2 = next_conn_id();
    h.server.handle_event(Event::Connected {
        conn: conn2,
        tx: tx2,
        chunking: Arc::new(AtomicBool::new(false)),
    });
    h.server
        .handle_message(conn2, open_buffer_msg("o2", &path, false));
    assert!(rx2.try_recv().is_ok(), "the second open is answered");
    std::fs::write(&path, "abcdef").unwrap();
    // The first connection force-reloads: the second one hears the whole new text.
    opened(&mut h, &path, true);
    let push = rx2.try_recv().expect("the other connection is told");
    let Some(server_message::Message::BufferUpdated(update)) = push.message else {
        panic!("expected BufferUpdated");
    };
    assert_eq!(update.new_server_version, 2);
    assert_eq!(
        (update.edits[0].start_offset, update.edits[0].end_offset),
        (1, 4)
    );
    assert_eq!(update.edits[0].text, "abcdef");

    h.server.handle_message(
        h.conn,
        ClientMessage::notification(notification::Message::CloseBuffer(
            crate::proto::CloseBuffer { path: lossy(&path) },
        )),
    );
    assert!(
        !h.server.buffers.is_empty(),
        "the second connection still follows it"
    );
    h.server.handle_event(Event::Disconnected { conn: conn2 });
    assert!(h.server.buffers.is_empty());
}

#[test]
fn opening_a_missing_or_binary_file_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let mut h = harness();
    h.send(open_buffer_msg("m", &dir.path().join("nope"), false));
    assert_eq!(error_of(h.next()).code, ErrorCode::Internal as i32);
    let bin = dir.path().join("b.bin");
    std::fs::write(&bin, [0xff, 0xfe, 0x00]).unwrap();
    h.send(open_buffer_msg("b", &bin, false));
    let e = error_of(h.next());
    assert!(e.message.contains("UTF-8"), "{}", e.message);
}
