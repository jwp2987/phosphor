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
use crate::client::ClientError;
use crate::client::RemoteServerClient;
use crate::proto::{
    DiscardFilesRequest, ErrorCode, ErrorResponse, FileStatusInfo, GitCommitChainMode,
    GitCommitChainRequest, GitCreatePrRequest, GitFileStatus, GitPullRequest, GitPushRequest,
    GitStageRequest, ReadFileContextRequest, ServerMessage, git_commit_chain_response,
    git_create_pr_response, git_file_status, git_pull_response, git_push_response, server_message,
};
use crate::protocol;
use crate::rt::FutureExt as _;
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
        .read_file_context(ReadFileContextRequest::default())
        .await
        .expect_err("read_file_context is not implemented by any backend yet");
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
    assert_git_write_contract(subject).await;
}

// ---- family f: git writes --------------------------------------------------------------

fn have_git() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok()
}

fn git(repo: &std::path::Path, args: &[&str]) -> String {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// The message of a discard/stage failure (the helper and the shell backend must agree on it).
fn op_failure(result: Result<(), BackendError>, what: &str) -> String {
    match result {
        Err(BackendError::Client(ClientError::FileOperationFailed(message))) => message,
        other => panic!("{what}: expected a FileOperationFailed error, got {other:?}"),
    }
}

/// An HTTP server that demands credentials from everyone, for the no-prompt assertion.
fn spawn_credentials_required_server() -> u16 {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut buf = [0u8; 4096];
            let _ = stream.read(&mut buf);
            let _ = stream.write_all(
                b"HTTP/1.1 401 Unauthorized\r\nWWW-Authenticate: Basic realm=\"lab\"\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            );
        }
    });
    port
}

/// Git writes behave the same on every backend: commit chain, push, pull, stage (paths and a
/// hunk), discard, the in-progress guard, validation errors, and credentials that fail fast.
pub(super) async fn assert_git_write_contract(subject: &Subject) {
    if !have_git() {
        return;
    }
    let name = subject.name;
    let b = &subject.backend;
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin.git");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    git(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    git(&work, &["init", "-q", "-b", "main"]);
    git(&work, &["config", "user.email", "t@example.com"]);
    git(&work, &["config", "user.name", "t"]);
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    std::fs::write(work.join("a.txt"), "one\n").unwrap();
    let repo_path = work.to_string_lossy().into_owned();

    // Commit chain: commit + push in one request.
    let resp = b
        .git_commit_chain(GitCommitChainRequest {
            repo_path: repo_path.clone(),
            message: "first".into(),
            include_unstaged: true,
            branch: "main".into(),
            mode: GitCommitChainMode::CommitAndPush as i32,
            autogenerate_pr_content: false,
        })
        .await
        .unwrap();
    let Some(git_commit_chain_response::Result::Success(ok)) = resp.result else {
        panic!("{name}: {resp:?}");
    };
    assert!(ok.delta.unwrap().unpushed_commits.is_empty(), "{name}");
    assert_eq!(
        git(&origin, &["log", "--format=%s", "main"]).trim(),
        "first",
        "{name}"
    );

    // Stage: one target only, repo-relative, paths and hunks, both directions.
    let stage = |paths: Vec<&str>, patch: Option<&str>, reverse: bool| GitStageRequest {
        repo_path: repo_path.clone(),
        paths: paths.into_iter().map(String::from).collect(),
        patch: patch.map(String::from),
        reverse,
        mode: None,
    };
    assert_eq!(
        op_failure(b.git_stage(stage(vec![], None, false)).await, name),
        "GitStageRequest set neither paths nor patch"
    );
    assert_eq!(
        op_failure(
            b.git_stage(stage(vec!["a.txt"], Some("x"), false)).await,
            name
        ),
        "GitStageRequest set both paths and patch"
    );
    assert_eq!(
        op_failure(
            b.git_stage(stage(vec!["/etc/passwd"], None, false)).await,
            name
        ),
        "GitStageRequest path must be repo-relative, got '/etc/passwd'"
    );
    std::fs::write(work.join("a.txt"), "one\ntwo\n").unwrap();
    b.git_stage(stage(vec!["a.txt"], None, false))
        .await
        .unwrap();
    assert!(git(&work, &["diff", "--cached"]).contains("+two"), "{name}");
    b.git_stage(stage(vec!["a.txt"], None, true)).await.unwrap();
    assert!(git(&work, &["diff", "--cached"]).is_empty(), "{name}");
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n one\n+two\n";
    b.git_stage(stage(vec![], Some(patch), false))
        .await
        .unwrap();
    assert!(git(&work, &["diff", "--cached"]).contains("+two"), "{name}");
    b.git_stage(stage(vec![], Some(patch), true)).await.unwrap();
    assert!(git(&work, &["diff", "--cached"]).is_empty(), "{name}");

    // Discard: validation, then a real restore and an untracked removal.
    let discard = |files: Vec<(String, git_file_status::Status)>| DiscardFilesRequest {
        repo_path: repo_path.clone(),
        files: files
            .into_iter()
            .map(|(path, status)| FileStatusInfo {
                path,
                status: Some(GitFileStatus {
                    status: Some(status),
                }),
            })
            .collect(),
        ..Default::default()
    };
    assert_eq!(
        op_failure(b.discard_files(discard(vec![])).await, name),
        "No files specified in DiscardFilesRequest"
    );
    assert_eq!(
        op_failure(
            b.discard_files(discard(vec![(
                "a.txt".into(),
                git_file_status::Status::Modified(Default::default())
            )]))
            .await,
            name
        ),
        "Invalid file entry in DiscardFilesRequest: FileStatusInfo path is not absolute: a.txt"
    );
    std::fs::write(work.join("new.txt"), "untracked\n").unwrap();
    let canonical = std::fs::canonicalize(&work).unwrap();
    b.discard_files(discard(vec![
        (
            canonical.join("a.txt").to_string_lossy().into(),
            git_file_status::Status::Modified(Default::default()),
        ),
        (
            canonical.join("new.txt").to_string_lossy().into(),
            git_file_status::Status::Untracked(Default::default()),
        ),
    ]))
    .await
    .unwrap();
    assert_eq!(
        std::fs::read_to_string(work.join("a.txt")).unwrap(),
        "one\n",
        "{name}"
    );
    assert!(!work.join("new.txt").exists(), "{name}");

    // Push a local commit, then pull one made elsewhere (fast-forward).
    std::fs::write(work.join("b.txt"), "b\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "second"]);
    let resp = b
        .git_push(GitPushRequest {
            repo_path: repo_path.clone(),
            branch: "main".into(),
        })
        .await
        .unwrap();
    let Some(git_push_response::Result::Success(delta)) = resp.result else {
        panic!("{name}: {resp:?}");
    };
    assert!(delta.unpushed_commits.is_empty(), "{name}");
    let other = dir.path().join("other");
    git(
        dir.path(),
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    git(&other, &["config", "user.email", "t@example.com"]);
    git(&other, &["config", "user.name", "t"]);
    std::fs::write(other.join("c.txt"), "c\n").unwrap();
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "third"]);
    git(&other, &["push", "-q", "origin", "main"]);
    let resp = b
        .git_pull(GitPullRequest {
            repo_path: repo_path.clone(),
            branch: "main".into(),
        })
        .await
        .unwrap();
    assert!(
        matches!(resp.result, Some(git_pull_response::Result::Success(_))),
        "{name}: {resp:?}"
    );
    assert!(work.join("c.txt").exists(), "{name}");

    // The guard: mid-merge, every write is refused and nothing changes.
    std::fs::write(work.join(".git").join("MERGE_HEAD"), "x\n").unwrap();
    let refused = op_failure(b.git_stage(stage(vec!["a.txt"], None, false)).await, name);
    assert!(
        refused.starts_with("another git operation is in progress"),
        "{name}: {refused}"
    );
    let resp = b
        .git_push(GitPushRequest {
            repo_path: repo_path.clone(),
            branch: "main".into(),
        })
        .await
        .unwrap();
    assert!(
        matches!(&resp.result, Some(git_push_response::Result::Error(e)) if e.message.starts_with("another git operation is in progress")),
        "{name}: {resp:?}"
    );
    std::fs::remove_file(work.join(".git").join("MERGE_HEAD")).unwrap();

    // Credentials never prompt: a remote that wants a login fails fast with git's own words.
    let port = spawn_credentials_required_server();
    git(
        &work,
        &[
            "remote",
            "set-url",
            "origin",
            &format!("http://127.0.0.1:{port}/r.git"),
        ],
    );
    std::fs::write(work.join("d.txt"), "d\n").unwrap();
    git(&work, &["add", "-A"]);
    git(&work, &["commit", "-q", "-m", "fourth"]);
    let resp = b
        .git_push(GitPushRequest {
            repo_path: repo_path.clone(),
            branch: "main".into(),
        })
        .with_timeout(std::time::Duration::from_secs(60))
        .await
        .unwrap_or_else(|_| panic!("{name}: push to a remote that wants credentials hung"))
        .unwrap();
    let Some(git_push_response::Result::Error(e)) = resp.result else {
        panic!("{name}: expected an auth failure, got {resp:?}");
    };
    let lower = e.message.to_lowercase();
    assert!(
        [
            "terminal prompts disabled",
            "could not read username",
            "authentication failed"
        ]
        .iter()
        .any(|w| lower.contains(w)),
        "{name}: {}",
        e.message
    );

    // A PR cannot be made here (a throwaway repo): an error response, never a hang or a prompt.
    let resp = b
        .git_create_pr(GitCreatePrRequest {
            repo_path,
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(
        matches!(resp.result, Some(git_create_pr_response::Result::Error(_))),
        "{name}: {resp:?}"
    );
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
    assert_eq!(backend.kind(Family::FileContext), BackendKind::Helper);

    let err = backend
        .read_file_context(ReadFileContextRequest::default())
        .await
        .expect_err("neither backend implements it yet");
    assert!(err.is_unsupported(), "{err:?}");
    assert_eq!(
        requests.load(Ordering::SeqCst),
        1,
        "the helper was asked once"
    );

    // From now on the family goes straight to the shell: the helper is not asked again.
    assert_eq!(backend.kind(Family::FileContext), BackendKind::Shell);
    let _ = backend
        .read_file_context(ReadFileContextRequest::default())
        .await;
    assert_eq!(requests.load(Ordering::SeqCst), 1);
    // Other families are untouched.
    assert_eq!(backend.kind(Family::GitRead), BackendKind::Helper);
}
