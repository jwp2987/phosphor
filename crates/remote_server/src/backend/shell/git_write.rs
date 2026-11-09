//! The git write family on the shell backend (wave H parity, package P7, family f): the same
//! `shellcmd::git::write` functions the helper runs, over a [`ShellGitRunner`] on the
//! out-of-band executor. Never the user's pty. Patches travel base64-encoded inside the script.
//!
//! Results match the helper arm: the commit chain, push, pull and PR requests answer with their
//! response (the git failure inside it); discard and stage turn a response error into
//! [`ClientError::FileOperationFailed`], exactly as the client does for the helper.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use futures::future::BoxFuture;

use super::ShellBackend;
use crate::backend::BackendError;
use crate::backend::shell_git_runner::ShellGitRunner;
use crate::client::ClientError;
use crate::proto::{
    self, DiscardFilesRequest, GitCommitChainRequest, GitCommitChainResponse, GitCreatePrRequest,
    GitCreatePrResponse, GitPullRequest, GitPullResponse, GitPushRequest, GitPushResponse,
    GitStageRequest,
};
use crate::shellcmd::git::write;

type Probe = Box<dyn Fn(PathBuf) -> BoxFuture<'static, anyhow::Result<bool>> + Send + Sync>;

impl ShellBackend {
    fn git_write_parts(&self) -> Result<(ShellGitRunner, Probe), BackendError> {
        let executor = self.executor.as_ref().ok_or_else(|| {
            BackendError::Unavailable(
                "git writes need an out-of-band channel for remote commands, and this session has none"
                    .into(),
            )
        })?;
        let runner = ShellGitRunner::new(Arc::clone(executor));
        let prober = runner.clone();
        let probe: Probe = Box::new(move |git_dir| prober.probe_operation(git_dir));
        Ok((runner, probe))
    }

    pub(in crate::backend) async fn git_commit_chain_shell(
        &self,
        request: GitCommitChainRequest,
    ) -> Result<GitCommitChainResponse, BackendError> {
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        Ok(write::commit_chain(&runner, &*probe, &repo, request).await)
    }

    pub(in crate::backend) async fn git_push_shell(
        &self,
        request: GitPushRequest,
    ) -> Result<GitPushResponse, BackendError> {
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        Ok(write::push(&runner, &*probe, &repo, request).await)
    }

    pub(in crate::backend) async fn git_pull_shell(
        &self,
        request: GitPullRequest,
    ) -> Result<GitPullResponse, BackendError> {
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        Ok(write::pull(&runner, &*probe, &repo, request).await)
    }

    pub(in crate::backend) async fn git_create_pr_shell(
        &self,
        request: GitCreatePrRequest,
    ) -> Result<GitCreatePrResponse, BackendError> {
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        Ok(write::create_pull_request(&runner, &*probe, &repo, request).await)
    }

    pub(in crate::backend) async fn discard_files_shell(
        &self,
        request: DiscardFilesRequest,
    ) -> Result<(), BackendError> {
        use proto::discard_files_response::Result as R;
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        // The sequence may leave a working-tree file behind (one the target branch lacks). The
        // shared code reports those through a synchronous callback; collect them and delete on
        // the host afterwards.
        let leftovers = Mutex::new(Vec::<String>::new());
        let response = write::discard(&runner, &*probe, &repo, request, &|rel: &str| {
            leftovers
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(rel.to_string());
        })
        .await;
        match response.result {
            Some(R::Error(e)) => Err(ClientError::FileOperationFailed(e.message).into()),
            _ => {
                let leftovers = leftovers.into_inner().unwrap_or_else(|e| e.into_inner());
                remove_leftovers(&runner, &repo, &leftovers).await;
                Ok(())
            }
        }
    }

    pub(in crate::backend) async fn git_stage_shell(
        &self,
        request: GitStageRequest,
    ) -> Result<(), BackendError> {
        use proto::git_stage_response::Result as R;
        let (runner, probe) = self.git_write_parts()?;
        let repo = PathBuf::from(&request.repo_path);
        match write::stage(&runner, &*probe, &repo, request).await.result {
            Some(R::Error(e)) => Err(ClientError::FileOperationFailed(e.message).into()),
            _ => Ok(()),
        }
    }
}

async fn remove_leftovers(runner: &ShellGitRunner, repo: &Path, leftovers: &[String]) {
    if let Err(e) = runner.remove_files(repo, leftovers).await {
        log::warn!("Failed to remove discarded files from the host: {e}");
    }
}
