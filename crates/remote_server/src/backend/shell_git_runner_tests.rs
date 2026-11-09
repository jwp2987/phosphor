use std::path::Path;
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use super::*;
use crate::backend::executor::{ChannelKind, ExecError, ExecOutput};
use crate::shellcmd::ShellKind;
use crate::shellcmd::git::{CommitChainMode, git_text, run_commit_chain};

/// Runs the script in a local `sh` (the same channel the contract harness uses).
#[derive(Debug)]
struct LocalSh {
    seen: Mutex<Vec<String>>,
}

impl OutOfBandExecutor for LocalSh {
    fn channel_kind(&self) -> ChannelKind {
        ChannelKind::LocalSh
    }
    fn shell_kind(&self) -> ShellKind {
        ShellKind::Posix
    }
    fn execute(&self, request: ExecRequest) -> BoxFuture<'_, Result<ExecOutput, ExecError>> {
        Box::pin(async move {
            self.seen.lock().unwrap().push(request.command.clone());
            let out = std::process::Command::new("sh")
                .arg("-c")
                .arg(&request.command)
                .output()
                .map_err(|e| ExecError::Failed(e.to_string()))?;
            Ok(ExecOutput {
                stdout: out.stdout,
                stderr: out.stderr,
                exit_code: out.status.code(),
                truncated: false,
            })
        })
    }
}

fn runner() -> (ShellGitRunner, Arc<LocalSh>) {
    let sh = Arc::new(LocalSh {
        seen: Mutex::new(Vec::new()),
    });
    (ShellGitRunner::new(sh.clone()), sh)
}

#[test]
fn script_quotes_cwd_args_and_sets_the_shared_env() {
    let (r, _) = runner();
    let s = r.git_script(
        &["log", "--format=a b", "it's"],
        None,
        Path::new("/my repo"),
    );
    assert_eq!(
        s,
        "cd '/my repo' && GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 git '-c' 'diff.autoRefreshIndex=false' 'log' '--format=a b' 'it'\\''s' </dev/null"
    );
}

#[test]
fn stdin_travels_as_base64_and_path_env_is_quoted() {
    let r = runner().0.with_path_env(Some("/a b:/bin".into()));
    let s = r.git_script(&["apply", "-"], Some(b"hi"), Path::new("/r"));
    assert!(
        s.starts_with("printf %s 'aGk=' | { base64 -d 2>/dev/null || base64 -D; } | { cd '/r' && ")
    );
    assert!(s.contains("PATH='/a b:/bin' git"));
    assert!(
        r.gh_script(&["pr", "view"], Path::new("/r"))
            .contains("gh 'pr' 'view' </dev/null")
    );
}

#[test]
fn runs_real_git_through_the_executor_when_available() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let (r, _) = runner();
    let g = |args: &[&str]| futures_lite::future::block_on(git_text(&r, repo, args));
    g(&["init", "-q"]).unwrap();
    g(&["config", "user.email", "t@example.com"]).unwrap();
    g(&["config", "user.name", "t"]).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    let (commits, upstream, pr) = futures_lite::future::block_on(run_commit_chain(
        &r,
        repo,
        CommitChainMode::CommitOnly,
        "first",
        true,
        "main",
    ))
    .unwrap();
    assert_eq!((upstream, pr), (None, None));
    let _ = commits;
    let log = g(&["log", "--format=%s"]).unwrap();
    assert_eq!(log.trim(), "first");
    // A failing command carries git's own stderr in the shared error shape.
    let e = g(&["rev-parse", "--verify", "nope^{}"])
        .unwrap_err()
        .to_string();
    assert!(e.starts_with("Git command failed: "), "{e}");
}

#[test]
fn patch_over_stdin_reaches_git_apply() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let (r, _) = runner();
    let g = |args: &[&str]| futures_lite::future::block_on(git_text(&r, repo, args));
    g(&["init", "-q"]).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    g(&["add", "a.txt"]).unwrap();
    let patch =
        b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n one\n+two\n";
    let out = futures_lite::future::block_on(crate::shellcmd::git::git_text_stdin(
        &r,
        repo,
        &["apply", "--cached", "--recount", "-"],
        Some(patch),
    ));
    assert!(out.is_ok(), "{out:?}");
    assert!(g(&["diff", "--cached"]).unwrap().contains("+two"));
}
