use crate::rules::git::{
    git_branch_delete_unmerged::GitBranchDeleteUnmerged, git_checkout::GitCheckout,
    git_command_not_found::GitCommandNotFound, git_push_create_pr::GitPushCreatePullRequest,
    git_push_force::GitPushForce, git_push_set_upstream::GitPushSetUpstream,
};
use crate::rules::Rule;

use super::CommandGroup;

mod git_branch_delete_unmerged;
mod git_checkout;
mod git_command_not_found;
mod git_push_create_pr;
mod git_push_force;
mod git_push_set_upstream;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(
        &["git"],
        vec![
            GitCommandNotFound.to_arc(),
            GitCheckout.to_arc(),
            GitPushCreatePullRequest.to_arc(),
            GitPushSetUpstream.to_arc(),
            GitPushForce.to_arc(),
            GitBranchDeleteUnmerged.to_arc(),
        ],
    )
}
