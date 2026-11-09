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
        Box::pin(async move { run_local_sh(request, None, None) })
    }
}

fn run_local_sh(
    request: ExecRequest,
    path: Option<&std::ffi::OsStr>,
    home: Option<&std::path::Path>,
) -> Result<ExecOutput, ExecError> {
    use std::io::Write as _;
    use std::process::{Command, Stdio};
    let mut command = Command::new("sh");
    command
        .arg("-c")
        .arg(&request.command)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(path) = path {
        command.env("PATH", path);
    }
    if let Some(home) = home {
        command.env("HOME", home);
    }
    let mut child = command
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
}

/// A local `sh` whose PATH holds only the tools shell mode uses, with `base64` and `openssl`
/// left out: a host that cannot decode base64.
#[derive(Debug)]
pub(super) struct NoDecoderShExecutor {
    bin: tempfile::TempDir,
}

impl NoDecoderShExecutor {
    pub(super) fn new() -> Self {
        let bin = tempfile::tempdir().unwrap();
        for tool in [
            "mkdir", "dirname", "rm", "mv", "stat", "chmod", "wc", "tr", "head", "sed", "grep",
            "cat", "readlink", "od", "printf", "sh", "env",
        ] {
            let found = std::env::var_os("PATH")
                .into_iter()
                .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
                .map(|d| d.join(tool))
                .find(|p| p.exists());
            if let Some(found) = found {
                std::os::unix::fs::symlink(found, bin.path().join(tool)).unwrap();
            }
        }
        Self { bin }
    }
}

impl OutOfBandExecutor for NoDecoderShExecutor {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::LocalSh
    }

    fn shell_kind(&self) -> ShellKind {
        ShellKind::Posix
    }

    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move { run_local_sh(request, Some(self.bin.path().as_os_str()), None) })
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

/// Family a: `ReadFileContext`. Every expectation below is a literal, so the helper and the shell
/// arm are held to the SAME answers, not to each other's.
pub(super) async fn assert_read_file_context(subject: &Subject) {
    use crate::proto::{
        LineRange, ReadFileContextFile, ReadFileContextRequest, file_context_proto::Content,
    };
    let dir = tempfile::tempdir().unwrap();
    let write = |name: &str, bytes: &[u8]| {
        let path = dir.path().join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    };
    let file = |path: &str, ranges: &[(u32, u32)]| ReadFileContextFile {
        path: path.to_string(),
        line_ranges: ranges
            .iter()
            .map(|&(start, end)| LineRange { start, end })
            .collect(),
    };
    let name = subject.name;
    let text = |c: &crate::proto::FileContextProto| match c.content.as_ref().unwrap() {
        Content::TextContent(t) => t.clone(),
        other => panic!("{name}: expected text, got {other:?}"),
    };
    let message = |f: &crate::proto::FailedFileRead| f.error.as_ref().unwrap().message.clone();

    // Whole files: text with its final newline, CRLF normalised, an apostrophe and a space in the
    // name (the shell arm quotes), non-ASCII text, an empty file.
    let plain = write("plain it's.txt", b"one\ntwo\n");
    let crlf = write("crlf.txt", b"a\r\nb");
    let uni = write("uni.txt", "héllo wörld\n日本語\n".as_bytes());
    let empty = write("empty.txt", b"");
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![
                file(&plain, &[]),
                file(&crlf, &[]),
                file(&uni, &[]),
                file(&empty, &[]),
            ],
            ..Default::default()
        })
        .await
        .unwrap();
    assert!(r.failed_files.is_empty(), "{name}: {:?}", r.failed_files);
    let texts: Vec<_> = r.file_contexts.iter().map(&text).collect();
    assert_eq!(
        texts,
        vec!["one\ntwo\n", "a\nb", "héllo wörld\n日本語\n", ""],
        "{name}"
    );
    assert_eq!(r.file_contexts[0].file_name, plain, "{name}");
    assert_eq!(r.file_contexts[0].line_count, 2, "{name}");
    assert_eq!(r.file_contexts[1].line_count, 2, "{name}");
    assert_eq!(r.file_contexts[3].line_count, 0, "{name}");
    for c in &r.file_contexts {
        assert_eq!(
            (c.line_range_start, c.line_range_end),
            (None, None),
            "{name}"
        );
        // The shell arm has one-second resolution.
        assert!(
            c.last_modified_epoch_millis
                .is_some_and(|ms| ms > 1_000_000_000_000),
            "{name}"
        );
    }

    // The byte cap cuts at a line and names the range that came back (`apply_diff` relies on it);
    // line_count is the whole file's.
    let big_body: String = (1..=3000).map(|i| format!("line {i:05}\n")).collect();
    let big = write("big.txt", big_body.as_bytes());
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![file(&big, &[])],
            max_file_bytes: Some(100),
            max_batch_bytes: None,
        })
        .await
        .unwrap();
    let c = &r.file_contexts[0];
    assert_eq!(text(c).lines().count(), 9, "{name}");
    assert_eq!(
        (c.line_range_start, c.line_range_end),
        (Some(1), Some(9)),
        "{name}"
    );
    assert_eq!(c.line_count, 3000, "{name}");

    // Line ranges.
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![file(&big, &[(2000, 2003)]), file(&plain, &[(2, 3)])],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(
        text(&r.file_contexts[0]),
        "line 02000\nline 02001\nline 02002",
        "{name}"
    );
    assert_eq!(
        (
            r.file_contexts[0].line_range_start,
            r.file_contexts[0].line_range_end
        ),
        (Some(2000), Some(2003)),
        "{name}"
    );
    assert_eq!(r.file_contexts[0].line_count, 3000, "{name}");
    assert_eq!(text(&r.file_contexts[1]), "two", "{name}");
    assert_eq!(r.file_contexts[1].line_count, 2, "{name}");

    // Per-file failures do not fail the batch; binary comes back as bytes.
    let blob = write("blob.bin", &[0, 1, 2, 3, 255, 10, 13]);
    let missing = dir
        .path()
        .join("missing.txt")
        .to_string_lossy()
        .into_owned();
    let subdir = dir.path().to_string_lossy().into_owned();
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![
                file(&missing, &[]),
                file(&blob, &[]),
                file(&subdir, &[]),
                file(&plain, &[]),
            ],
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(r.file_contexts.len(), 2, "{name}");
    assert_eq!(
        r.file_contexts[0].content,
        Some(Content::BinaryContent(vec![0, 1, 2, 3, 255, 10, 13])),
        "{name}"
    );
    assert_eq!(text(&r.file_contexts[1]), "one\ntwo\n", "{name}");
    assert_eq!(r.failed_files.len(), 2, "{name}");
    assert_eq!(r.failed_files[0].path, missing, "{name}");
    assert_eq!(message(&r.failed_files[0]), "File does not exist", "{name}");
    assert_eq!(r.failed_files[1].path, subdir, "{name}");
    assert!(
        message(&r.failed_files[1]).contains("Is a directory"),
        "{name}"
    );

    // A binary file over the cap is refused with both sizes.
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![file(&blob, &[])],
            max_file_bytes: Some(3),
            max_batch_bytes: None,
        })
        .await
        .unwrap();
    assert!(r.file_contexts.is_empty(), "{name}");
    assert!(
        message(&r.failed_files[0]).contains("too large to read"),
        "{name}"
    );

    // The batch budget is shared; files past it are named.
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest {
            files: vec![file(&plain, &[]), file(&crlf, &[])],
            max_file_bytes: None,
            max_batch_bytes: Some(8),
        })
        .await
        .unwrap();
    assert_eq!(text(&r.file_contexts[0]), "one\ntwo\n", "{name}");
    assert_eq!(r.failed_files[0].path, crlf, "{name}");
    assert!(
        message(&r.failed_files[0]).contains("total size limit"),
        "{name}"
    );

    // An empty request is an empty answer.
    let r = subject
        .backend
        .read_file_context(ReadFileContextRequest::default())
        .await
        .unwrap();
    assert!(
        r.file_contexts.is_empty() && r.failed_files.is_empty(),
        "{name}"
    );
}

/// Writes, deletes and whole-file reads (the `apply_file_diffs` write half). As above, literal
/// expectations shared by both arms.
pub(super) async fn assert_file_write(subject: &Subject) {
    use std::os::unix::fs::PermissionsExt as _;
    let name = subject.name;
    let dir = tempfile::tempdir().unwrap();
    let path_of = |rel: &str| dir.path().join(rel).to_string_lossy().into_owned();

    // A new file in directories that do not exist yet; awkward text survives byte for byte.
    let nested = path_of("a/b/it's new.txt");
    let tricky = "it's \"quoted\" $HOME `x` \\ héllo 日本語\nno final newline";
    subject
        .backend
        .write_file(nested.clone(), tricky.to_string())
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&nested).unwrap(), tricky, "{name}");
    assert_eq!(
        subject
            .backend
            .read_file_bytes(nested.clone())
            .await
            .unwrap(),
        tricky.as_bytes(),
        "{name}"
    );

    // An empty file, and a file bigger than one command's chunk.
    let empty = path_of("empty");
    subject
        .backend
        .write_file(empty.clone(), String::new())
        .await
        .unwrap();
    assert_eq!(std::fs::read(&empty).unwrap(), b"", "{name}");
    let big: String = (0..30_000).map(|i| format!("row {i}\n")).collect();
    assert!(big.len() > 2 * 64 * 1024);
    let big_path = path_of("big.txt");
    subject
        .backend
        .write_file(big_path.clone(), big.clone())
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&big_path).unwrap(), big, "{name}");

    // Overwriting keeps the mode and leaves no temporary file behind.
    let script = path_of("run.sh");
    std::fs::write(&script, "old").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o751)).unwrap();
    subject
        .backend
        .write_file(script.clone(), "new".to_string())
        .await
        .unwrap();
    assert_eq!(std::fs::read_to_string(&script).unwrap(), "new", "{name}");
    assert_eq!(
        std::fs::metadata(&script).unwrap().permissions().mode() & 0o777,
        0o751,
        "{name}"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("phosphor-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{name}: {leftovers:?}");

    // Delete; deleting what is not there is an error, not a silent success.
    subject.backend.delete_file(script.clone()).await.unwrap();
    assert!(!std::path::Path::new(&script).exists(), "{name}");
    assert!(
        subject.backend.delete_file(script.clone()).await.is_err(),
        "{name}"
    );
    assert!(
        subject.backend.read_file_bytes(script).await.is_err(),
        "{name}"
    );
}

/// Every contract assertion. Family packages append theirs here.
pub(super) async fn assert_contract(subject: &Subject) {
    assert_unimplemented_is_unsupported(subject).await;
    assert_read_file_context(subject).await;
    assert_file_write(subject).await;
    assert_repo_metadata(subject).await;
    assert_git_write_contract(subject).await;
    assert_codebase_index(subject).await;
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

// ---- repo metadata / file tree (P3) ----------------------------------------------------------

use crate::proto::repo_node_metadata;

/// (path, is_dir, ignored, loaded) of each node of each entry update, in order.
fn view(
    entries: &[crate::proto::RepoMetadataEntryUpdate],
) -> Vec<(String, Vec<(String, bool, bool, bool)>)> {
    entries
        .iter()
        .map(|e| {
            let nodes = e
                .subtree_metadata
                .iter()
                .map(|n| match n.node.clone().unwrap() {
                    repo_node_metadata::Node::Directory(d) => (d.path, true, d.ignored, d.loaded),
                    repo_node_metadata::Node::File(f) => (f.path, false, f.ignored, false),
                })
                .collect();
            (e.parent_path_to_replace.clone(), nodes)
        })
        .collect()
}

fn git_available() -> bool {
    std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

/// A real git repository: `.gitignore` (`*.log`, `target/`), `src/lib.rs`, `src/deep/x.rs`,
/// `a.txt`, an ignored `x.log` and an ignored `target/debug/`.
pub(super) fn make_repo() -> (tempfile::TempDir, std::path::PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let init = std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(init.success());
    std::fs::write(root.join(".gitignore"), "*.log\ntarget/\n").unwrap();
    std::fs::create_dir_all(root.join("src/deep")).unwrap();
    std::fs::write(root.join("src/lib.rs"), "").unwrap();
    std::fs::write(root.join("src/deep/x.rs"), "").unwrap();
    std::fs::write(root.join("a.txt"), "").unwrap();
    std::fs::write(root.join("x.log"), "").unwrap();
    std::fs::create_dir_all(root.join("target/debug")).unwrap();
    std::fs::write(root.join("target/debug/o"), "").unwrap();
    (tmp, root)
}

fn s(p: &std::path::Path) -> String {
    p.to_string_lossy().into_owned()
}

/// The file tree contract: the same requests give the same answers from the helper and the
/// shell backend (answers, not pushes; the pushes are compared by their own tests).
pub(super) async fn assert_repo_metadata(subject: &Subject) {
    let name = subject.name;
    let backend = &subject.backend;

    // Outside git: indexed_path is the directory itself, is_git false.
    let plain = tempfile::tempdir().unwrap();
    let plain_root = plain.path().canonicalize().unwrap();
    std::fs::write(plain_root.join("f.txt"), "").unwrap();
    let nav = backend
        .navigate_to_directory(s(&plain_root))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert_eq!(nav.indexed_path, s(&plain_root), "{name}");
    assert!(!nav.is_git, "{name}");

    // A path that does not exist is an invalid request, not Unsupported and not a lost connection.
    let err = backend
        .navigate_to_directory("/no/such/dir/for/phosphor".into())
        .await
        .expect_err("a missing path");
    assert!(
        !err.is_unsupported() && !err.is_connection_lost(),
        "{name}: {err:?}"
    );
    assert!(
        format!("{err:?}").contains("InvalidRequest") && format!("{err}").contains("Invalid path"),
        "{name}: {err:?}"
    );

    if !git_available() {
        eprintln!("no git on this machine; skipping the repository half of the file tree contract");
        return;
    }
    let (_tmp, root) = make_repo();

    // Inside a repository, from a subdirectory: the root comes back.
    let nav = backend
        .navigate_to_directory(s(&root.join("src/deep")))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert_eq!(nav.indexed_path, s(&root), "{name}");
    assert!(nav.is_git, "{name}");

    // Lazy load: an ordinary directory lists its children, subdirectories already listed in
    // the snapshot are loaded.
    let loaded = backend
        .load_repo_metadata_directory(s(&root), s(&root.join("src")))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert_eq!(
        (loaded.repo_path.as_str(), loaded.dir_path.as_str()),
        (s(&root).as_str(), s(&root.join("src")).as_str()),
        "{name}"
    );
    assert_eq!(
        view(&loaded.entries),
        vec![(
            s(&root.join("src")),
            vec![
                (s(&root.join("src/deep")), true, false, true),
                (s(&root.join("src/lib.rs")), false, false, false),
            ]
        )],
        "{name}"
    );

    // An ignored directory loads with everything in it ignored.
    let loaded = backend
        .load_repo_metadata_directory(s(&root), s(&root.join("target")))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert_eq!(
        view(&loaded.entries),
        vec![(
            s(&root.join("target")),
            vec![(s(&root.join("target/debug")), true, true, false)]
        )],
        "{name}"
    );

    // An empty directory is an empty answer.
    std::fs::create_dir(root.join("empty")).unwrap();
    let loaded = backend
        .load_repo_metadata_directory(s(&root), s(&root.join("empty")))
        .await
        .unwrap_or_else(|e| panic!("{name}: {e:?}"));
    assert!(loaded.entries.is_empty(), "{name}");

    // A directory outside the repository is refused.
    let outside = backend
        .load_repo_metadata_directory(s(&root), s(&plain_root))
        .await
        .expect_err("outside the repo");
    assert!(
        format!("{outside}").contains("is not a descendant of repo_path"),
        "{name}: {outside:?}"
    );
    // A directory that does not exist names the bad field.
    let missing = backend
        .load_repo_metadata_directory(s(&root), s(&root.join("missing")))
        .await
        .expect_err("a missing dir");
    assert!(
        format!("{missing}").contains("Invalid dir_path"),
        "{name}: {missing:?}"
    );
}

/// The snapshot a navigation pushes, as (parent, children) pairs, whichever way it was pushed.
fn snapshot_view(update: &repo_metadata::RepoMetadataUpdate) -> Vec<(String, Vec<String>)> {
    update
        .update_entries
        .iter()
        .map(|e| {
            let kids = e
                .subtree_metadata
                .iter()
                .map(|n| match n {
                    repo_metadata::file_tree_update::RepoNodeMetadata::Directory(d) => {
                        format!("d{}{}:{}", u8::from(d.ignored), u8::from(d.loaded), d.path)
                    }
                    repo_metadata::file_tree_update::RepoNodeMetadata::File(f) => {
                        format!("f{}:{}", u8::from(f.ignored), f.path)
                    }
                })
                .collect();
            (e.parent_path_to_replace.to_string(), kids)
        })
        .collect()
}

#[tokio::test]
async fn the_shell_backend_queues_the_snapshot_the_helper_pushes() {
    if !git_available() {
        return;
    }
    let (_tmp, root) = make_repo();

    // Shell: the pushes queued by the call.
    let shell = ShellBackend::new(Some(Arc::new(LocalShExecutor)));
    shell.navigate_to_directory(s(&root)).await.unwrap();
    let pushes = shell.take_pushes();
    assert_eq!(pushes.len(), 1);
    let ShellPush::RepoMetadataSnapshot(snapshot) = &pushes[0] else {
        panic!("expected a snapshot, got {pushes:?}");
    };
    let shell_view =
        snapshot_view(&crate::repo_metadata_proto::proto_snapshot_to_update(snapshot).unwrap());
    assert!(shell.take_pushes().is_empty(), "drained");

    // The expected tree, written out.
    let r = |rel: &str| s(&root.join(rel));
    assert_eq!(
        shell_view,
        vec![
            (
                s(&root),
                vec![
                    format!("d10:{}", r(".git")),
                    format!("d01:{}", r("src")),
                    format!("d10:{}", r("target")),
                    format!("f0:{}", r(".gitignore")),
                    format!("f0:{}", r("a.txt")),
                    format!("f1:{}", r("x.log")),
                ]
            ),
            (
                r("src"),
                vec![
                    format!("d01:{}", r("src/deep")),
                    format!("f0:{}", r("src/lib.rs"))
                ]
            ),
            (r("src/deep"), vec![format!("f0:{}", r("src/deep/x.rs"))]),
        ]
    );

    // Helper: the same snapshot, pushed on the connection.
    #[cfg(all(feature = "daemon", unix))]
    {
        let stream = crate::daemon::spawn_in_process_for_tests();
        let stream = async_io::Async::new(stream).unwrap();
        let (reader, writer) = futures::io::AsyncReadExt::split(stream);
        let executor = executor::Background::default();
        let (client, events, _host_responses) = RemoteServerClient::new(reader, writer, &executor);
        let helper = HelperBackend::new(Arc::new(client));
        let nav = helper.navigate_to_directory(s(&root)).await.unwrap();
        assert!(nav.is_git);
        let update = loop {
            match events.recv().await.expect("the connection ended") {
                crate::client::ClientEvent::RepoMetadataSnapshotReceived { update } => {
                    break update;
                }
                _ => continue,
            }
        };
        assert_eq!(snapshot_view(&update), shell_view);
    }
}

#[tokio::test]
async fn a_shell_refresh_queues_only_what_changed_and_never_before_it_is_asked() {
    if !git_available() {
        return;
    }
    let (_tmp, root) = make_repo();
    let shell = ShellBackend::new(Some(Arc::new(LocalShExecutor)));
    shell.navigate_to_directory(s(&root)).await.unwrap();
    shell.take_pushes();

    // Nothing changed: a refresh says nothing.
    shell.refresh_repo_tree(&s(&root)).await.unwrap();
    assert!(shell.take_pushes().is_empty());

    // A file appears and one vanishes behind the app's back: nothing is seen until it asks.
    std::fs::write(root.join("src/new.rs"), "").unwrap();
    std::fs::remove_file(root.join("a.txt")).unwrap();
    assert!(shell.take_pushes().is_empty());
    shell.refresh_repo_tree(&s(&root)).await.unwrap();
    let pushes = shell.take_pushes();
    assert_eq!(pushes.len(), 1);
    let ShellPush::RepoMetadataUpdate(update) = &pushes[0] else {
        panic!("expected an update, got {pushes:?}");
    };
    assert_eq!(update.repo_path, s(&root));
    assert_eq!(update.remove_entries, vec![s(&root.join("a.txt"))]);
    assert_eq!(
        view(&update.update_entries),
        vec![(
            s(&root.join("src")),
            vec![(s(&root.join("src/new.rs")), false, false, false)]
        )]
    );
    // And it is remembered: the same refresh again is silent.
    shell.refresh_repo_tree(&s(&root)).await.unwrap();
    assert!(shell.take_pushes().is_empty());
}

/// A shell backend whose app-side index store lives in a scratch directory that is never
/// cleaned up before the test process exits.
fn shell_with_index() -> (ShellBackend, std::path::PathBuf) {
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()))
        .path()
        .to_path_buf();
    let index = Arc::new(ShellIndex::new(root.clone(), "contract-host".into()));
    (
        ShellBackend::new(Some(Arc::new(LocalShExecutor))).with_index(index),
        root,
    )
}

fn shell_subject() -> Subject {
    Subject {
        name: "shell(local sh)",
        backend: Arc::new(shell_with_index().0),
    }
}

/// The printf-octal fallback (parity P2): a host with no base64 or openssl still writes, byte
/// for byte, under the same assertions as every other backend. Reads use `od` there.
#[tokio::test]
async fn the_shell_backend_writes_on_a_host_with_no_base64_decoder() {
    let shell = ShellBackend::new(Some(Arc::new(NoDecoderShExecutor::new())));
    assert_eq!(
        shell.decode_tool().await.unwrap(),
        None,
        "the fake host must have no decoder"
    );
    let subject = Subject {
        name: "shell(no base64 decoder)",
        backend: Arc::new(shell),
    };
    assert_file_write(&subject).await;
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

// ---- buffers (P4, family c) ------------------------------------------------------------------

/// A routed backend plus, for the helper, the channel its pushes arrive on.
pub(super) struct BufferSubject {
    pub name: &'static str,
    pub backend: RoutedBackend,
    pub pushes: Option<async_channel::Receiver<crate::client::ClientEvent>>,
}

fn text_edit(start: u64, end: u64, text: &str) -> crate::proto::TextEdit {
    crate::proto::TextEdit {
        start_offset: start,
        end_offset: end,
        text: text.to_owned(),
    }
}

/// Family c: open, edit and save, an outside change, discard and keep-mine. The two arms differ
/// in exactly the way design ruling R4 says: the helper PUSHES a conflict when the file changes
/// under unsaved edits, the shell arm finds it when the user saves. Everything else is held to
/// the same literal answers.
pub(super) async fn assert_buffers(subject: &BufferSubject) {
    let name = subject.name;
    let backend = &subject.backend;
    let helper = backend.kind(Family::Buffers) == BackendKind::Helper;
    let dir = tempfile::tempdir().unwrap();
    // A space and an apostrophe in the name (the shell arm quotes), non-ASCII text.
    let file = dir.path().join("my it's.txt");
    std::fs::write(&file, "héllo\n").unwrap();
    let path = s(&file);

    // Open.
    let opened = backend.open_buffer(path.clone(), false).await.unwrap();
    assert_eq!(opened.content, "héllo\n", "{name}");
    assert!(opened.server_version >= 1, "{name}");
    let sv = opened.server_version;

    // Edit (1-indexed character offsets: after the "o") and save: the file has the new text, and
    // no temp file is left behind.
    backend
        .buffer_edit(path.clone(), sv, 10, vec![text_edit(6, 6, " there")])
        .unwrap();
    let outcome = backend
        .save_buffer_content(path.clone(), "héllo there\n".to_owned())
        .await
        .unwrap();
    assert_eq!(outcome, BufferSaveOutcome::Saved, "{name}");
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "héllo there\n",
        "{name}"
    );
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains("phosphor-tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{name}: {leftovers:?}");
    // A second save of the same text is still a save (the saved state is the new base).
    assert_eq!(
        backend
            .save_buffer_content(path.clone(), "héllo there\n".to_owned())
            .await
            .unwrap(),
        BufferSaveOutcome::Saved,
        "{name}"
    );

    // An unsaved edit, then somebody else changes the file.
    backend
        .buffer_edit(path.clone(), sv, 11, vec![text_edit(1, 1, "X")])
        .unwrap();
    // On the helper a round trip makes sure the edit was applied before the file changes.
    backend.open_buffer(path.clone(), false).await.unwrap();
    std::fs::write(&file, "outside\n").unwrap();
    if helper {
        let pushes = subject
            .pushes
            .as_ref()
            .expect("the helper subject has pushes");
        loop {
            use crate::rt::FutureExt as _;
            let event = pushes
                .recv()
                .with_timeout(std::time::Duration::from_secs(20))
                .await
                .unwrap_or_else(|_| panic!("{name}: no conflict push"))
                .unwrap();
            if let crate::client::ClientEvent::BufferConflictDetected { path: p } = event {
                assert_eq!(p, path, "{name}");
                break;
            }
        }
    } else {
        let outcome = backend
            .save_buffer_content(path.clone(), "Xhéllo there\n".to_owned())
            .await
            .unwrap();
        assert_eq!(outcome, BufferSaveOutcome::Conflict, "{name}");
    }
    // Either way nothing of the editor's text reached the disk.
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "outside\n",
        "{name}"
    );

    // Discard: take the disk's copy.
    let reloaded = backend.open_buffer(path.clone(), true).await.unwrap();
    assert_eq!(reloaded.content, "outside\n", "{name}");
    let sv = reloaded.server_version;
    if helper {
        assert!(
            sv > opened.server_version,
            "{name}: the reload bumps the version"
        );
    }

    // Edit, outside change again, and this time keep the editor's text.
    backend
        .buffer_edit(path.clone(), sv, 12, vec![text_edit(1, 1, "mine ")])
        .unwrap();
    backend.open_buffer(path.clone(), false).await.unwrap();
    std::fs::write(&file, "outside again\n").unwrap();
    let resolved = backend
        .resolve_conflict(path.clone(), sv, "mine outside\n".to_owned(), 12)
        .await
        .unwrap();
    assert!(
        matches!(
            resolved.result,
            Some(crate::proto::resolve_conflict_response::Result::Success(_))
        ),
        "{name}: {resolved:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "mine outside\n",
        "{name}"
    );

    // Closing is silent, and saving a file that is not open is an error on the helper only: the
    // shell arm has no buffer to be missing.
    backend.close_buffer(path.clone());

    // A file that is not there, or is not text, does not open.
    assert!(
        backend
            .open_buffer(s(&dir.path().join("nope")), false)
            .await
            .is_err(),
        "{name}"
    );
    let bin = dir.path().join("b.bin");
    std::fs::write(&bin, [0xff, 0xfe, 0x00, 0x01]).unwrap();
    assert!(backend.open_buffer(s(&bin), false).await.is_err(), "{name}");
}

#[tokio::test]
async fn the_shell_arm_meets_the_buffer_contract() {
    let router = FeatureRouter::new();
    let host = HostId::new("buffer-contract-shell".to_string());
    let session = warp_core::SessionId::from(2u64);
    router.register_out_of_band(session, Arc::new(LocalShExecutor));
    let subject = BufferSubject {
        name: "shell(local sh)",
        backend: router.backend(&host, None, [session]),
        pushes: None,
    };
    assert_eq!(subject.backend.kind(Family::Buffers), BackendKind::Shell);
    assert_buffers(&subject).await;
}

#[tokio::test]
async fn the_shell_arm_finds_a_same_size_same_second_change_with_the_hash() {
    // The stamp alone cannot see this change (same size, same second); the hash does.
    let router = FeatureRouter::new();
    let host = HostId::new("buffer-contract-hash".to_string());
    let session = warp_core::SessionId::from(3u64);
    router.register_out_of_band(session, Arc::new(LocalShExecutor));
    let backend = router.backend(&host, None, [session]);
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("f.txt");
    std::fs::write(&file, "aaaa").unwrap();
    backend.open_buffer(s(&file), false).await.unwrap();
    let mtime = std::fs::metadata(&file).unwrap().modified().unwrap();
    std::fs::write(&file, "bbbb").unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(mtime)
        .unwrap();
    let outcome = backend
        .save_buffer_content(s(&file), "cccc".to_owned())
        .await
        .unwrap();
    assert_eq!(outcome, BufferSaveOutcome::Conflict);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "bbbb");
    // The user then chooses to overwrite: the next save goes through.
    assert_eq!(
        backend
            .save_buffer_content(s(&file), "cccc".to_owned())
            .await
            .unwrap(),
        BufferSaveOutcome::Saved
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "cccc");
    // And a mere `touch` of an unchanged file is not a conflict.
    backend.open_buffer(s(&file), true).await.unwrap();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_modified(std::time::SystemTime::now() + std::time::Duration::from_secs(5))
        .unwrap();
    assert_eq!(
        backend
            .save_buffer_content(s(&file), "dddd".to_owned())
            .await
            .unwrap(),
        BufferSaveOutcome::Saved
    );
}

#[cfg(all(feature = "daemon", unix))]
#[tokio::test]
async fn the_helper_arm_over_an_in_process_daemon_meets_the_buffer_contract() {
    let stream = crate::daemon::spawn_in_process_for_tests();
    let stream = async_io::Async::new(stream).unwrap();
    let (reader, writer) = futures::io::AsyncReadExt::split(stream);
    let executor = executor::Background::default();
    let (client, events, _host_responses) = RemoteServerClient::new(reader, writer, &executor);
    // No capability list was asked for, so the router trusts the helper (as in the other tests).
    let router = FeatureRouter::new();
    let host = HostId::new("buffer-contract-helper".to_string());
    let session = warp_core::SessionId::from(4u64);
    router.register_out_of_band(session, Arc::new(LocalShExecutor));
    let subject = BufferSubject {
        name: "helper(in-process daemon)",
        backend: router.backend(&host, Some(Arc::new(client)), [session]),
        pushes: Some(events),
    };
    assert_eq!(subject.backend.kind(Family::Buffers), BackendKind::Helper);
    assert_buffers(&subject).await;
}

// ---- agent context: host skills and global rules (P8, family h) -------------------------------

/// A local `sh` whose `$HOME` is a fixture directory: the host whose skills the shell arm reads.
#[derive(Debug)]
struct HomeShExecutor {
    home: std::path::PathBuf,
}

impl OutOfBandExecutor for HomeShExecutor {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::LocalSh
    }

    fn shell_kind(&self) -> ShellKind {
        ShellKind::Posix
    }

    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move { run_local_sh(request, None, Some(&self.home)) })
    }
}

/// Where a backend's agent-context snapshots come from: the shell arm reads on demand and queues
/// a push when the context changed; the helper pushes on connect and when a watched directory
/// changes.
enum ContextSource {
    Shell(ShellBackend),
    #[cfg(all(feature = "daemon", unix))]
    Helper {
        events: async_channel::Receiver<crate::client::ClientEvent>,
        _client: Arc<RemoteServerClient>,
    },
}

impl ContextSource {
    /// The next snapshot (a changed context): the shell arm reads now, the helper's arrives by
    /// itself.
    async fn next(&mut self) -> crate::proto::RemoteAgentContextSnapshot {
        match self {
            ContextSource::Shell(shell) => {
                assert!(
                    shell.refresh_agent_context().await.unwrap(),
                    "the context changed, so the read must queue a snapshot"
                );
                let mut pushes = shell.take_pushes();
                assert_eq!(pushes.len(), 1, "{pushes:?}");
                let Some(ShellPush::AgentContext(snapshot)) = pushes.pop() else {
                    panic!("expected an agent context push");
                };
                snapshot
            }
            #[cfg(all(feature = "daemon", unix))]
            ContextSource::Helper { events, .. } => {
                use crate::rt::FutureExt as _;
                loop {
                    let event = events
                        .recv()
                        .with_timeout(std::time::Duration::from_secs(20))
                        .await
                        .expect("no agent context snapshot within 20 s")
                        .expect("the connection ended");
                    if let crate::client::ClientEvent::RemoteAgentContextSnapshotReceived {
                        snapshot,
                    } = event
                    {
                        return snapshot;
                    }
                }
            }
        }
    }
}

/// Writes `rel` under `home` atomically (a temp file renamed into place), so a scan that runs
/// between the create and the write never sees an empty file.
fn put(home: &std::path::Path, rel: &str, body: &[u8]) {
    let path = home.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let tmp = path.with_extension("tmp-fixture");
    std::fs::write(&tmp, body).unwrap();
    std::fs::rename(tmp, path).unwrap();
}

/// The files both backends are expected to read: two skills and a rule file, plus four that
/// are left out (not UTF-8, over the size cap, wrong depth, hidden).
fn write_agent_context_fixture(home: &std::path::Path) {
    put(home, ".agents/skills/alpha/SKILL.md", b"alpha body");
    put(home, ".claude/skills/beta/SKILL.md", b"beta body");
    put(home, ".agents/AGENTS.md", b"be kind");
    put(home, ".agents/skills/bad/SKILL.md", &[0xff, 0xfe, 0x00]);
    put(
        home,
        ".agents/skills/huge/SKILL.md",
        &vec![b'a'; crate::shellcmd::agent_context::MAX_FILE_BYTES + 1],
    );
    put(home, ".agents/skills/deep/er/SKILL.md", b"no");
    put(home, ".agents/skills/.hidden/SKILL.md", b"no");
}

/// The agent-context contract: both backends report the same skills (from `~/.agents/skills` and
/// `~/.claude/skills`, one level down, UTF-8 only, size-capped), the same global rule file, never
/// bundled skills, and a higher revision when something changed.
///
/// The caller wrote [`write_agent_context_fixture`] before the first snapshot was requested.
async fn assert_agent_context(name: &str, home: &std::path::Path, source: &mut ContextSource) {
    use crate::proto::remote_skill_proto::Source;

    let one = source.next().await;
    let h = home.to_string_lossy().into_owned();
    assert!(one.revision >= 1, "{name}");
    assert_eq!(one.home_dir, h, "{name}");
    let skills: Vec<(String, String)> = one
        .skills
        .iter()
        .map(|s| (s.path.clone(), s.content.clone()))
        .collect();
    assert_eq!(
        skills,
        vec![
            (
                format!("{h}/.agents/skills/alpha/SKILL.md"),
                "alpha body".into()
            ),
            (
                format!("{h}/.claude/skills/beta/SKILL.md"),
                "beta body".into()
            ),
        ],
        "{name}"
    );
    assert!(
        one.skills
            .iter()
            .all(|s| matches!(s.source, Some(Source::Home(_)))),
        "{name}: host skills only, never bundled"
    );
    assert_eq!(one.global_rules.len(), 1, "{name}");
    assert_eq!(one.global_rules[0].path, format!("{h}/.agents/AGENTS.md"));
    assert_eq!(one.global_rules[0].content, "be kind", "{name}");

    // A skill is added: a new, higher revision with all three.
    put(home, ".agents/skills/gamma/SKILL.md", b"gamma body");
    let two = source.next().await;
    assert!(two.revision > one.revision, "{name}");
    assert_eq!(two.skills.len(), 3, "{name}");
    assert!(
        two.skills
            .iter()
            .any(|s| s.path == format!("{h}/.agents/skills/gamma/SKILL.md")),
        "{name}"
    );

    // The rules file goes away.
    std::fs::remove_file(home.join(".agents/AGENTS.md")).unwrap();
    let three = source.next().await;
    assert!(three.revision > two.revision, "{name}");
    assert!(three.global_rules.is_empty(), "{name}");
    assert_eq!(three.skills.len(), 3, "{name}");
}

#[tokio::test]
async fn the_shell_backend_reports_the_host_agent_context() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    write_agent_context_fixture(&home);
    let shell = ShellBackend::new(Some(Arc::new(HomeShExecutor { home: home.clone() })));
    assert_agent_context("shell(local sh)", &home, &mut ContextSource::Shell(shell)).await;
}

#[cfg(all(feature = "daemon", unix))]
#[tokio::test]
async fn the_helper_pushes_the_host_agent_context_on_connect_and_on_change() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    let stream = crate::daemon::spawn_in_process_with_home_for_tests(&home);
    let stream = async_io::Async::new(stream).unwrap();
    let (reader, writer) = futures::io::AsyncReadExt::split(stream);
    let executor = executor::Background::default();
    let (client, events, _host_responses) = RemoteServerClient::new(reader, writer, &executor);
    let client = Arc::new(client);
    // The fixture is complete before initialize, which is what triggers the first scan.
    write_agent_context_fixture(&home);
    client
        .initialize(None, crate::client::ClientPreferences::default())
        .await
        .unwrap();
    let mut source = ContextSource::Helper {
        events,
        _client: client,
    };
    assert_agent_context("helper(in-process daemon)", &home, &mut source).await;
}

#[tokio::test]
async fn an_unchanged_host_context_is_not_published_again_and_revisions_only_grow() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().canonicalize().unwrap();
    put(&home, ".agents/skills/alpha/SKILL.md", b"alpha");
    let shell = ShellBackend::new(Some(Arc::new(HomeShExecutor { home: home.clone() })));
    assert!(shell.refresh_agent_context().await.unwrap());
    let first = match shell.take_pushes().pop() {
        Some(ShellPush::AgentContext(s)) => s,
        other => panic!("{other:?}"),
    };
    // Same files: nothing queued.
    assert!(!shell.refresh_agent_context().await.unwrap());
    assert!(shell.take_pushes().is_empty());
    // Touching a file that is not a skill or rule changes nothing either.
    put(&home, ".agents/notes.txt", b"x");
    assert!(!shell.refresh_agent_context().await.unwrap());
    // A change: higher revision.
    put(&home, ".agents/skills/alpha/SKILL.md", b"alpha v2");
    assert!(shell.refresh_agent_context().await.unwrap());
    let second = match shell.take_pushes().pop() {
        Some(ShellPush::AgentContext(s)) => s,
        other => panic!("{other:?}"),
    };
    assert!(second.revision > first.revision);
}

#[tokio::test]
async fn the_agent_context_over_every_posix_family_login_shell_wrapper_is_one_script() {
    // fish and nushell reach `sh -c`; the same section commands must render for all three.
    for kind in [ShellKind::Posix, ShellKind::Fish, ShellKind::Nu] {
        let script = Script::new("n0nce")
            .unwrap()
            .section(
                "skills",
                crate::shellcmd::agent_context::skills_command(kind).unwrap(),
                StderrMode::Discard,
            )
            .unwrap();
        let rendered = script.render(kind);
        assert!(rendered.contains("SKILL.md"), "{kind:?}");
    }
}

#[tokio::test]
async fn without_a_channel_the_agent_context_is_unavailable() {
    let shell = ShellBackend::new(None);
    let err = shell.refresh_agent_context().await.unwrap_err();
    assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");
}

// ---- codebase indexing (P9b) -------------------------------------------------------------------

use crate::proto::{
    CodebaseVector, GetChunks, QueryVectors, ScanCodebaseSuccess, StoreVectors,
    get_chunks_response, query_vectors_response, scan_codebase_response, store_vectors_response,
};

fn sha256_of(bytes: &[u8]) -> String {
    crate::index::types::sha256_hex(bytes)
}

async fn scan_ok(subject: &Subject, repo: &str) -> ScanCodebaseSuccess {
    match subject
        .backend
        .scan_codebase(repo.to_string())
        .await
        .unwrap_or_else(|e| panic!("{}: {e:?}", subject.name))
        .result
    {
        Some(scan_codebase_response::Result::Success(s)) => s,
        other => panic!("{}: scan failed: {other:?}", subject.name),
    }
}

async fn chunks_ok(
    subject: &Subject,
    repo: &str,
    key: &str,
    hashes: Vec<String>,
) -> crate::proto::GetChunksSuccess {
    match subject
        .backend
        .get_chunks(GetChunks {
            repo_path: repo.to_string(),
            storage_key: key.to_string(),
            content_hashes: hashes,
        })
        .await
        .unwrap_or_else(|e| panic!("{}: {e:?}", subject.name))
        .result
    {
        Some(get_chunks_response::Result::Success(s)) => s,
        other => panic!("{}: get_chunks failed: {other:?}", subject.name),
    }
}

/// Scan, fetch text, store vectors, query, edit, rescan: literal expectations shared by the
/// helper and the shell arm. The repository has no ignore files and no pruned directory names
/// (the shell arm has no ignore-file reader), so both arms see the same tree.
pub(super) async fn assert_codebase_index(subject: &Subject) {
    let name = subject.name;
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let put = |rel: &str, bytes: &[u8]| {
        let p = root.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    };
    put("a.txt", b"alpha\nbeta\n");
    put("src/b.rs", b"fn b() {}\n");
    put("it's here.txt", b"quoted\n");
    put("empty.txt", b"");
    put("bin.dat", &[0, 1, 2, 255]);
    let repo = root.to_string_lossy().into_owned();

    // The scan: only text files with a fragment, hashes of the bytes, spans as the chunker gives.
    let scan = scan_ok(subject, &repo).await;
    let paths: Vec<_> = scan.manifest.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "it's here.txt", "src/b.rs"], "{name}");
    assert_eq!(
        scan.manifest[0].file_hash,
        sha256_of(b"alpha\nbeta\n"),
        "{name}"
    );
    assert_eq!(scan.chunks.len(), 3, "{name}");
    let a = &scan.chunks[0];
    assert_eq!(a.path, "a.txt", "{name}");
    assert_eq!(
        (a.start_line, a.end_line, a.byte_start, a.byte_end),
        (0, 1, 0, 10),
        "{name}"
    );
    assert_eq!(a.content_hash, sha256_of(b"alpha\nbeta"), "{name}");
    assert_eq!(scan.root_hash.len(), 64, "{name}");
    assert_eq!(
        scan,
        scan_ok(subject, &repo).await,
        "{name}: a scan is stable"
    );

    // Text for every fragment, since none has a vector yet.
    let hash_of = |scan: &ScanCodebaseSuccess, path: &str| {
        scan.chunks
            .iter()
            .find(|c| c.path == path)
            .unwrap()
            .content_hash
            .clone()
    };
    let all: Vec<String> = scan.chunks.iter().map(|c| c.content_hash.clone()).collect();
    let got = chunks_ok(subject, &repo, "m:2", all.clone()).await;
    assert_eq!(got.already_embedded, 0, "{name}");
    assert!(got.missing_hashes.is_empty(), "{name}");
    let mut texts: Vec<_> = got.chunks.iter().map(|c| c.text.as_str()).collect();
    texts.sort();
    assert_eq!(texts, ["alpha\nbeta", "fn b() {}", "quoted"], "{name}");

    // Store vectors; they are not offered again, and another storage key starts empty.
    let vector = |hash: String, values: Vec<f32>| CodebaseVector {
        content_hash: hash,
        values,
    };
    let stored = subject
        .backend
        .store_vectors(StoreVectors {
            repo_path: repo.clone(),
            storage_key: "m:2".into(),
            vectors: vec![
                vector(hash_of(&scan, "a.txt"), vec![1.0, 0.0]),
                vector(hash_of(&scan, "src/b.rs"), vec![0.0, 1.0]),
                vector(hash_of(&scan, "it's here.txt"), vec![0.6, 0.8]),
            ],
        })
        .await
        .unwrap();
    assert!(
        matches!(
            stored.result,
            Some(store_vectors_response::Result::Success(ref s)) if s.stored == 3
        ),
        "{name}: {stored:?}"
    );
    let again = chunks_ok(subject, &repo, "m:2", all.clone()).await;
    assert_eq!(
        (again.already_embedded, again.chunks.len()),
        (3, 0),
        "{name}"
    );
    let other = chunks_ok(subject, &repo, "other:2", all.clone()).await;
    assert_eq!(
        (other.already_embedded, other.chunks.len()),
        (0, 3),
        "{name}"
    );

    // Brute-force ranking.
    let ranked = |q: Vec<f32>, k: u32| {
        let backend = Arc::clone(&subject.backend);
        let repo = repo.clone();
        async move {
            backend
                .query_vectors(QueryVectors {
                    repo_path: repo,
                    storage_key: "m:2".into(),
                    query_vector: q,
                    top_k: k,
                })
                .await
                .unwrap()
                .result
        }
    };
    let Some(query_vectors_response::Result::Success(hits)) = ranked(vec![0.0, 1.0], 2).await
    else {
        panic!("{name}: query failed");
    };
    let order: Vec<_> = hits.fragments.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(order, ["src/b.rs", "it's here.txt"], "{name}");
    assert!((hits.fragments[0].score - 1.0).abs() < 1e-5, "{name}");
    assert!((hits.fragments[1].score - 0.8).abs() < 1e-5, "{name}");
    // A query of the wrong size is an error response, not a crash.
    assert!(
        matches!(
            ranked(vec![1.0, 0.0, 0.0], 2).await,
            Some(query_vectors_response::Result::Error(_))
        ),
        "{name}"
    );

    // An edit changes one file: only its new fragment needs text, the root hash moves, and the
    // untouched files keep their vectors.
    put("a.txt", b"alpha\nbeta\ngamma\n");
    let edited = scan_ok(subject, &repo).await;
    assert_ne!(edited.root_hash, scan.root_hash, "{name}");
    let fresh = chunks_ok(
        subject,
        &repo,
        "m:2",
        edited
            .chunks
            .iter()
            .map(|c| c.content_hash.clone())
            .collect(),
    )
    .await;
    assert_eq!(fresh.already_embedded, 2, "{name}");
    assert_eq!(fresh.chunks.len(), 1, "{name}");
    assert_eq!(fresh.chunks[0].text, "alpha\nbeta\ngamma", "{name}");

    // A deleted file leaves the manifest.
    std::fs::remove_file(root.join("src/b.rs")).unwrap();
    let removed = scan_ok(subject, &repo).await;
    let paths: Vec<_> = removed.manifest.iter().map(|m| m.path.as_str()).collect();
    assert_eq!(paths, ["a.txt", "it's here.txt"], "{name}");

    // A directory that is not there is an error response from either arm.
    let missing = root.join("nope").to_string_lossy().into_owned();
    let response = subject.backend.scan_codebase(missing).await.unwrap();
    assert!(
        matches!(
            response.result,
            Some(scan_codebase_response::Result::Error(_))
        ),
        "{name}"
    );
}

/// Shell arm only: the text cached by a scan is a shortcut, not a dependency. A backend that
/// never scanned in this process (the app restarted between scan and fetch) reads the host again
/// and checks each fragment against its hash.
#[tokio::test]
async fn the_shell_arm_rereads_fragments_when_its_text_cache_is_cold() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha\nbeta\n").unwrap();
    let repo = dir.path().to_string_lossy().into_owned();
    let (first, root) = shell_with_index();
    let scan = scan_ok(
        &Subject {
            name: "first",
            backend: Arc::new(first),
        },
        &repo,
    )
    .await;
    let cold = ShellBackend::new(Some(Arc::new(LocalShExecutor)))
        .with_index(Arc::new(ShellIndex::new(root, "contract-host".into())));
    let subject = Subject {
        name: "cold",
        backend: Arc::new(cold),
    };
    let got = chunks_ok(
        &subject,
        &repo,
        "m:2",
        scan.chunks.iter().map(|c| c.content_hash.clone()).collect(),
    )
    .await;
    assert_eq!(got.chunks.len(), 1);
    assert_eq!(got.chunks[0].text, "alpha\nbeta");
    // The file changed after the scan: the fragment is reported missing, never served stale.
    std::fs::write(dir.path().join("a.txt"), "different\n").unwrap();
    let got = chunks_ok(
        &subject,
        &repo,
        "m:2",
        scan.chunks.iter().map(|c| c.content_hash.clone()).collect(),
    )
    .await;
    assert!(got.chunks.is_empty());
    assert_eq!(got.missing_hashes.len(), 1);
}

/// Shell arm only: a second scan of an unchanged tree reads no file (it only lists and hashes),
/// and a file found binary is not read again either.
#[tokio::test]
async fn the_shell_arm_reads_only_files_whose_hash_is_new() {
    #[derive(Debug)]
    struct Counting(std::sync::atomic::AtomicUsize);
    impl OutOfBandExecutor for Counting {
        fn channel_kind(&self) -> ChannelKind {
            ChannelKind::LocalSh
        }
        fn shell_kind(&self) -> ShellKind {
            ShellKind::Posix
        }
        fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
            if request.command.contains("head -c") {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
            Box::pin(async move { run_local_sh(request, None) })
        }
    }
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("a.txt"), "alpha\n").unwrap();
    std::fs::write(dir.path().join("bin.dat"), [0u8, 1, 2]).unwrap();
    let repo = dir.path().to_string_lossy().into_owned();
    let root = Box::leak(Box::new(tempfile::tempdir().unwrap()))
        .path()
        .to_path_buf();
    let counting = Arc::new(Counting(Default::default()));
    let shell = ShellBackend::new(Some(counting.clone()))
        .with_index(Arc::new(ShellIndex::new(root, "h".into())));
    shell.scan_codebase(repo.clone()).await.unwrap();
    let after_first = counting.0.load(Ordering::SeqCst);
    assert_eq!(after_first, 1, "one batched read for the two new files");
    shell.scan_codebase(repo.clone()).await.unwrap();
    assert_eq!(
        counting.0.load(Ordering::SeqCst),
        after_first,
        "nothing new: no file is read"
    );
    std::fs::write(dir.path().join("a.txt"), "alpha changed\n").unwrap();
    shell.scan_codebase(repo).await.unwrap();
    assert_eq!(counting.0.load(Ordering::SeqCst), after_first + 1);
}

/// Without index storage configured the shell arm says so (Unavailable), it does not pretend.
#[tokio::test]
async fn the_shell_arm_without_index_storage_is_unavailable() {
    let shell = ShellBackend::new(Some(Arc::new(LocalShExecutor)));
    let err = shell.scan_codebase("/".into()).await.unwrap_err();
    assert!(matches!(err, BackendError::Unavailable(_)), "{err:?}");
}
