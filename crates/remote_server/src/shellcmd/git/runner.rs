//! The one door from the git algorithms to the outside world.

use std::path::Path;

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;

/// Arguments every runner puts before the caller's (`git -c diff.autoRefreshIndex=false ...`).
pub const GIT_BASE_ARGS: [&str; 2] = ["-c", "diff.autoRefreshIndex=false"];

/// Environment every runner sets on the git child: no optional index locks (our background reads
/// must not fight the user's git) and no terminal prompt (a credential prompt fails fast instead
/// of hanging a spinner).
pub const GIT_ENV: [(&str, &str); 2] = [("GIT_OPTIONAL_LOCKS", "0"), ("GIT_TERMINAL_PROMPT", "0")];

/// What a finished command printed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GitOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    /// `None` when the process was killed by a signal.
    pub exit_code: Option<i32>,
}

impl GitOutput {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Runs `git` (and `gh`) somewhere: locally, in the helper, or on a host over the out-of-band
/// executor. Implementations MUST
///
/// * prepend [`GIT_BASE_ARGS`] and set [`GIT_ENV`] for `run`;
/// * run in `cwd`;
/// * feed `stdin` (when `Some`) to the process and close it, otherwise give it no stdin;
/// * return `Err` only when the process could not be run at all (the message is surfaced
///   verbatim); a non-zero exit is an `Ok(GitOutput)` the caller classifies, see [`git_text`].
pub trait GitRunner: Send + Sync {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>>;

    /// Runs `gh <args>` in `cwd`. Runners that cannot (the default) report it unavailable.
    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        let _ = (args, cwd);
        Box::pin(async { Err(anyhow!("Failed to execute gh command: not available here")) })
    }
}

/// Runs git and applies the app's exit policy: exit 0 is success; exit 1 with non-empty stdout
/// is also success (`git diff` reports differences that way); anything else is
/// `Git command failed: <stderr>, <stdout>`.
pub async fn git_text(runner: &dyn GitRunner, cwd: &Path, args: &[&str]) -> Result<String> {
    git_text_stdin(runner, cwd, args, None).await
}

/// [`git_text`] with bytes on the child's stdin.
pub async fn git_text_stdin(
    runner: &dyn GitRunner,
    cwd: &Path,
    args: &[&str],
    stdin: Option<&[u8]>,
) -> Result<String> {
    let output = runner.run(args, stdin, cwd).await?;
    let stdout = output.stdout_lossy();
    if output.success() || (output.exit_code == Some(1) && !stdout.is_empty()) {
        Ok(stdout)
    } else {
        Err(anyhow!(
            "Git command failed: {}, {}",
            output.stderr_lossy(),
            stdout
        ))
    }
}

/// Runs `gh`: only exit 0 is success; failure is `gh command failed: <stderr>`.
pub async fn gh_text(runner: &dyn GitRunner, cwd: &Path, args: &[&str]) -> Result<String> {
    let output = runner.run_gh(args, cwd).await?;
    if output.success() {
        Ok(output.stdout_lossy())
    } else {
        Err(anyhow!("gh command failed: {}", output.stderr_lossy()))
    }
}
