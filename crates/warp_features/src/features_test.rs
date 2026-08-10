use super::*;

#[test]
#[ignore = "CORE-3768 - need to clean up PREVIEW_FLAGS, but this is a temporary fix for the cluttered changelog"]
fn test_all_preview_flags_have_a_description() {
    for flag in PREVIEW_FLAGS {
        assert!(
            flag.flag_description()
                .is_some_and(|description| !description.is_empty()),
            "Missing description for preview-enabled flag {flag:?}"
        );
    }
}

#[test]
fn local_child_harnesses_are_local_only_by_default() {
    assert!(LOCAL_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DEBUG_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
    assert!(!DOGFOOD_FLAGS.contains(&FeatureFlag::LocalClaudeCodexChildHarnesses));
}

/// The force-disable list is a very blunt instrument -- it beats the channel
/// lists, the compile-time cargo features and the user's own preference -- so
/// its membership is pinned exactly. A flag swept in here silently becomes
/// dead code no compiler warning will find (that is what happened to computer
/// use between `5013248be` and this test).
#[test]
fn force_disabled_flags_membership_is_exact() {
    assert_eq!(
        FORCE_DISABLED_FLAGS.to_vec(),
        vec![
            FeatureFlag::ForceLogin,
            FeatureFlag::AvatarInTabBar,
            FeatureFlag::HOARemoteControl,
        ],
        "adding a flag here makes it unreachable by any means; prefer leaving it \
         out of the channel lists (which already means off-by-default)"
    );
}

/// Computer use is local: it drives this machine's own mouse and keyboard via
/// `crates/computer_use`, and never talks to a server. It must not be swept in
/// with the account-bound flags again.
#[test]
fn computer_use_flags_are_not_force_disabled() {
    assert!(!FeatureFlag::AgentModeComputerUse.is_force_disabled());
    assert!(!FeatureFlag::LocalComputerUse.is_force_disabled());
}

#[test]
fn account_bound_flags_are_still_force_disabled() {
    assert!(FeatureFlag::ForceLogin.is_force_disabled());
    assert!(FeatureFlag::AvatarInTabBar.is_force_disabled());
    assert!(FeatureFlag::HOARemoteControl.is_force_disabled());
}

/// Behavioural counterpart to the membership tests: a user preference is the
/// lowest-ceremony way to prove `is_enabled` actually consults *something* for
/// computer use, and still consults nothing for the account-bound flags.
///
/// Both assertions live in one test on purpose: `set_user_preference` writes
/// process-global state, so splitting them would let the two halves race.
#[test]
fn computer_use_flag_is_no_longer_unconditionally_false() {
    // Outside `test-util` builds `is_enabled` asserts the flags were
    // initialized by the app; this test drives the flag state directly.
    mark_initialized();

    FeatureFlag::AgentModeComputerUse.set_user_preference(true);
    assert!(
        FeatureFlag::AgentModeComputerUse.is_enabled(),
        "AgentModeComputerUse must be reachable, otherwise the restored \
         computer-use dispatch path is dead code"
    );

    FeatureFlag::AgentModeComputerUse.set_user_preference(false);
    assert!(!FeatureFlag::AgentModeComputerUse.is_enabled());

    // The force-disabled flags outrank a user preference, which is the whole
    // point of the list.
    FeatureFlag::ForceLogin.set_user_preference(true);
    assert!(!FeatureFlag::ForceLogin.is_enabled());
}
