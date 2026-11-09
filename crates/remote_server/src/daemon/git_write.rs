//! Git write requests for the helper: commit chain, push, pull, create PR, discard files, stage
//! (wave H parity, package P7, family f; capability `git_write`).
//!
//! The logic is `shellcmd::git::write`, shared with the shell backend, run through the helper's
//! own [`HelperGitRunner`]: identical argv and answers on both arms. Every handler is blocking
//! and the server runs it on a worker thread, never the connection thread. The reply is the
//! request's own response type with the old server's error contract (a bad `repo_path`, an empty
//! discard list and a malformed stage request come back as the response's error, not as a
//! protocol error). Credentials never prompt, see `git_runner.rs`.
//!
//! After a write the helper has nothing to push: the diff-state family (not built into this
//! helper yet) owns that, and the client refreshes itself after its own write.

use std::path::PathBuf;

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;

use super::git_runner::HelperGitRunner;
use crate::proto::{self, server_message};
use crate::shellcmd::git::{OPERATION_SENTINELS, write};

/// The capability this module advertises.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::GIT_WRITE;

fn canonical_repo_path(raw: &str) -> Result<PathBuf> {
    let path = crate::naming::expand_tilde(raw);
    std::fs::canonicalize(&path).map_err(|e| anyhow!("Invalid repo_path: {e}"))
}

/// The in-progress check on the helper's own filesystem.
fn probe(git_dir: PathBuf) -> BoxFuture<'static, Result<bool>> {
    Box::pin(async move {
        Ok(OPERATION_SENTINELS
            .iter()
            .any(|sentinel| git_dir.join(sentinel).exists()))
    })
}

fn block<T>(future: impl std::future::Future<Output = T>) -> T {
    futures_lite::future::block_on(future)
}

pub(super) fn commit_chain(msg: proto::GitCommitChainRequest) -> server_message::Message {
    use proto::git_commit_chain_response::Result as R;
    log::info!("Handling GitCommitChain repo={}", msg.repo_path);
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => block(write::commit_chain(
            &HelperGitRunner::default(),
            &probe,
            &repo,
            msg,
        )),
        Err(e) => proto::GitCommitChainResponse {
            result: Some(R::Error(proto::GitOpError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::GitCommitChainResponse(response)
}

pub(super) fn push(msg: proto::GitPushRequest) -> server_message::Message {
    use proto::git_push_response::Result as R;
    log::info!(
        "Handling GitPush repo={} branch={}",
        msg.repo_path,
        msg.branch
    );
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => block(write::push(&HelperGitRunner::default(), &probe, &repo, msg)),
        Err(e) => proto::GitPushResponse {
            result: Some(R::Error(proto::GitOpError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::GitPushResponse(response)
}

pub(super) fn pull(msg: proto::GitPullRequest) -> server_message::Message {
    use proto::git_pull_response::Result as R;
    log::info!(
        "Handling GitPull repo={} branch={}",
        msg.repo_path,
        msg.branch
    );
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => block(write::pull(&HelperGitRunner::default(), &probe, &repo, msg)),
        Err(e) => proto::GitPullResponse {
            result: Some(R::Error(proto::GitOpError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::GitPullResponse(response)
}

pub(super) fn create_pr(msg: proto::GitCreatePrRequest) -> server_message::Message {
    use proto::git_create_pr_response::Result as R;
    log::info!("Handling GitCreatePr repo={}", msg.repo_path);
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => block(write::create_pull_request(
            &HelperGitRunner::default(),
            &probe,
            &repo,
            msg,
        )),
        Err(e) => proto::GitCreatePrResponse {
            result: Some(R::Error(proto::GitOpError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::GitCreatePrResponse(response)
}

pub(super) fn discard_files(msg: proto::DiscardFilesRequest) -> server_message::Message {
    use proto::discard_files_response::Result as R;
    log::info!(
        "Handling DiscardFiles repo={} files={}",
        msg.repo_path,
        msg.files.len()
    );
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => {
            let root = repo.clone();
            block(write::discard(
                &HelperGitRunner::default(),
                &probe,
                &repo,
                msg,
                // A file the target branch does not have: git rm -f may have left it behind.
                &move |rel| {
                    if let Err(e) = std::fs::remove_file(root.join(rel)) {
                        if e.kind() != std::io::ErrorKind::NotFound {
                            log::warn!("Failed to remove file '{rel}' from filesystem: {e}");
                        }
                    }
                },
            ))
        }
        Err(e) => proto::DiscardFilesResponse {
            result: Some(R::Error(proto::DiscardFilesError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::DiscardFilesResponse(response)
}

pub(super) fn stage(msg: proto::GitStageRequest) -> server_message::Message {
    use proto::git_stage_response::Result as R;
    log::info!(
        "Handling GitStage repo={} reverse={}",
        msg.repo_path,
        msg.reverse
    );
    let response = match canonical_repo_path(&msg.repo_path) {
        Ok(repo) => block(write::stage(
            &HelperGitRunner::default(),
            &probe,
            &repo,
            msg,
        )),
        Err(e) => proto::GitStageResponse {
            result: Some(R::Error(proto::GitStageError {
                message: e.to_string(),
            })),
        },
    };
    server_message::Message::GitStageResponse(response)
}
