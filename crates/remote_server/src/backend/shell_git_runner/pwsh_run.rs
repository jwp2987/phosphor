//! The PowerShell door of [`ShellGitRunner`] (#1874): git and gh on a Windows host reached over
//! ssh with no helper. The script is [`crate::shellcmd::git::pwsh::run_script`]; the patch (stdin)
//! travels as the command's own stdin, never on the command line.

use std::path::{Path, PathBuf};

use anyhow::{Result, anyhow};

use super::{
    ExecRequest, GIT_BASE_ARGS, GIT_ENV, GIT_NOPROMPT_ENV, GIT_UNSET_ENV, GitOutput,
    MAX_EXEC_OUTPUT_BYTES, ShellGitRunner,
};
use crate::shellcmd::git::pwsh::{PwshCommand, run_script};
use crate::shellcmd::git::{operation_probe_script_pwsh, parse_operation_probe};

impl ShellGitRunner {
    pub(super) fn is_pwsh(&self) -> bool {
        self.executor.shell_kind().is_powershell()
    }

    /// `git <args>` in `cwd`, with the shared base arguments and environment.
    pub(super) async fn run_git_pwsh(
        &self,
        args: &[&str],
        stdin: Option<&[u8]>,
        cwd: &Path,
    ) -> Result<GitOutput> {
        let mut argv: Vec<&str> = GIT_BASE_ARGS.to_vec();
        argv.extend_from_slice(args);
        let set: Vec<(&str, &str)> = GIT_ENV
            .iter()
            .chain(GIT_NOPROMPT_ENV.iter())
            .copied()
            .collect();
        let script = run_script(&PwshCommand {
            program: "git",
            args: &argv,
            cwd: &cwd.to_string_lossy(),
            forward_stdin: stdin.is_some(),
            unset: &GIT_UNSET_ENV,
            set: &set,
            ssh_batch: true,
            path: self.path_env.as_deref(),
        });
        self.exec_pwsh(script, stdin.map(<[u8]>::to_vec).unwrap_or_default())
            .await
    }

    /// `gh <args>` in `cwd`.
    pub(super) async fn run_gh_pwsh(&self, args: &[&str], cwd: &Path) -> Result<GitOutput> {
        let script = run_script(&PwshCommand {
            program: "gh",
            args,
            cwd: &cwd.to_string_lossy(),
            forward_stdin: false,
            unset: &[],
            set: &[
                ("HOMEBREW_NO_AUTO_UPDATE", "1"),
                ("GH_PROMPT_DISABLED", "1"),
            ],
            ssh_batch: false,
            path: self.path_env.as_deref(),
        });
        self.exec_pwsh(script, Vec::new()).await
    }

    pub(super) async fn probe_operation_pwsh(&self, git_dir: PathBuf) -> Result<bool> {
        let script = operation_probe_script_pwsh(&git_dir.to_string_lossy());
        let output = self.exec_pwsh(script, Vec::new()).await?;
        Ok(parse_operation_probe(&output.stdout_lossy()))
    }

    pub(super) async fn remove_files_pwsh(
        &self,
        repo: &Path,
        relative_paths: &[String],
    ) -> Result<()> {
        let q = |s: &str| format!("'{}'", s.replace('\'', "''"));
        let files: Vec<String> = relative_paths.iter().map(|p| q(p)).collect();
        let script = format!(
            "Set-Location -LiteralPath {}\nRemove-Item -LiteralPath {} -Force -ErrorAction SilentlyContinue",
            q(&repo.to_string_lossy()),
            files.join(", ")
        );
        let output = self.exec_pwsh(script, Vec::new()).await?;
        if output.success() {
            Ok(())
        } else {
            Err(anyhow!("rm failed: {}", output.stderr_lossy()))
        }
    }

    async fn exec_pwsh(&self, script: String, stdin: Vec<u8>) -> Result<GitOutput> {
        let output = self
            .executor
            .execute(ExecRequest {
                command: self.executor.shell_kind().wrap(&script),
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
