//! Family d on the shell arm: `GetDiffState` over the out-of-band channel.
//!
//! The same [`crate::shellcmd::git::diff`] code the helper runs, through a [`ShellGitRunner`], so
//! the snapshot is identical. There is no subscription: the call returns the current state, and
//! the app asks again when a refresh event says the host may have changed (the prompt came back,
//! the cwd moved, the user asked, the app wrote). A path that is not a directory is the same
//! `DiffStateError` the helper gives; a directory that is not a repository is a `NotInRepository`
//! snapshot.

use std::path::Path;

use super::super::{BackendError, Family, ShellBackend};
use crate::proto::{GetDiffState, GetDiffStateResponse};
use crate::shellcmd::git::GitRunner as _;
use crate::shellcmd::git::diff::{self, Mode};

impl ShellBackend {
    pub(in crate::backend) async fn get_diff_state_shell(
        &self,
        request: GetDiffState,
    ) -> Result<GetDiffStateResponse, BackendError> {
        log::debug!("shell arm serving {:?}", Family::DiffState);
        let runner = self.git_runner("get_diff_state")?;
        let repo = Path::new(&request.repo_path);
        // Is it a directory at all? (`cd` fails on the host when it is not.)
        let probe = runner
            .run(&["rev-parse", "--git-dir"], None, repo)
            .await
            .map_err(|e| BackendError::Unavailable(e.to_string()))?;
        if !probe.success() && !probe.stderr_lossy().contains("not a git repository") {
            return Ok(diff::error_response(format!(
                "Invalid repo_path: {}",
                probe.stderr_lossy().trim()
            )));
        }
        let mode = Mode::from_proto(request.mode.as_ref());
        let snapshot = diff::compute_snapshot(&runner, repo, &request.repo_path, &mode).await;
        Ok(diff::snapshot_response(snapshot))
    }
}
