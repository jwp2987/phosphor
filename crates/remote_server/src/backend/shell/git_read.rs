//! Family `git_read` on the shell arm: the branch picker's list and the PR dialog's committed
//! file list over the out-of-band channel (#1872). The same code the helper's handlers use
//! ([`crate::shellcmd::git::get_all_branches`] and friends), through a [`ShellGitRunner`].

use std::path::Path;

use super::super::{BackendError, Family, ShellBackend};
use crate::proto::{
    BranchInfo, FileChangeEntry, GetBranches, GetBranchesError, GetBranchesResponse,
    GetBranchesSuccess, GetCommittedBranchFilesError, GetCommittedBranchFilesRequest,
    GetCommittedBranchFilesResponse, GetCommittedBranchFilesSuccess, get_branches_response,
    get_committed_branch_files_response,
};
use crate::shellcmd::git::{get_all_branches, get_committed_branch_file_entries};

impl ShellBackend {
    pub(in crate::backend) async fn get_branches_shell(
        &self,
        request: GetBranches,
    ) -> Result<GetBranchesResponse, BackendError> {
        log::debug!("shell arm serving {:?} (branches)", Family::GitRead);
        let runner = self.git_runner("get_branches")?;
        let result = get_all_branches(
            &runner,
            Path::new(&request.repo_path),
            request.max_branch_count.map(|c| c as usize),
            request.include_remotes,
        )
        .await;
        Ok(GetBranchesResponse {
            result: Some(match result {
                Ok(branches) => get_branches_response::Result::Success(GetBranchesSuccess {
                    branches: branches
                        .into_iter()
                        .map(|(name, is_main)| BranchInfo { name, is_main })
                        .collect(),
                }),
                Err(e) => get_branches_response::Result::Error(GetBranchesError {
                    message: format!("{e:#}"),
                }),
            }),
        })
    }

    pub(in crate::backend) async fn get_committed_branch_files_shell(
        &self,
        request: GetCommittedBranchFilesRequest,
    ) -> Result<GetCommittedBranchFilesResponse, BackendError> {
        log::debug!("shell arm serving {:?} (committed files)", Family::GitRead);
        let runner = self.git_runner("get_committed_branch_files")?;
        let result =
            get_committed_branch_file_entries(&runner, Path::new(&request.repo_path)).await;
        Ok(GetCommittedBranchFilesResponse {
            result: Some(match result {
                Ok(files) => get_committed_branch_files_response::Result::Success(
                    GetCommittedBranchFilesSuccess {
                        files: files
                            .into_iter()
                            .map(|(path, additions, deletions)| FileChangeEntry {
                                path,
                                additions,
                                deletions,
                            })
                            .collect(),
                    },
                ),
                Err(e) => get_committed_branch_files_response::Result::Error(
                    GetCommittedBranchFilesError {
                        message: format!("{e:#}"),
                    },
                ),
            }),
        })
    }
}
