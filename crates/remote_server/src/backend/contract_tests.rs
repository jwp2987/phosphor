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

fn shell_subject() -> Subject {
    Subject {
        name: "shell(local sh)",
        backend: Arc::new(ShellBackend::new(Some(Arc::new(LocalShExecutor)))),
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
