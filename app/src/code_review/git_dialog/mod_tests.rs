//! Tests for [`user_facing_git_error`].
//!
//! Warp has no `git_dialog` tests at all (`42effe840:app/src/code_review/git_dialog/`
//! contains no test files) and the pin has no `git pull` path of any kind, so
//! there is nothing to port here and these are fork-authored — the same
//! situation `commit_tests.rs` documents.
//!
//! Every `*_STDERR` constant below is **verbatim** output from a real
//! `git pull --ff-only origin <branch>` run against throwaway repositories on
//! git 2.53, reproduced rather than recalled. That is the point of the file:
//! the mapping is a chain of `contains` checks over git's prose, so the only
//! thing that can keep it honest is pinning the prose. A reworded upstream
//! message then shows up as a failing assertion instead of as a silent
//! regression to the generic "Git operation failed." fallback, which is exactly
//! how all six of pull's characteristic failures behaved before this mapping
//! existed.
//!
//! The near-miss in `diverged_history_is_not_reported_as_a_push_rejection` is
//! the reason the pull arms cannot simply be folded into the push arm: git's
//! ff-only refusal says "Not possible to fast-forward", which does **not**
//! contain the substring "non-fast-forward" that the push arm keys on.

use super::user_facing_git_error;

/// Wraps stderr/stdout the way `util::git::run_git_command_with_env` does when
/// it turns a non-zero git exit into an `anyhow::Error`. The local arm of every
/// dialog mode passes exactly this shape to `user_facing_git_error`.
fn local_arm(stderr: &str, stdout: &str) -> String {
    format!("Git command failed: {stderr}, {stdout}")
}

// ── Verbatim git 2.53 output ─────────────────────────────────────────

const DIVERGED_STDERR: &str = concat!(
    "From /tmp/gitfm/origin\n",
    " * branch            main       -> FETCH_HEAD\n",
    "   2f92707..ae330c2  main       -> origin/main\n",
    "hint: Diverging branches can't be fast-forwarded, you need to either:\n",
    "hint: \n",
    "hint: \tgit merge --no-ff\n",
    "hint: \n",
    "hint: or:\n",
    "hint: \n",
    "hint: \tgit rebase\n",
    "hint: \n",
    "hint: Disable this message with \"git config set advice.diverging false\"\n",
    "fatal: Not possible to fast-forward, aborting.\n",
);

const DIRTY_TRACKED_STDERR: &str = concat!(
    "From /tmp/gitfm/origin\n",
    " * branch            main       -> FETCH_HEAD\n",
    "error: Your local changes to the following files would be overwritten by merge:\n",
    "\tf.txt\n",
    "Please commit your changes or stash them before you merge.\n",
    "Aborting\n",
);

const UNTRACKED_COLLISION_STDERR: &str = concat!(
    "From /tmp/gitfm/origin\n",
    " * branch            main       -> FETCH_HEAD\n",
    "   ae330c2..d112f74  main       -> origin/main\n",
    "error: The following untracked working tree files would be overwritten by merge:\n",
    "\tnewfile.txt\n",
    "Please move or remove them before you merge.\n",
    "Aborting\n",
);

const UNMERGED_FILES_STDERR: &str = concat!(
    "error: Pulling is not possible because you have unmerged files.\n",
    "hint: Fix them up in the work tree, and then use 'git add/rm <file>'\n",
    "hint: as appropriate to mark resolution and make a commit.\n",
    "fatal: Exiting because of an unresolved conflict.\n",
);

const MID_MERGE_STDERR: &str = concat!(
    "error: You have not concluded your merge (MERGE_HEAD exists).\n",
    "hint: Please, commit your changes before merging.\n",
    "fatal: Exiting because of unfinished merge.\n",
);

const MISSING_REMOTE_REF_STDERR: &str = "fatal: couldn't find remote ref no-such-branch\n";

const NO_ORIGIN_STDERR: &str = concat!(
    "fatal: 'origin' does not appear to be a git repository\n",
    "fatal: Could not read from remote repository.\n",
    "\n",
    "Please make sure you have the correct access rights\n",
    "and the repository exists.\n",
);

const NO_UPSTREAM_STDERR: &str = concat!(
    "There is no tracking information for the current branch.\n",
    "Please specify which branch you want to merge with.\n",
    "See git-pull(1) for details.\n",
    "\n",
    "    git pull <remote> <branch>\n",
);

// ── Pull: history relationship ───────────────────────────────────────

/// The regression this whole arm exists for. `--ff-only` refusing to merge is
/// the most likely pull outcome after "already up to date", and before the
/// dedicated arm it fell all the way through to "Git operation failed."
///
/// It is also a genuine near-miss: the push arm above it keys on
/// "non-fast-forward", and git's refusal reads "Not possible to fast-forward" —
/// visually similar, not a substring match. Asserting the *negative* here is
/// what stops someone from "simplifying" the two arms back into one.
#[test]
fn diverged_history_is_not_reported_as_a_push_rejection() {
    let mapped = user_facing_git_error(&local_arm(DIVERGED_STDERR, ""));

    assert_eq!(
        mapped,
        "Branch has diverged from the remote \u{2014} merge or rebase manually."
    );
    assert_ne!(
        mapped,
        "Remote has new changes \u{2014} pull before pushing."
    );
    assert_ne!(mapped, "Git operation failed.");
}

/// The push rejection must keep its own copy — the pull arms sit directly below
/// it and must not capture it.
#[test]
fn push_rejection_still_asks_the_user_to_pull_first() {
    let stderr = concat!(
        "To /tmp/gitfm/origin\n",
        " ! [rejected]        main -> main (fetch first)\n",
        "error: failed to push some refs to '/tmp/gitfm/origin'\n",
        "hint: Updates were rejected because the remote contains work that you do not\n",
        "hint: have locally.\n",
    );

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Remote has new changes \u{2014} pull before pushing."
    );
}

// ── Pull: working-tree state ─────────────────────────────────────────

#[test]
fn dirty_tracked_files_ask_for_commit_or_stash() {
    // stdout carries git's "Updating <a>..<b>" line even though the pull
    // aborted, so the mapping must not depend on stdout being empty.
    assert_eq!(
        user_facing_git_error(&local_arm(
            DIRTY_TRACKED_STDERR,
            "Updating 2f92707..ae330c2\n"
        )),
        "Uncommitted changes would be overwritten. Commit or stash them first."
    );
}

/// Untracked collisions get different copy from tracked ones because the
/// remedy differs — stashing does not move an untracked file out of the way.
#[test]
fn untracked_collisions_ask_to_move_or_remove() {
    assert_eq!(
        user_facing_git_error(&local_arm(
            UNTRACKED_COLLISION_STDERR,
            "Updating 2f92707..d112f74\n"
        )),
        "Untracked files would be overwritten. Move or remove them first."
    );
}

/// Stage 1 ships no conflict-resolution UX, so an already-conflicted tree has
/// to be named explicitly rather than reported as a generic failure.
#[test]
fn unmerged_files_report_the_unresolved_conflict() {
    assert_eq!(
        user_facing_git_error(&local_arm(UNMERGED_FILES_STDERR, "")),
        "Unresolved merge conflicts in the working tree. Resolve them first."
    );
}

#[test]
fn mid_merge_reports_the_in_progress_merge() {
    assert_eq!(
        user_facing_git_error(&local_arm(MID_MERGE_STDERR, "")),
        "A merge is already in progress. Finish or abort it first."
    );
}

/// `git commit` phrases the same tree state as "Committing is not possible",
/// which the shared "unmerged files" key also catches. Previously generic;
/// this is an improvement to the commit mode, not a regression.
#[test]
fn commit_with_unmerged_files_reports_the_unresolved_conflict() {
    let stderr = "error: Committing is not possible because you have unmerged files.\n";

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Unresolved merge conflicts in the working tree. Resolve them first."
    );
}

// ── Remote resolution ────────────────────────────────────────────────

#[test]
fn missing_remote_ref_reports_the_branch_rather_than_the_remote() {
    assert_eq!(
        user_facing_git_error(&local_arm(MISSING_REMOTE_REF_STDERR, "")),
        "Branch not found on the remote."
    );
}

/// The "no remote" arm now sits *below* the pull arms, so this pins that none
/// of them captures it on the way past.
#[test]
fn missing_origin_still_reports_no_remote() {
    assert_eq!(
        user_facing_git_error(&local_arm(NO_ORIGIN_STDERR, "")),
        "No remote configured for this branch."
    );
}

#[test]
fn missing_upstream_reports_no_remote() {
    assert_eq!(
        user_facing_git_error(&local_arm(NO_UPSTREAM_STDERR, "")),
        "No remote configured for this branch."
    );
}

// ── Credentials ──────────────────────────────────────────────────────

/// Ties `util::git::git_child_env`'s `GIT_TERMINAL_PROMPT=0` to this mapping:
/// suppressing the prompt is only an improvement if the resulting error is
/// legible. Without the mapping the fix would trade a hang for
/// "Git operation failed."
#[test]
fn suppressed_credential_prompt_reports_authentication() {
    let stderr =
        "fatal: could not read Username for 'https://github.com': terminal prompts disabled\n";

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Authentication failed. Check your Git credentials."
    );
}

#[test]
fn ssh_key_rejection_reports_authentication() {
    let stderr = concat!(
        "git@github.com: Permission denied (publickey).\n",
        "fatal: Could not read from remote repository.\n",
    );

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Authentication failed. Check your Git credentials."
    );
}

// ── Pre-existing arms (previously untested) ──────────────────────────

#[test]
fn empty_commit_reports_nothing_to_commit() {
    assert_eq!(
        user_facing_git_error(&local_arm("", "nothing to commit, working tree clean\n")),
        "No changes to commit."
    );
}

#[test]
fn unset_identity_names_the_two_config_keys() {
    let stderr = concat!(
        "Author identity unknown\n",
        "\n",
        "*** Please tell me who you are.\n",
    );

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Git identity not configured. Set user.name and user.email."
    );
}

#[test]
fn unreachable_host_reports_a_network_error() {
    let stderr = "fatal: unable to access 'https://example.invalid/r.git/': \
                  Could not resolve host: example.invalid\n";

    assert_eq!(
        user_facing_git_error(&local_arm(stderr, "")),
        "Network error. Check your connection."
    );
}

#[test]
fn missing_repository_reports_repository_not_found() {
    assert_eq!(
        user_facing_git_error(&local_arm("remote: Repository not found.\n", "")),
        "Remote repository not found."
    );
}

#[test]
fn missing_gh_binary_points_at_the_install_page() {
    assert_eq!(
        user_facing_git_error("Failed to execute gh command: No such file or directory"),
        "GitHub CLI (gh) not installed. See https://cli.github.com/."
    );
}

// ── Shape invariants ─────────────────────────────────────────────────

/// The remote arm passes `GitOpError::message` — the daemon's `{e:#}` of the
/// same `anyhow` error — so a bare stderr and a wrapped one must classify
/// identically. Otherwise SSH sessions would silently get different copy from
/// local ones for the same failure.
#[test]
fn remote_arm_classifies_identically_to_the_local_arm() {
    for stderr in [
        DIVERGED_STDERR,
        DIRTY_TRACKED_STDERR,
        UNTRACKED_COLLISION_STDERR,
        UNMERGED_FILES_STDERR,
        MID_MERGE_STDERR,
        MISSING_REMOTE_REF_STDERR,
        NO_ORIGIN_STDERR,
        NO_UPSTREAM_STDERR,
    ] {
        assert_eq!(
            user_facing_git_error(stderr),
            user_facing_git_error(&local_arm(stderr, "")),
            "bare and wrapped forms disagree for: {stderr}"
        );
    }
}

/// Matching is case-insensitive by way of the leading `to_lowercase`, which the
/// `MERGE_HEAD exists` key depends on — it is written lowercase in the source
/// but only ever appears uppercase in git's output.
#[test]
fn matching_is_case_insensitive() {
    assert_eq!(
        user_facing_git_error("FATAL: NOT POSSIBLE TO FAST-FORWARD, ABORTING."),
        "Branch has diverged from the remote \u{2014} merge or rebase manually."
    );
}

#[test]
fn unrecognized_errors_fall_back_to_the_generic_message() {
    assert_eq!(
        user_facing_git_error(&local_arm("fatal: some brand new git failure\n", "")),
        "Git operation failed."
    );
}
