use std::path::Path;
use std::sync::Mutex;

use futures::future::BoxFuture;

use super::*;
use crate::shellcmd::git::{GitOutput, GitRunner};

#[test]
fn repository_info_reads_name_and_owner_without_a_url() {
    assert_eq!(
        repository_info_from_gh_output(r#"{"name":"widgets","owner":{"login":"acme"}}"#).unwrap(),
        RepositoryInfo {
            name: "widgets".into(),
            owner: Some("acme".into()),
            host: None,
        }
    );
}

#[test]
fn repository_info_reads_the_host_from_the_url() {
    let info = repository_info_from_gh_output(
        r#"{"name":"widgets","owner":{"login":"acme"},"url":"https://github.com/acme/widgets"}"#,
    )
    .unwrap();
    assert_eq!(info.host.as_deref(), Some("github.com"));
}

#[test]
fn repository_info_rejects_missing_empty_and_malformed() {
    assert!(repository_info_from_gh_output(r#"{"owner":{"login":"acme"}}"#).is_err());
    assert!(repository_info_from_gh_output(r#"{"name":"w","owner":{}}"#).is_err());
    assert!(repository_info_from_gh_output(r#"{"name":"","owner":{"login":"a"}}"#).is_err());
    assert!(repository_info_from_gh_output(r#"{"name":"w","owner":{"login":""}}"#).is_err());
    assert!(repository_info_from_gh_output("not json").is_err());
}

#[test]
fn host_from_url_handles_ports_user_info_ipv6_and_junk() {
    assert_eq!(
        host_from_url("https://GitHub.Example.com:8443/a/b").as_deref(),
        Some("github.example.com")
    );
    assert_eq!(
        host_from_url("ssh://git@github.com/a/b").as_deref(),
        Some("github.com")
    );
    assert_eq!(
        host_from_url("https://[::1]:80/x").as_deref(),
        Some("[::1]")
    );
    assert_eq!(host_from_url("github.com/a/b"), None);
    assert_eq!(host_from_url("https:///a"), None);
    assert_eq!(host_from_url("://x"), None);
}

#[test]
fn pr_info_reads_every_field_and_names_the_missing_one() {
    let pr = pr_info_from_gh_output(
        r#"{"number":7,"url":"https://github.com/a/b/pull/7","state":"OPEN","isDraft":true,"baseRefName":"main"}"#,
    )
    .unwrap();
    assert_eq!(
        pr,
        PrInfo {
            number: 7,
            url: "https://github.com/a/b/pull/7".into(),
            state: "OPEN".into(),
            draft: true,
            base_branch: "main".into(),
        }
    );
    let err = pr_info_from_gh_output(r#"{"number":7}"#).unwrap_err();
    assert!(err.to_string().contains("'url'"), "{err}");
    assert!(pr_info_from_gh_output("").is_err());
}

/// Answers by matching the first argument; records every call.
struct Scripted {
    calls: Mutex<Vec<String>>,
    git: fn(&[&str]) -> GitOutput,
    gh: fn(&[&str]) -> GitOutput,
}

fn out(code: i32, stdout: &str, stderr: &str) -> GitOutput {
    GitOutput {
        stdout: stdout.as_bytes().to_vec(),
        stderr: stderr.as_bytes().to_vec(),
        exit_code: Some(code),
    }
}

impl GitRunner for Scripted {
    fn run<'a>(
        &'a self,
        args: &'a [&'a str],
        _stdin: Option<&'a [u8]>,
        _cwd: &'a Path,
    ) -> BoxFuture<'a, anyhow::Result<GitOutput>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("git {}", args.join(" ")));
        let result = (self.git)(args);
        Box::pin(async move { Ok(result) })
    }
    fn run_gh<'a>(
        &'a self,
        args: &'a [&'a str],
        _cwd: &'a Path,
    ) -> BoxFuture<'a, anyhow::Result<GitOutput>> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("gh {}", args.join(" ")));
        let result = (self.gh)(args);
        Box::pin(async move { Ok(result) })
    }
}

fn on_branch(args: &[&str]) -> GitOutput {
    match args[0] {
        "rev-parse" => out(0, "true\n", ""),
        "symbolic-ref" => out(0, "feature\n", ""),
        _ => out(1, "", "unexpected"),
    }
}

#[test]
fn a_pr_lookup_with_no_pr_is_none_not_an_error() {
    let r = Scripted {
        calls: Mutex::new(Vec::new()),
        git: on_branch,
        gh: |_| out(1, "", "no pull requests found for branch \"feature\""),
    };
    let got = futures_lite::future::block_on(get_pr_for_branch(&r, Path::new("/r")));
    assert_eq!(got.unwrap(), None);
}

#[test]
fn a_missing_or_unauthenticated_gh_is_an_error_the_caller_maps_to_absent() {
    let r = Scripted {
        calls: Mutex::new(Vec::new()),
        git: on_branch,
        gh: |_| {
            out(
                4,
                "",
                "To get started with GitHub CLI, please run:  gh auth login",
            )
        },
    };
    let err = futures_lite::future::block_on(get_pr_for_branch(&r, Path::new("/r")))
        .unwrap_err()
        .to_string();
    assert!(crate::shellcmd::git::is_gh_auth_error(&err), "{err}");
}

#[test]
fn a_detached_head_never_runs_gh() {
    let r = Scripted {
        calls: Mutex::new(Vec::new()),
        git: |args| match args[0] {
            "rev-parse" => out(0, "true\n", ""),
            _ => out(1, "", "fatal: ref HEAD is not a symbolic ref"),
        },
        gh: |_| panic!("gh must not run on a detached HEAD"),
    };
    let got = futures_lite::future::block_on(get_pr_for_branch(&r, Path::new("/r")));
    assert_eq!(got.unwrap(), None);
    assert!(!r.calls.lock().unwrap().iter().any(|c| c.starts_with("gh")));
}

#[test]
fn outside_a_work_tree_neither_lookup_runs_gh() {
    let r = Scripted {
        calls: Mutex::new(Vec::new()),
        git: |_| out(128, "", "fatal: not a git repository"),
        gh: |_| panic!("gh must not run outside a work tree"),
    };
    assert_eq!(
        futures_lite::future::block_on(get_pr_for_branch(&r, Path::new("/r"))).unwrap(),
        None
    );
    assert_eq!(
        futures_lite::future::block_on(get_repository_info(&r, Path::new("/r"))).unwrap(),
        None
    );
}

#[test]
fn repository_lookup_reads_gh_and_treats_no_github_remote_as_none() {
    let ok = Scripted {
        calls: Mutex::new(Vec::new()),
        git: on_branch,
        gh: |_| {
            out(
                0,
                r#"{"name":"widgets","owner":{"login":"acme"},"url":"https://github.com/acme/widgets"}"#,
                "",
            )
        },
    };
    let info = futures_lite::future::block_on(get_repository_info(&ok, Path::new("/r")))
        .unwrap()
        .unwrap();
    assert_eq!(info.owner.as_deref(), Some("acme"));
    let none = Scripted {
        calls: Mutex::new(Vec::new()),
        git: on_branch,
        gh: |_| {
            out(
                1,
                "",
                "none of the git remotes configured for this repository point to a known GitHub host",
            )
        },
    };
    assert_eq!(
        futures_lite::future::block_on(get_repository_info(&none, Path::new("/r"))).unwrap(),
        None
    );
}
