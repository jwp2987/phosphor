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
use crate::shellcmd::{sh_quote, sh_quote_path};

mod pwsh_run;

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

    /// `env ... git -c ...` up to (not including) the caller's arguments: the fixed environment
    /// and base arguments of every git call this runner makes. The batched read path (#2156)
    /// wraps it in a shell function so a whole refresh shares the exact same prefix.
    pub(super) fn git_command_prefix(&self) -> String {
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
        let argv: Vec<String> = GIT_BASE_ARGS.iter().map(|a| sh_quote(a)).collect();
        format!("{} git {}", env.join(" "), argv.join(" "))
    }

    /// The POSIX script for one git call. A patch or commit message (`stdin`) is NOT in the
    /// script: the caller hands those bytes to the executor as the command's stdin (#1864), so
    /// they never reach a command line (no `ps`/audit-log leak, no argument-length limit).
    /// `Some` leaves the script's stdin open for git; `None` closes it (`</dev/null`).
    pub fn git_script(&self, args: &[&str], stdin: Option<&[u8]>, cwd: &Path) -> String {
        let mut command = self.git_command_prefix();
        for arg in args {
            command.push(' ');
            command.push_str(&sh_quote(arg));
        }
        let body = format!("cd {} && {command}", sh_quote_path(&cwd.to_string_lossy()));
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
            sh_quote_path(&cwd.to_string_lossy()),
            env.join(" "),
            argv.join(" ")
        )
    }

    /// Whether the git dir holds an in-progress sentinel (merge, rebase, lock...), by running
    /// [`operation_probe_script`] on the host. The execution-time guard of every git write.
    pub fn probe_operation(&self, git_dir: PathBuf) -> BoxFuture<'static, Result<bool>> {
        let this = self.clone();
        Box::pin(async move {
            if this.is_pwsh() {
                return this.probe_operation_pwsh(git_dir).await;
            }
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
        if self.is_pwsh() {
            return self.remove_files_pwsh(repo, relative_paths).await;
        }
        let files: Vec<String> = relative_paths.iter().map(|p| sh_quote(p)).collect();
        let script = format!(
            "cd {} && rm -f -- {} </dev/null",
            sh_quote_path(&repo.to_string_lossy()),
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
        let started = std::time::Instant::now();
        let result = self.exec_inner(script.as_str(), stdin).await;
        // One line per exec channel (#2156), so a prompt's side-call count can be attributed.
        log::debug!(
            "shell exec: producer=git_runner family={} elapsed_ms={} ok={}",
            exec_family(&script),
            started.elapsed().as_millis(),
            result.is_ok()
        );
        result
    }

    async fn exec_inner(&self, script: &str, stdin: Vec<u8>) -> Result<GitOutput> {
        let shell = self.executor.shell_kind();
        if shell.is_powershell() {
            return Err(anyhow!(
                "Failed to execute git command: git over PowerShell hosts is not supported"
            ));
        }
        let output = self
            .executor
            .execute(ExecRequest {
                command: shell.wrap(script),
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

/// The tool and subcommand of a git/gh script built by [`ShellGitRunner`], for the per-exec debug
/// line only (`git:status`, `gh:pr`, `other`). Never includes paths or arguments' values.
fn exec_family(script: &str) -> String {
    let after_env = |marker: &str| script.split_once(marker).map(|(_, rest)| rest.trim_start());
    if let Some(rest) = after_env(" gh ") {
        let mut words = rest.split_whitespace().map(|w| w.trim_matches('\''));
        return format!("gh:{}", words.next().unwrap_or("?"));
    }
    if let Some(rest) = script.split_once(" git ").map(|(_, r)| r) {
        // Skip the fixed base args (`-c k=v`, `--no-pager`, ...) to the subcommand.
        let mut words = rest.split_whitespace().map(|w| w.trim_matches('\''));
        while let Some(w) = words.next() {
            if w == "-c" {
                words.next();
            } else if !w.starts_with('-') && !w.contains('=') {
                return format!("git:{w}");
            }
        }
    }
    "other".to_string()
}

impl GitRunner for ShellGitRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            if self.is_pwsh() {
                return self.run_git_pwsh(args, stdin, cwd).await;
            }
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
        Box::pin(async move {
            if self.is_pwsh() {
                return self.run_gh_pwsh(args, cwd).await;
            }
            self.exec(self.gh_script(args, cwd), Vec::new()).await
        })
    }
}

#[cfg(test)]
#[path = "shell_git_runner_tests.rs"]
mod tests;
