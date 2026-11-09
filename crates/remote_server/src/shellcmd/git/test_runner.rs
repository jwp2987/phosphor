//! A [`GitRunner`] over `std::process` and a throwaway repository, for the unit tests of the
//! shared git code (the helper and the shell backend have their own runners and their own tests).

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;
use futures::future::BoxFuture;

use super::{GIT_BASE_ARGS, GIT_ENV, GitOutput, GitRunner};

pub(crate) struct StdRunner {
    /// A directory put first on `PATH`, to stand in for `gh`.
    pub path_prefix: Option<PathBuf>,
}

impl StdRunner {
    pub(crate) fn new() -> Self {
        Self { path_prefix: None }
    }

    fn path(&self) -> Option<std::ffi::OsString> {
        let prefix = self.path_prefix.as_ref()?;
        let mut paths = vec![prefix.clone()];
        paths.extend(std::env::split_paths(&std::env::var_os("PATH")?));
        std::env::join_paths(paths).ok()
    }
}

fn finish(child: std::process::Child) -> Result<GitOutput> {
    let out = child.wait_with_output()?;
    Ok(GitOutput {
        stdout: out.stdout,
        stderr: out.stderr,
        exit_code: out.status.code(),
    })
}

impl GitRunner for StdRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        _stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            let mut cmd = Command::new("git");
            cmd.args(GIT_BASE_ARGS).args(args).current_dir(cwd);
            for (k, v) in GIT_ENV {
                cmd.env(k, v);
            }
            if let Some(path) = self.path() {
                cmd.env("PATH", path);
            }
            finish(
                cmd.stdin(Stdio::null())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()?,
            )
        })
    }

    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            let mut cmd = Command::new("gh");
            cmd.args(args)
                .current_dir(cwd)
                .env("GH_PROMPT_DISABLED", "1");
            if let Some(path) = self.path() {
                cmd.env("PATH", path);
            }
            let child = cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| anyhow::anyhow!("Failed to execute gh command: {e}"))?;
            finish(child)
        })
    }
}

pub(crate) fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// Runs `git args` in `dir`, panicking on failure.
pub(crate) fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .output()
        .expect("git runs");
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A repository on branch `main` with one commit holding `files`.
pub(crate) fn repo_with(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    git(root, &["init", "-q"]);
    git(root, &["symbolic-ref", "HEAD", "refs/heads/main"]);
    for (name, body) in files {
        if let Some(parent) = Path::new(name).parent() {
            std::fs::create_dir_all(root.join(parent)).unwrap();
        }
        std::fs::write(root.join(name), body).unwrap();
    }
    git(root, &["add", "-A"]);
    git(root, &["commit", "-q", "-m", "first"]);
    dir
}
