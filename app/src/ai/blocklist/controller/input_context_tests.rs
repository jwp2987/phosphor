use super::*;

fn git(head: &str, branch: Option<&str>) -> AIAgentContext {
    AIAgentContext::Git {
        head: head.into(),
        branch: branch.map(Into::into),
    }
}

/// #892: after the agent's own `git switch`, the result context carries the branch read
/// from `.git/HEAD`, replacing the stale pre-switch value rather than adding to it.
#[test]
fn fresh_branch_replaces_stale_git_context() {
    let stale = [
        AIAgentContext::Directory {
            pwd: Some("/repo".into()),
            home_dir: None,
            are_file_symbols_indexed: false,
        },
        git("main", Some("main")),
    ];
    let out = with_fresh_git_branch(&stale, "feature/x".into());
    let gits: Vec<_> = out
        .iter()
        .filter(|c| matches!(c, AIAgentContext::Git { .. }))
        .collect();
    assert_eq!(gits, vec![&git("feature/x", Some("feature/x"))]);
    assert!(
        out.iter()
            .any(|c| matches!(c, AIAgentContext::Directory { .. }))
    );
}

#[test]
fn fresh_branch_is_added_when_context_had_none() {
    let out = with_fresh_git_branch(&[], "dev".into());
    assert_eq!(&*out, &[git("dev", Some("dev"))]);
}

#[test]
fn fresh_detached_sha_has_no_branch() {
    let out = with_fresh_git_branch(&[git("main", Some("main"))], "0123abc".into());
    assert_eq!(&*out, &[git("0123abc", None)]);
}
