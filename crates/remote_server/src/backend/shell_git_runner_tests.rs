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
            use std::io::Write as _;
            use std::process::Stdio;
            let mut child = std::process::Command::new("sh")
                .arg("-c")
                .arg(&request.command)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| ExecError::Failed(e.to_string()))?;
            if let Some(mut pipe) = child.stdin.take() {
                let _ = pipe.write_all(&request.stdin);
            }
            let out = child
                .wait_with_output()
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
        "cd '/my repo' && env -u GIT_ASKPASS -u SSH_ASKPASS GIT_OPTIONAL_LOCKS=0 GIT_TERMINAL_PROMPT=0 SSH_ASKPASS_REQUIRE=never GIT_SSH_COMMAND=\"${GIT_SSH_COMMAND:-ssh} -o BatchMode=yes\" git '-c' 'diff.autoRefreshIndex=false' 'log' '--format=a b' 'it'\\''s' </dev/null"
    );
}

#[test]
fn stdin_never_enters_the_script_and_path_env_is_quoted() {
    let r = runner().0.with_path_env(Some("/a b:/bin".into()));
    let s = r.git_script(&["apply", "-"], Some(b"hi"), Path::new("/r"));
    // The patch is the executor's stdin (#1864): the script has neither it nor its base64, and
    // leaves git's stdin open.
    assert!(s.starts_with("cd '/r' && "), "{s}");
    assert!(!s.contains("aGk=") && !s.contains("base64") && !s.contains("</dev/null"));
    assert!(s.contains("PATH='/a b:/bin' git"));
    assert!(
        r.gh_script(&["pr", "view"], Path::new("/r"))
            .contains("gh 'pr' 'view' </dev/null")
    );
}

/// #1869: a patch far over the 128 KiB single-argument limit (and a secret in it) is staged
/// through stdin; the command line the executor sees carries none of it.
#[test]
fn a_200_kib_patch_stages_and_never_appears_in_the_command_line() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let (r, sh) = runner();
    let g = |args: &[&str]| futures_lite::future::block_on(git_text(&r, repo, args));
    g(&["init", "-q"]).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    g(&["add", "a.txt"]).unwrap();
    let mut patch = String::from(
        "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2501 @@\n one\n",
    );
    for i in 0..2500 {
        patch.push_str(&format!("+SECRETLINE-{i:04}-{}\n", "x".repeat(70)));
    }
    assert!(patch.len() > 200 * 1024);
    let out = futures_lite::future::block_on(crate::shellcmd::git::git_text_stdin(
        &r,
        repo,
        &["apply", "--cached", "--recount", "-"],
        Some(patch.as_bytes()),
    ));
    assert!(out.is_ok(), "{out:?}");
    assert!(
        g(&["diff", "--cached"])
            .unwrap()
            .contains("SECRETLINE-2499")
    );
    let seen = sh.seen.lock().unwrap();
    assert!(
        seen.iter().all(|c| !c.contains("SECRETLINE")),
        "patch text reached a command line"
    );
    assert!(seen.iter().all(|c| c.len() < 4096));
}

/// #1864: a commit message goes to `git commit` on stdin, not into the command line.
#[test]
fn a_commit_message_is_stdin_not_argv() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let (r, sh) = runner();
    let g = |args: &[&str]| futures_lite::future::block_on(git_text(&r, repo, args));
    g(&["init", "-q"]).unwrap();
    g(&["config", "user.email", "t@example.com"]).unwrap();
    g(&["config", "user.name", "t"]).unwrap();
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    futures_lite::future::block_on(crate::shellcmd::git::run_commit(
        &r,
        repo,
        "subject line\n\nSECRETBODY token",
        true,
    ))
    .unwrap();
    assert_eq!(
        g(&["log", "-1", "--format=%B"]).unwrap().trim(),
        "subject line\n\nSECRETBODY token"
    );
    assert!(
        sh.seen
            .lock()
            .unwrap()
            .iter()
            .all(|c| !c.contains("SECRETBODY"))
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

/// Same probe as the helper's test, but with the host's login environment offering askpass
/// helpers and an ssh command of its own: the script must drop the helpers and keep the command.
#[test]
fn the_shell_runner_never_lets_git_prompt() {
    if std::process::Command::new("git")
        .arg("--version")
        .output()
        .is_err()
    {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let (r, _) = runner();
    let script = r.git_script(&["-c", "alias.penv=!env", "penv"], None, dir.path());
    let out = std::process::Command::new("sh")
        .arg("-c")
        .arg(&script)
        .env("GIT_ASKPASS", "/bin/false")
        .env("SSH_ASKPASS", "/bin/false")
        .env("GIT_SSH_COMMAND", "ssh -i /k")
        .output()
        .unwrap();
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for want in [
        "GIT_TERMINAL_PROMPT=0",
        "SSH_ASKPASS_REQUIRE=never",
        "GIT_SSH_COMMAND=ssh -i /k -o BatchMode=yes",
    ] {
        assert!(text.lines().any(|l| l == want), "missing {want}: {text}");
    }
    assert!(
        !text.lines().any(|l| l.starts_with("GIT_ASKPASS=")),
        "{text}"
    );
    assert!(
        !text.lines().any(|l| l.starts_with("SSH_ASKPASS=")),
        "{text}"
    );
}
