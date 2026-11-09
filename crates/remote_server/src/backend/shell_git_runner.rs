//! [`GitRunner`] over an [`OutOfBandExecutor`]: the shell backend's way to run the shared git
//! algorithms (`shellcmd::git`) on the host. Never the user's pty (ruling R1): everything goes
//! through the executor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;

use super::executor::{ExecRequest, MAX_EXEC_OUTPUT_BYTES, OutOfBandExecutor};
use crate::shellcmd::git::{
    GIT_BASE_ARGS, GIT_ENV, GIT_NOPROMPT_ENV, GIT_SSH_COMMAND_SH_ASSIGNMENT, GIT_UNSET_ENV,
    GitOutput, GitRunner, operation_probe_script, parse_operation_probe,
};
use crate::shellcmd::sh_quote;

/// Runs git/gh by handing the host's login shell one POSIX script per call.
#[derive(Clone, Debug)]
pub struct ShellGitRunner {
    executor: Arc<dyn OutOfBandExecutor>,
    /// `PATH` to set for the child, when the caller knows one that finds hook tools.
    path_env: Option<String>,
}

impl ShellGitRunner {
    pub fn new(executor: Arc<dyn OutOfBandExecutor>) -> Self {
        Self {
            executor,
            path_env: None,
        }
    }

    pub fn with_path_env(mut self, path_env: Option<String>) -> Self {
        self.path_env = path_env;
        self
    }

    /// The POSIX script for one git call. A patch or commit message (`stdin`) is NOT in the
    /// script: the caller hands those bytes to the executor as the command's stdin (#1864), so
    /// they never reach a command line (no `ps`/audit-log leak, no argument-length limit).
    /// `Some` leaves the script's stdin open for git; `None` closes it (`</dev/null`).
    pub fn git_script(&self, args: &[&str], stdin: Option<&[u8]>, cwd: &Path) -> String {
        // `env -u` removes the askpass helpers; the assignments never let git or ssh prompt
        // (owner ruling), matching `daemon::git_runner` exactly.
        let mut env: Vec<String> = vec!["env".to_string()];
        env.extend(GIT_UNSET_ENV.iter().map(|k| format!("-u {k}")));
        env.extend(
            GIT_ENV
                .iter()
                .chain(GIT_NOPROMPT_ENV.iter())
                .map(|(k, v)| format!("{k}={v}")),
        );
        env.push(GIT_SSH_COMMAND_SH_ASSIGNMENT.to_string());
        if let Some(path) = &self.path_env {
            env.push(format!("PATH={}", sh_quote(path)));
        }
        let mut argv: Vec<String> = GIT_BASE_ARGS.iter().map(|a| sh_quote(a)).collect();
        argv.extend(args.iter().map(|a| sh_quote(a)));
        let body = format!(
            "cd {} && {} git {}",
            sh_quote(&cwd.to_string_lossy()),
            env.join(" "),
            argv.join(" ")
        );
        match stdin {
            None => format!("{body} </dev/null"),
            Some(_) => body,
        }
    }

    /// The POSIX script for one gh call.
    pub fn gh_script(&self, args: &[&str], cwd: &Path) -> String {
        let mut env = vec![
            "HOMEBREW_NO_AUTO_UPDATE=1".to_string(),
            "GH_PROMPT_DISABLED=1".to_string(),
        ];
        if let Some(path) = &self.path_env {
            env.push(format!("PATH={}", sh_quote(path)));
        }
        let argv: Vec<String> = args.iter().map(|a| sh_quote(a)).collect();
        format!(
            "cd {} && {} gh {} </dev/null",
            sh_quote(&cwd.to_string_lossy()),
            env.join(" "),
            argv.join(" ")
        )
    }

    /// Whether the git dir holds an in-progress sentinel (merge, rebase, lock...), by running
    /// [`operation_probe_script`] on the host. The execution-time guard of every git write.
    pub fn probe_operation(&self, git_dir: PathBuf) -> BoxFuture<'static, Result<bool>> {
        let this = self.clone();
        Box::pin(async move {
            let script = operation_probe_script(&git_dir.to_string_lossy());
            let output = this.exec(script, Vec::new()).await?;
            Ok(parse_operation_probe(&output.stdout_lossy()))
        })
    }

    /// Deletes working-tree files (repo-relative, never directories) the discard sequence left
    /// behind, the host-side half of `remove_from_fs`.
    pub async fn remove_files(&self, repo: &Path, relative_paths: &[String]) -> Result<()> {
        if relative_paths.is_empty() {
            return Ok(());
        }
        let files: Vec<String> = relative_paths.iter().map(|p| sh_quote(p)).collect();
        let script = format!(
            "cd {} && rm -f -- {} </dev/null",
            sh_quote(&repo.to_string_lossy()),
            files.join(" ")
        );
        let output = self.exec(script, Vec::new()).await?;
        if output.success() {
            Ok(())
        } else {
            Err(anyhow!("rm failed: {}", output.stderr_lossy()))
        }
    }

    async fn exec(&self, script: String, stdin: Vec<u8>) -> Result<GitOutput> {
        let shell = self.executor.shell_kind();
        if shell.is_powershell() {
            return Err(anyhow!(
                "Failed to execute git command: git over PowerShell hosts is not supported"
            ));
        }
        let output = self
            .executor
            .execute(ExecRequest {
                command: shell.wrap(&script),
                stdin,
                max_output_bytes: MAX_EXEC_OUTPUT_BYTES,
            })
            .await
            .map_err(|e| anyhow!("Failed to execute git command: {e}"))?;
        if output.truncated {
            return Err(anyhow!("Failed to execute git command: output too large"));
        }
        Ok(GitOutput {
            stdout: output.stdout,
            stderr: output.stderr,
            exit_code: output.exit_code,
        })
    }
}

impl GitRunner for ShellGitRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            self.exec(
                self.git_script(args, stdin, cwd),
                stdin.map(<[u8]>::to_vec).unwrap_or_default(),
            )
            .await
        })
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move { self.exec(self.gh_script(args, cwd), Vec::new()).await })
    }
}

#[cfg(test)]
#[path = "shell_git_runner_tests.rs"]
mod tests;
