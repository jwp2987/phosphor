//! The contract harness: ONE set of assertions run against every [`RemoteFeatures`] backend, so
//! "the shell backend behaves like the helper" is checked, not assumed.
//!
//! Subjects:
//! * the shell backend over a local `sh` ([`LocalShExecutor`]), always;
//! * the helper backend over an in-process daemon, when built with `--features daemon` on unix
//!   (`cargo test -p remote_server --features daemon`).
//!
//! P0 proves the harness with one family-independent assertion ([`assert_unimplemented_is_unsupported`]).
//! Each family package adds its assertions to [`assert_contract`], which then bind both backends.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use futures::future::BoxFuture;
use futures::io::{AsyncRead, AsyncWrite};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use warpui::r#async::executor;

use super::*;
use crate::HostId;
use crate::client::RemoteServerClient;
use crate::proto::{ErrorCode, ErrorResponse, GitStageRequest, ServerMessage, server_message};
use crate::protocol;
use crate::shellcmd::{Script, ShellKind, StderrMode};

/// Runs commands with a local `sh -c`: the stand-in for the ControlMaster executor.
#[derive(Debug)]
pub(super) struct LocalShExecutor;

impl OutOfBandExecutor for LocalShExecutor {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::LocalSh
    }

    fn shell_kind(&self) -> ShellKind {
        ShellKind::Posix
    }

    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move {
            use std::io::Write as _;
            use std::process::{Command, Stdio};
            let mut child = Command::new("sh")
                .arg("-c")
                .arg(&request.command)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| ExecError::Failed(e.to_string()))?;
            if let Some(mut stdin) = child.stdin.take() {
                let _ = stdin.write_all(&request.stdin);
            }
            let out = child
                .wait_with_output()
                .map_err(|e| ExecError::Failed(e.to_string()))?;
            let limit = request.max_output_bytes;
            let truncated = limit > 0 && out.stdout.len() > limit;
            let mut stdout = out.stdout;
            if truncated {
                stdout.truncate(limit);
            }
            Ok(ExecOutput {
                stdout,
                stderr: out.stderr,
                exit_code: out.status.code(),
                truncated,
            })
        })
    }
}

/// A backend under test.
pub(super) struct Subject {
    pub name: &'static str,
    pub backend: Arc<dyn RemoteFeatures>,
}

/// The family-independent assertion that proves the harness: a request no backend implements yet
/// is reported as Unsupported (so the router can fall back), never a hang, a panic or a generic
/// failure. Identical for the helper (the daemon answers `UNSUPPORTED`) and the shell skeleton.
pub(super) async fn assert_unimplemented_is_unsupported(subject: &Subject) {
    let err = subject
        .backend
        .git_stage(GitStageRequest::default())
        .await
        .expect_err("git_stage is not implemented by any backend in P0");
    assert!(
        err.is_unsupported(),
        "{}: expected Unsupported, got {err:?}",
        subject.name
    );
    assert!(!err.is_connection_lost(), "{}: {err:?}", subject.name);
}

/// Every contract assertion. Family packages append theirs here.
pub(super) async fn assert_contract(subject: &Subject) {
    assert_unimplemented_is_unsupported(subject).await;
}

fn shell_subject() -> Subject {
    Subject {
        name: "shell(local sh)",
        backend: Arc::new(ShellBackend::new(Some(Arc::new(LocalShExecutor)))),
    }
}

#[tokio::test]
async fn the_shell_backend_meets_the_contract() {
    assert_contract(&shell_subject()).await;
}

#[cfg(all(feature = "daemon", unix))]
#[tokio::test]
async fn the_helper_backend_over_an_in_process_daemon_meets_the_contract() {
    let stream = crate::daemon::spawn_in_process_for_tests();
    let stream = async_io::Async::new(stream).unwrap();
    let (reader, writer) = futures::io::AsyncReadExt::split(stream);
    let executor = executor::Background::default();
    let (client, _events, _host_responses) = RemoteServerClient::new(reader, writer, &executor);
    let subject = Subject {
        name: "helper(in-process daemon)",
        backend: Arc::new(HelperBackend::new(Arc::new(client))),
    };
    assert_contract(&subject).await;
}

#[tokio::test]
async fn without_an_out_of_band_channel_the_shell_backend_is_unavailable_not_unsupported() {
    let shell = ShellBackend::new(None);
    assert!(!shell.is_available());
    let err = shell
        .git_stage(GitStageRequest::default())
        .await
        .expect_err("no channel");
    assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");
    assert!(!err.is_unsupported());
}

#[tokio::test]
async fn a_framed_script_round_trips_through_a_local_sh() {
    let shell = ShellBackend::new(Some(Arc::new(LocalShExecutor)));
    let script = Script::new(ShellBackend::new_nonce())
        .unwrap()
        .section("two-lines", "printf 'a\\nb'", StderrMode::Merge)
        .unwrap()
        .section("fails", "echo oops >&2; exit 3", StderrMode::Merge)
        .unwrap()
        .section("quiet", "echo hidden >&2", StderrMode::Discard)
        .unwrap()
        .section("cd-is-isolated", "cd / && pwd", StderrMode::Merge)
        .unwrap()
        .section(
            "still-home",
            "test \"$(pwd)\" != / ; echo $?",
            StderrMode::Merge,
        )
        .unwrap();
    let frame = shell.run_script(&script).await.unwrap();
    assert_eq!(frame.section("two-lines").unwrap().output, b"a\nb");
    let fails = frame.section("fails").unwrap();
    assert_eq!((fails.exit, fails.text().as_str()), (3, "oops\n"));
    assert_eq!(frame.section("quiet").unwrap().output, b"");
    assert_eq!(frame.section("cd-is-isolated").unwrap().text(), "/\n");
    // The earlier `cd /` ran in its own subshell and did not leak into the next section.
    assert_eq!(frame.section("still-home").unwrap().text(), "0\n");
}

#[tokio::test]
async fn the_base64_probe_finds_a_tool_and_a_binary_file_round_trips() {
    let shell = ShellBackend::new(Some(Arc::new(LocalShExecutor)));
    let tool = shell
        .base64_tool()
        .await
        .unwrap()
        .expect("a test machine has base64, openssl or od");
    // Cached: a second call does not run the probe again (same answer either way).
    assert_eq!(shell.base64_tool().await.unwrap(), Some(tool));

    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("blob.bin");
    let data: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    std::fs::write(&path, &data).unwrap();
    let quoted = crate::shellcmd::sh_quote(path.to_str().unwrap());
    let script = Script::new(ShellBackend::new_nonce())
        .unwrap()
        .section(
            "blob",
            tool.encode_file_command(&quoted),
            StderrMode::Discard,
        )
        .unwrap();
    let frame = shell.run_script(&script).await.unwrap();
    assert_eq!(
        tool.decode(&frame.section("blob").unwrap().output).unwrap(),
        data
    );
}

/// A server that answers every request `UNSUPPORTED` and counts them: a legacy-looking daemon
/// (it advertises nothing, so the router believes it supports everything) that does not.
async fn unsupported_server(
    mut reader: impl AsyncRead + Unpin,
    mut writer: impl AsyncWrite + Unpin,
    requests: Arc<AtomicUsize>,
) {
    while let Ok(msg) = protocol::read_client_message(&mut reader).await {
        requests.fetch_add(1, Ordering::SeqCst);
        let response = ServerMessage {
            request_id: msg.request_id,
            message: Some(server_message::Message::Error(ErrorResponse {
                code: ErrorCode::Unsupported.into(),
                message: "no".into(),
            })),
        };
        if protocol::write_server_message(&mut writer, &response)
            .await
            .is_err()
        {
            break;
        }
    }
}

#[tokio::test]
async fn unsupported_from_the_helper_is_served_by_the_shell_and_sticks() {
    let requests = Arc::new(AtomicUsize::new(0));
    let (client_stream, server_stream) = tokio::io::duplex(4096);
    let (server_read, server_write) = tokio::io::split(server_stream);
    let (client_read, client_write) = tokio::io::split(client_stream);
    tokio::spawn(unsupported_server(
        server_read.compat(),
        server_write.compat_write(),
        Arc::clone(&requests),
    ));
    let executor = executor::Background::default();
    let (client, _events, _host_responses) =
        RemoteServerClient::new(client_read.compat(), client_write.compat_write(), &executor);

    let router = FeatureRouter::new();
    let host = HostId::new("contract-host".to_string());
    let session = warp_core::SessionId::from(1u64);
    router.register_out_of_band(session, Arc::new(LocalShExecutor));
    let backend = router.backend(&host, Some(Arc::new(client)), [session]);

    // No capability list was sent, so the router starts out trusting the helper.
    assert_eq!(backend.kind(Family::GitWrite), BackendKind::Helper);

    let err = backend
        .git_stage(GitStageRequest::default())
        .await
        .expect_err("neither backend implements it in P0");
    assert!(err.is_unsupported(), "{err:?}");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "the helper was asked once"
    );

    // From now on the family goes straight to the shell: the helper is not asked again.
    assert_eq!(backend.kind(Family::GitWrite), BackendKind::Shell);
    let _ = backend.git_stage(GitStageRequest::default()).await;
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    // Other families are untouched.
    assert_eq!(backend.kind(Family::GitRead), BackendKind::Helper);
}
