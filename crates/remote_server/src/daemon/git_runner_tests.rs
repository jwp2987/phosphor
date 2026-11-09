use super::*;
use crate::shellcmd::git::{apply_patch_cached, detect_current_branch, git_text, run_commit};

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    futures_lite::future::block_on(f)
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

#[test]
fn shared_algorithms_run_on_the_helper_runner() {
    if !have_git() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let repo = dir.path();
    let r = HelperGitRunner::default();
    block(git_text(&r, repo, &["init", "-q", "-b", "trunk"])).unwrap();
    block(git_text(
        &r,
        repo,
        &["config", "user.email", "t@example.com"],
    ))
    .unwrap();
    block(git_text(&r, repo, &["config", "user.name", "t"])).unwrap();
    // Unborn branch: rev-parse fails, `branch --show-current` answers.
    assert_eq!(block(detect_current_branch(&r, repo)).unwrap(), "trunk");
    std::fs::write(repo.join("a.txt"), "one\n").unwrap();
    block(run_commit(&r, repo, "first", true)).unwrap();
    assert_eq!(block(detect_current_branch(&r, repo)).unwrap(), "trunk");

    // Hunk staging from stdin.
    std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
    let patch =
        b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n one\n+two\n";
    block(apply_patch_cached(&r, repo, "-", Some(patch), false, true)).unwrap();
    assert!(
        block(git_text(&r, repo, &["diff", "--cached"]))
            .unwrap()
            .contains("+two")
    );
}

#[test]
fn a_missing_program_is_a_spawn_error_not_an_exit_code() {
    let r = HelperGitRunner::default();
    let e = block(r.run(&["--version"], None, Path::new("/definitely/not/here")))
        .unwrap_err()
        .to_string();
    assert!(e.starts_with("Failed to execute git command: "), "{e}");
}

/// The git child never gets a way to prompt (owner ruling): `git` aliases starting with `!`
/// run `env`, which shows exactly what the child was given.
#[test]
fn the_helper_runner_never_lets_git_prompt() {
    if !have_git() {
        return;
    }
    let dir = tempfile::tempdir().unwrap();
    let r = HelperGitRunner::default();
    let out = block(git_text(&r, dir.path(), &["-c", "alias.penv=!env", "penv"])).unwrap();
    for want in ["GIT_TERMINAL_PROMPT=0", "SSH_ASKPASS_REQUIRE=never"] {
        assert!(out.lines().any(|l| l == want), "missing {want}: {out}");
    }
    assert!(
        out.lines()
            .any(|l| l.starts_with("GIT_SSH_COMMAND=") && l.ends_with("-o BatchMode=yes")),
        "{out}"
    );
    assert!(!out.lines().any(|l| l.starts_with("GIT_ASKPASS=")), "{out}");
    assert!(!out.lines().any(|l| l.starts_with("SSH_ASKPASS=")), "{out}");
}
