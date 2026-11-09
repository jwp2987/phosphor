use std::time::Duration;

use crate::proto::{
    Abort, ClientMessage, ErrorCode, GetBranches, Initialize, ListDirectory, RunCommandRequest,
    SessionBootstrapped, host_scoped_request, notification, run_command_response, server_message,
    session_scoped_request,
};
use crate::rt::FutureExt as _;

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
    // Parity family a: batch file reads are served by the helper.
    assert!(caps.supports(crate::capability::FILE_CONTEXT));
    assert!(!caps.supports(crate::capability::BUFFERS));
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
            "SaveBuffer",
            host_scoped_request::Message::SaveBuffer(Default::default()),
        ),
        (
            "ResolveConflict",
            host_scoped_request::Message::ResolveConflict(Default::default()),
        ),
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
    let session_requests: Vec<(&str, session_scoped_request::Message)> = vec![
        (
            "NavigatedToDirectory",
            session_scoped_request::Message::NavigatedToDirectory(Default::default()),
        ),
        (
            "LoadRepoMetadataDirectory",
            session_scoped_request::Message::LoadRepoMetadataDirectory(Default::default()),
        ),
        (
            "OpenBuffer",
            session_scoped_request::Message::OpenBuffer(Default::default()),
        ),
        (
            "GetDiffState",
            session_scoped_request::Message::GetDiffState(Default::default()),
        ),
    ];
    for (name, message) in session_requests {
        h.send(session("rs", message));
        let e = error_of(h.next());
        assert_eq!(e.code, ErrorCode::Unsupported as i32, "{name}");
        assert!(e.message.contains(name), "{name}: {}", e.message);
    }
}
