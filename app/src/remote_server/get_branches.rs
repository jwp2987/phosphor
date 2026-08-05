//! Response helpers for the `GetBranches` remote-server RPC (the code-review
//! branch picker over SSH). The daemon handler in `server_model.rs` runs the
//! git listing; these pure functions build the proto response so they can be
//! unit-tested without the async handler pipeline.

use super::proto::{
    get_branches_response, BranchInfo, GetBranchesError, GetBranchesResponse, GetBranchesSuccess,
};

/// Builds a success response from `(branch_name, is_main)` pairs, as returned
/// by `LocalDiffStateModel::get_all_branches`.
pub(super) fn success_response(branches: Vec<(String, bool)>) -> GetBranchesResponse {
    GetBranchesResponse {
        result: Some(get_branches_response::Result::Success(GetBranchesSuccess {
            branches: branches
                .into_iter()
                .map(|(name, is_main)| BranchInfo { name, is_main })
                .collect(),
        })),
    }
}

/// Builds an error response carrying `message`.
pub(super) fn error_response(message: String) -> GetBranchesResponse {
    GetBranchesResponse {
        result: Some(get_branches_response::Result::Error(GetBranchesError { message })),
    }
}

/// Maps the git listing result into a proto response.
pub(super) fn branches_result_to_response(
    result: anyhow::Result<Vec<(String, bool)>>,
) -> GetBranchesResponse {
    match result {
        Ok(branches) => success_response(branches),
        Err(e) => error_response(format!("{e:#}")),
    }
}

#[cfg(test)]
#[path = "get_branches_tests.rs"]
mod tests;
