//! Response helpers for the `GetCommittedBranchFiles` remote-server RPC (the
//! code-review file list over SSH). The daemon handler in `server_model.rs`
//! runs the git listing; these pure functions build the proto response so they
//! can be unit-tested without the async handler pipeline.

use super::proto::{
    get_committed_branch_files_response, FileChangeEntry, GetCommittedBranchFilesError,
    GetCommittedBranchFilesResponse, GetCommittedBranchFilesSuccess,
};

/// Builds a success response from `(path, additions, deletions)` tuples, as
/// returned by `DiffStateModel::get_committed_branch_file_entries`.
pub(super) fn success_response(
    files: Vec<(String, u64, u64)>,
) -> GetCommittedBranchFilesResponse {
    GetCommittedBranchFilesResponse {
        result: Some(get_committed_branch_files_response::Result::Success(
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
        )),
    }
}

/// Builds an error response carrying `message`.
pub(super) fn error_response(message: String) -> GetCommittedBranchFilesResponse {
    GetCommittedBranchFilesResponse {
        result: Some(get_committed_branch_files_response::Result::Error(
            GetCommittedBranchFilesError { message },
        )),
    }
}

/// Maps the git listing result into a proto response.
pub(super) fn files_result_to_response(
    result: anyhow::Result<Vec<(String, u64, u64)>>,
) -> GetCommittedBranchFilesResponse {
    match result {
        Ok(files) => success_response(files),
        Err(e) => error_response(format!("{e:#}")),
    }
}

#[cfg(test)]
#[path = "get_committed_branch_files_tests.rs"]
mod tests;
