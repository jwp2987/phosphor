use std::path::Path;
use std::process::{Command, Stdio};

use super::*;
use crate::proto::{
    self, DiscardFilesRequest, FileStatusInfo, GitCommitChainMode, GitCommitChainRequest,
    GitFileStatus, GitPullRequest, GitPushRequest, GitStageRequest, git_commit_chain_response,
    git_file_status, git_pull_response, git_push_response, git_stage_response,
};
use crate::shellcmd::git::{GIT_BASE_ARGS, GIT_ENV, GitOutput, git_text, operation_probe_script};

/// A bare-bones local runner (the helper's `HelperGitRunner` lives behind the daemon feature).
struct TestRunner;

impl GitRunner for TestRunner {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        stdin: Option<&'a [u8]>,
        cwd: &'a Path,
    ) -> BoxFuture<'a, Result<GitOutput>> {
        Box::pin(async move {
            use std::io::Write as _;
            let mut cmd = Command::new("git");
            cmd.args(GIT_BASE_ARGS).args(args).current_dir(cwd);
            for (k, v) in GIT_ENV {
                cmd.env(k, v);
            }
            cmd.stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
            let mut child = cmd.spawn()?;
            if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
                pipe.write_all(bytes)?;
            }
            let out = child.wait_with_output()?;
            Ok(GitOutput {
                stdout: out.stdout,
                stderr: out.stderr,
                exit_code: out.status.code(),
            })
        })
    }
}

fn have_git() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

fn block<T>(f: impl std::future::Future<Output = T>) -> T {
    futures_lite::future::block_on(f)
}

fn g(repo: &Path, args: &[&str]) -> String {
    block(git_text(&TestRunner, repo, args)).unwrap_or_else(|e| panic!("git {args:?}: {e}"))
}

fn fs_probe(git_dir: PathBuf) -> BoxFuture<'static, Result<bool>> {
    Box::pin(async move {
        Ok(crate::shellcmd::git::OPERATION_SENTINELS
            .iter()
            .any(|s| git_dir.join(s).exists()))
    })
}

/// A work repo with one commit on `main` and a bare `origin`.
fn repo_with_origin() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let origin = dir.path().join("origin.git");
    let work = dir.path().join("work");
    std::fs::create_dir_all(&work).unwrap();
    g(
        dir.path(),
        &[
            "init",
            "-q",
            "--bare",
            "-b",
            "main",
            origin.to_str().unwrap(),
        ],
    );
    g(&work, &["init", "-q", "-b", "main"]);
    g(&work, &["config", "user.email", "t@example.com"]);
    g(&work, &["config", "user.name", "t"]);
    g(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    std::fs::write(work.join("a.txt"), "one\n").unwrap();
    g(&work, &["add", "-A"]);
    g(&work, &["commit", "-q", "-m", "first"]);
    (dir, work, origin)
}

#[test]
fn commit_chain_commits_pushes_and_reports_the_delta() {
    if !have_git() {
        return;
    }
    let (_dir, work, origin) = repo_with_origin();
    std::fs::write(work.join("b.txt"), "two\n").unwrap();
    let resp = block(commit_chain(
        &TestRunner,
        &fs_probe,
        &work,
        GitCommitChainRequest {
            repo_path: work.to_string_lossy().into(),
            message: "second".into(),
            include_unstaged: true,
            branch: "main".into(),
            mode: GitCommitChainMode::CommitAndPush as i32,
            autogenerate_pr_content: true,
        },
    ));
    let Some(git_commit_chain_response::Result::Success(ok)) = resp.result else {
        panic!("{resp:?}");
    };
    assert!(ok.pr_info.is_none());
    assert!(ok.delta.unwrap().unpushed_commits.is_empty(), "pushed");
    assert_eq!(
        g(&origin, &["log", "--format=%s", "main"]).lines().next(),
        Some("second")
    );
}

#[test]
fn push_and_pull_fast_forward_between_two_clones() {
    if !have_git() {
        return;
    }
    let (dir, work, origin) = repo_with_origin();
    g(&work, &["push", "-q", "--set-upstream", "origin", "main"]);
    // A second clone pushes a commit; the first pulls it.
    let other = dir.path().join("other");
    g(
        dir.path(),
        &[
            "clone",
            "-q",
            origin.to_str().unwrap(),
            other.to_str().unwrap(),
        ],
    );
    g(&other, &["config", "user.email", "t@example.com"]);
    g(&other, &["config", "user.name", "t"]);
    std::fs::write(other.join("c.txt"), "c\n").unwrap();
    g(&other, &["add", "-A"]);
    g(&other, &["commit", "-q", "-m", "from other"]);
    g(&other, &["push", "-q", "origin", "main"]);

    let resp = block(pull(
        &TestRunner,
        &fs_probe,
        &work,
        GitPullRequest {
            repo_path: String::new(),
            branch: "main".into(),
        },
    ));
    assert!(
        matches!(resp.result, Some(git_pull_response::Result::Success(_))),
        "{resp:?}"
    );
    assert!(work.join("c.txt").exists());

    // A local commit pushes and leaves nothing unpushed.
    std::fs::write(work.join("d.txt"), "d\n").unwrap();
    g(&work, &["add", "-A"]);
    g(&work, &["commit", "-q", "-m", "local"]);
    let resp = block(push(
        &TestRunner,
        &fs_probe,
        &work,
        GitPushRequest {
            repo_path: String::new(),
            branch: "main".into(),
        },
    ));
    let Some(git_push_response::Result::Success(delta)) = resp.result else {
        panic!("{resp:?}");
    };
    assert!(delta.unpushed_commits.is_empty());
    assert_eq!(
        g(&origin, &["log", "--format=%s", "main"]).lines().next(),
        Some("local")
    );

    // Diverged history is refused (ff-only), with git's words.
    g(&other, &["pull", "-q", "--ff-only", "origin", "main"]);
    std::fs::write(other.join("e.txt"), "e\n").unwrap();
    g(&other, &["add", "-A"]);
    g(&other, &["commit", "-q", "-m", "other2"]);
    g(&other, &["push", "-q", "origin", "main"]);
    std::fs::write(work.join("f.txt"), "f\n").unwrap();
    g(&work, &["add", "-A"]);
    g(&work, &["commit", "-q", "-m", "work2"]);
    let resp = block(pull(
        &TestRunner,
        &fs_probe,
        &work,
        GitPullRequest {
            repo_path: String::new(),
            branch: "main".into(),
        },
    ));
    let Some(git_pull_response::Result::Error(e)) = resp.result else {
        panic!("{resp:?}");
    };
    assert!(!e.message.is_empty());
}

#[test]
fn stage_takes_exactly_one_target_and_stages_paths_and_hunks() {
    if !have_git() {
        return;
    }
    let (_dir, work, _origin) = repo_with_origin();
    let stage_req = |paths: Vec<&str>, patch: Option<&str>, reverse: bool| GitStageRequest {
        repo_path: String::new(),
        paths: paths.into_iter().map(String::from).collect(),
        patch: patch.map(String::from),
        reverse,
        mode: None,
    };
    let run = |req| block(stage(&TestRunner, &fs_probe, &work, req));
    let err_of = |resp: proto::GitStageResponse| match resp.result {
        Some(git_stage_response::Result::Error(e)) => e.message,
        other => panic!("expected an error, got {other:?}"),
    };
    assert_eq!(
        err_of(run(stage_req(vec![], None, false))),
        "GitStageRequest set neither paths nor patch"
    );
    assert_eq!(
        err_of(run(stage_req(vec!["a.txt"], Some("x"), false))),
        "GitStageRequest set both paths and patch"
    );
    assert_eq!(
        err_of(run(stage_req(vec!["/etc/passwd"], None, false))),
        "GitStageRequest path must be repo-relative, got '/etc/passwd'"
    );

    // Whole file, then un-stage it.
    std::fs::write(work.join("a.txt"), "one\ntwo\n").unwrap();
    let ok = run(stage_req(vec!["a.txt"], None, false));
    assert!(
        matches!(ok.result, Some(git_stage_response::Result::Success(_))),
        "{ok:?}"
    );
    assert!(g(&work, &["diff", "--cached"]).contains("+two"));
    run(stage_req(vec!["a.txt"], None, true));
    assert!(g(&work, &["diff", "--cached"]).is_empty());

    // One hunk from a patch, then back out with reverse.
    let patch = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1,2 @@\n one\n+two\n";
    let ok = run(stage_req(vec![], Some(patch), false));
    assert!(
        matches!(ok.result, Some(git_stage_response::Result::Success(_))),
        "{ok:?}"
    );
    assert!(g(&work, &["diff", "--cached"]).contains("+two"));
    run(stage_req(vec![], Some(patch), true));
    assert!(g(&work, &["diff", "--cached"]).is_empty());
}

#[test]
fn discard_validates_then_restores_and_removes() {
    if !have_git() {
        return;
    }
    let (_dir, work, _origin) = repo_with_origin();
    let info = |path: &Path, status: git_file_status::Status| FileStatusInfo {
        path: path.to_string_lossy().into(),
        status: Some(GitFileStatus {
            status: Some(status),
        }),
    };
    let run = |req| {
        block(discard(&TestRunner, &fs_probe, &work, req, &|rel| {
            let _ = std::fs::remove_file(work.join(rel));
        }))
    };
    let err_of = |resp: proto::DiscardFilesResponse| match resp.result {
        Some(proto::discard_files_response::Result::Error(e)) => e.message,
        other => panic!("expected an error, got {other:?}"),
    };
    assert_eq!(
        err_of(run(DiscardFilesRequest::default())),
        "No files specified in DiscardFilesRequest"
    );
    let bad = run(DiscardFilesRequest {
        files: vec![FileStatusInfo {
            path: "a.txt".into(),
            status: Some(GitFileStatus {
                status: Some(git_file_status::Status::Modified(Default::default())),
            }),
        }],
        ..Default::default()
    });
    assert_eq!(
        err_of(bad),
        "Invalid file entry in DiscardFilesRequest: FileStatusInfo path is not absolute: a.txt"
    );

    std::fs::write(work.join("a.txt"), "changed\n").unwrap();
    std::fs::write(work.join("new.txt"), "untracked\n").unwrap();
    let resp = run(DiscardFilesRequest {
        files: vec![
            info(
                &work.join("a.txt"),
                git_file_status::Status::Modified(Default::default()),
            ),
            info(
                &work.join("new.txt"),
                git_file_status::Status::Untracked(Default::default()),
            ),
        ],
        ..Default::default()
    });
    assert!(
        matches!(
            resp.result,
            Some(proto::discard_files_response::Result::Success(_))
        ),
        "{resp:?}"
    );
    assert_eq!(
        std::fs::read_to_string(work.join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(!work.join("new.txt").exists());
}

#[test]
fn every_write_is_refused_while_the_repo_is_mid_operation() {
    if !have_git() {
        return;
    }
    let (_dir, work, _origin) = repo_with_origin();
    std::fs::write(work.join(".git").join("MERGE_HEAD"), "x\n").unwrap();
    let want = OPERATION_IN_PROGRESS;
    let resp = block(stage(
        &TestRunner,
        &fs_probe,
        &work,
        GitStageRequest {
            paths: vec!["a.txt".into()],
            ..Default::default()
        },
    ));
    assert!(format!("{resp:?}").contains(want), "{resp:?}");
    let resp = block(push(
        &TestRunner,
        &fs_probe,
        &work,
        GitPushRequest::default(),
    ));
    assert!(format!("{resp:?}").contains(want), "{resp:?}");
    let resp = block(commit_chain(
        &TestRunner,
        &fs_probe,
        &work,
        GitCommitChainRequest::default(),
    ));
    assert!(format!("{resp:?}").contains(want), "{resp:?}");
    assert_eq!(
        g(&work, &["log", "--format=%s"]).trim(),
        "first",
        "nothing was committed"
    );
}

#[test]
fn absolute_paths_are_recognised_in_either_hosts_encoding() {
    for abs in ["/a/b", "C:\\a", "c:/a", "\\\\server\\share"] {
        assert!(is_absolute_on_its_host(abs), "{abs}");
    }
    for rel in ["a/b", "", "C:", "C:a", "./x"] {
        assert!(!is_absolute_on_its_host(rel), "{rel}");
    }
    // Keep the probe script builder linked into this module's contract.
    assert!(operation_probe_script("/r/.git").contains("MERGE_HEAD"));
}

#[test]
fn the_ssh_command_is_batch_mode_whether_or_not_the_user_has_one() {
    use crate::shellcmd::git::git_ssh_command;
    assert_eq!(git_ssh_command(None), "ssh -o BatchMode=yes");
    assert_eq!(git_ssh_command(Some("")), "ssh -o BatchMode=yes");
    assert_eq!(
        git_ssh_command(Some("ssh -i /k -F /c")),
        "ssh -i /k -F /c -o BatchMode=yes"
    );
}
