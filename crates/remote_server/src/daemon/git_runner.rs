//! [`GitRunner`] for the helper: runs the host's `git`/`gh` with `std::process`, exactly the
//! argv and environment the app's local runner uses, so the shared algorithms in
//! `shellcmd::git` behave the same here. No tokio, no libgit2.
//!
//! The returned futures do their work when polled, synchronously: the daemon calls these from
//! its worker threads (never the connection thread), the same rule `git.rs::run_git` follows.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;

use crate::shellcmd::git::{
    GIT_BASE_ARGS, GIT_ENV, GIT_NOPROMPT_ENV, GIT_UNSET_ENV, GitOutput, GitRunner, git_ssh_command,
};

/// Runs git/gh as child processes of the helper.
#[derive(Clone, Debug, Default)]
pub struct HelperGitRunner {
    /// `PATH` for the child; `None` inherits the helper's.
    pub path_env: Option<String>,
}

fn collect(mut child: std::process::Child, stdin: Option<&[u8]>) -> std::io::Result<GitOutput> {
    if let Some(bytes) = stdin {
        if let Some(mut pipe) = child.stdin.take() {
            // A patch is small; write it from a thread so a child that fills its stdout pipe
            // before reading stdin cannot deadlock us.
            let bytes = bytes.to_vec();
            std::thread::spawn(move || {
                let _ = pipe.write_all(&bytes);
            });
        }
    }
    let out = child.wait_with_output()?;
    Ok(GitOutput {
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.status.code(),
    })
}

impl HelperGitRunner {
    fn run_blocking(&self, args: &[&str], stdin: Option<&[u8]>, cwd: &Path) -> Result<GitOutput> {
        let mut cmd = Command::new("git");
        cmd.args(GIT_BASE_ARGS).args(args).current_dir(cwd);
        for (key, value) in GIT_ENV.into_iter().chain(GIT_NOPROMPT_ENV) {
            cmd.env(key, value);
        }
        // Never prompt (owner ruling): no askpass helper, ssh in batch mode. The user runs the
        // command in their own terminal when credentials are needed.
        for key in GIT_UNSET_ENV {
            cmd.env_remove(key);
        }
        cmd.env(
            "GIT_SSH_COMMAND",
            git_ssh_command(std::env::var("GIT_SSH_COMMAND").ok().as_deref()),
        );
        if let Some(path) = &self.path_env {
            cmd.env("PATH", path);
        }
        cmd.stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        let child = cmd
            .spawn()
            .map_err(|e| anyhow!("Failed to execute git command: {e}"))?;
        collect(child, stdin).map_err(|e| anyhow!("Failed to execute git command: {e}"))
    }

    fn run_gh_blocking(&self, args: &[&str], cwd: &Path) -> Result<GitOutput> {
        let mut cmd = Command::new("gh");
        cmd.args(args)
            .current_dir(cwd)
            .env("HOMEBREW_NO_AUTO_UPDATE", "1")
            .env("GH_PROMPT_DISABLED", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(path) = &self.path_env {
            cmd.env("PATH", path);
        }
        let child = cmd
            .spawn()
            .map_err(|e| anyhow!("Failed to execute gh command: {e}"))?;
        collect(child, None).map_err(|e| anyhow!("Failed to execute gh command: {e}"))
    }
}

impl GitRunner for HelperGitRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move { self.run_blocking(args, stdin, cwd) })
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move { self.run_gh_blocking(args, cwd) })
    }
}

#[cfg(test)]
#[path = "git_runner_tests.rs"]
mod tests;
