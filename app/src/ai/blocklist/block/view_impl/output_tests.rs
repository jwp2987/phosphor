use ai::skills::{ParsedSkill, SkillProvider, SkillReference, SkillScope};
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;

use super::{
    attachment_caps::AttachmentCaps, parsed_skill_for_common_locations, read_skill_display_text,
    should_decorate_blind_use_computer_screenshot,
};
use crate::ai::skills::SkillManager;

fn sighted_caps() -> AttachmentCaps {
    AttachmentCaps {
        images: true,
        pdf: true,
        audio: false,
    }
}

fn blind_caps() -> AttachmentCaps {
    AttachmentCaps {
        images: false,
        pdf: false,
        audio: false,
    }
}

// Mirrors the pin's `use_computer_decoration_skips_screenshot_only_rows` in shape (a
// pure predicate over a `use_computer` block's decoration), but not in subject: the
// pin decorates recording status, which this fork has declined (DECLINED.md,
// "computer_use session recording", #350). This decorates screenshot delivery status
// instead -- see `should_decorate_blind_use_computer_screenshot`'s doc comment.

#[test]
fn use_computer_screenshot_decoration_shows_when_model_cannot_see_images() {
    assert!(should_decorate_blind_use_computer_screenshot(
        true,
        Some(blind_caps()),
    ));
}

#[test]
fn use_computer_screenshot_decoration_hidden_when_model_can_see_images() {
    // The model may or may not have actually received *this* screenshot --
    // `ScreenshotDelivery::Superseded`/`Undeliverable` are real per-turn outcomes for
    // an image-capable model -- but the block cannot know which without turn-local
    // state it doesn't have. It must stay quiet rather than claim delivery.
    assert!(!should_decorate_blind_use_computer_screenshot(
        true,
        Some(sighted_caps()),
    ));
}

#[test]
fn use_computer_screenshot_decoration_hidden_without_a_screenshot() {
    // Nothing was captured, so there is nothing to say was or wasn't delivered --
    // matches the pin's own "screenshot-only rows" exemption in spirit, just for the
    // opposite direction (no actions vs. no screenshot).
    assert!(!should_decorate_blind_use_computer_screenshot(
        false,
        Some(blind_caps()),
    ));
}

#[test]
fn use_computer_screenshot_decoration_hidden_when_caps_are_unresolvable() {
    // `attachment_caps_for_block` returns `None` when the model/provider can't be
    // resolved (e.g. deleted mid-conversation). Absence of proof the model is blind
    // is not proof it can see -- so this stays quiet rather than guessing either way.
    assert!(!should_decorate_blind_use_computer_screenshot(true, None));
}

fn make_skill(name: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: String::new(),
        path: LocalOrRemotePath::Local(
            std::path::PathBuf::from("/home/user/.agents/skills")
                .join(name)
                .join("SKILL.md"),
        ),
        content: String::new(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Home,
    }
}

#[test]
fn read_skill_display_text_shows_slash_command_when_skill_found() {
    let skill = make_skill("hello-world");
    let reference = SkillReference::Path(skill.path.clone());
    assert_eq!(
        read_skill_display_text(Some(&skill), &reference),
        "/hello-world"
    );
}

#[test]
fn read_skill_display_text_no_double_slash_when_skill_not_found_with_path_reference() {
    // When the skill is not in the manager the fallback is
    // `skill_reference.display_label()`, which for a path reference is an
    // absolute path starting with '/'. The display text must NOT prepend an
    // extra '/' — doing so would produce '//home/…'.
    let path = std::path::PathBuf::from("/home/devbox/.warp-local/skills/hello-world/SKILL.md");
    let reference = SkillReference::Path(LocalOrRemotePath::Local(path));
    let display = read_skill_display_text(None, &reference);
    assert!(
        !display.starts_with("//"),
        "display text must not start with '//': {display}"
    );
    assert!(
        display.starts_with('/'),
        "display text should start with '/': {display}"
    );
}

#[test]
fn read_skill_display_text_bundled_id_fallback_when_skill_not_found() {
    // The fallback uses the user-facing label (the bare id), not the canonical
    // `@warp-skill:<id>` reference form, so bundled-skill copy reads the same
    // way as path-based skill copy.
    let reference = SkillReference::BundledSkillId("create-pr".to_string());
    let display = read_skill_display_text(None, &reference);
    assert_eq!(display, "create-pr");
}

fn remote_location(host_id: &HostId, path: &str) -> LocalOrRemotePath {
    LocalOrRemotePath::Remote(RemotePath::new(
        host_id.clone(),
        StandardizedPath::try_new(path).unwrap(),
    ))
}

#[test]
fn parsed_skill_for_common_locations_resolves_cached_remote_skill() {
    let host_id = HostId::new("remote-host".to_string());
    let skill = ParsedSkill {
        name: "deploy".to_string(),
        description: "Deploy skill".to_string(),
        path: remote_location(&host_id, "/repo/.agents/skills/deploy/SKILL.md"),
        content: "# Deploy".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };
    let locations = vec![
        remote_location(&host_id, "/repo/.agents/skills/deploy/README.md"),
        remote_location(&host_id, "/repo/.agents/skills/deploy/scripts/run.sh"),
    ];

    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(repo_metadata::DirectoryWatcher::new);
        app.add_singleton_model(|_| repo_metadata::repositories::DetectedRepositories::default());
        app.add_singleton_model(repo_metadata::RepoMetadataModel::new);
        app.add_singleton_model(watcher::HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(crate::warp_managed_paths_watcher::WarpManagedPathsWatcher::new_for_testing);
        let manager = app.add_singleton_model(SkillManager::new);
        manager.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(skill.clone());
        });

        let resolved = manager.read(&app, |_, ctx| {
            parsed_skill_for_common_locations(locations, ctx).map(|skill| skill.path.clone())
        });
        assert_eq!(resolved, Some(skill.path));
    });
}

#[test]
fn parsed_skill_for_common_locations_does_not_mix_remote_hosts() {
    let first_host = HostId::new("first-host".to_string());
    let second_host = HostId::new("second-host".to_string());
    let skill = ParsedSkill {
        name: "deploy".to_string(),
        description: "Deploy skill".to_string(),
        path: remote_location(&first_host, "/repo/.agents/skills/deploy/SKILL.md"),
        content: "# Deploy".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };
    let locations = vec![
        remote_location(&first_host, "/repo/.agents/skills/deploy/README.md"),
        remote_location(&second_host, "/repo/.agents/skills/deploy/README.md"),
    ];

    warpui::App::test((), |mut app| async move {
        app.add_singleton_model(repo_metadata::DirectoryWatcher::new);
        app.add_singleton_model(|_| repo_metadata::repositories::DetectedRepositories::default());
        app.add_singleton_model(repo_metadata::RepoMetadataModel::new);
        app.add_singleton_model(watcher::HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(crate::warp_managed_paths_watcher::WarpManagedPathsWatcher::new_for_testing);
        let manager = app.add_singleton_model(SkillManager::new);
        manager.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(skill);
        });

        assert!(manager.read(&app, |_, ctx| {
            parsed_skill_for_common_locations(locations, ctx).is_none()
        }));
    });
}

mod is_orphaned_by_finished_output_tests {
    //! Ported from the **re-pin candidate** `4111d08f9`
    //! (`4111d08f9:app/src/ai/blocklist/inline_action/run_agents_card_view_tests.rs`,
    //! `mod is_orphaned_by_finished_output_tests`). `4111d08f9` is this round's
    //! candidate, *not* the pin -- `ORACLE.md` still pins `42effe840`, where
    //! neither this predicate nor these tests exist (zero `is_orphaned` and
    //! zero `statusless` hits in `42effe840:.../run_agents_card_view.rs` and
    //! its test file).
    //!
    //! Retargeted from the candidate's RunAgents orchestration card -- which
    //! this fork declines (`DECLINED.md` #290 for `run_agents_card_view`, #325
    //! for the `RunAgents` action) -- onto the equivalent decision inside
    //! `action_icon`, which the candidate's own predicate doc comment names as
    //! its mirror. The behaviour was live here with zero coverage: nothing in
    //! this file mentioned `is_orphaned` or `statusless` before this module.
    //! Retargeting, rather than porting the candidate's predicate into a file
    //! nothing calls, is deliberate: an uncalled helper plus tests is exactly
    //! what `script/check_stub_coverage` exists to catch.
    //!
    //! **Scope limit, read before trusting these.** Every test here calls
    //! `is_orphaned_by_finished_output` directly. None of them execute
    //! `action_icon`, so none of them constrain how `action_icon` *uses* the
    //! predicate: moving the early return, inverting the argument at the call
    //! site, or deleting the call outright leaves this whole module green. What
    //! is guarded is the predicate's value over its input space; the call-site
    //! wiring is unguarded, and testing it would need an `AppContext`, a
    //! `ModelHandle<BlocklistAIActionModel>`, an `AIBlockModel` impl and
    //! equality over `warpui::elements::Icon` -- deliberately not built.

    use std::cell::Cell;

    use super::super::is_orphaned_by_finished_output;
    use crate::ai::agent::{AIAgentOutput, CancellationReason, RenderableAIError, Shared};
    use crate::ai::blocklist::action_model::AIActionStatus;
    use crate::ai::blocklist::block::model::AIBlockOutputStatus;

    fn partial_output() -> Shared<AIAgentOutput> {
        Shared::new(AIAgentOutput::default())
    }

    fn cancelled_block() -> AIBlockOutputStatus {
        AIBlockOutputStatus::Cancelled {
            partial_output: Some(partial_output()),
            reason: CancellationReason::ManuallyCancelled,
        }
    }

    fn failed_block() -> AIBlockOutputStatus {
        AIBlockOutputStatus::Failed {
            partial_output: Some(partial_output()),
            // This fork's `RenderableAIError` has no `other()` constructor and
            // no `is_user_error` field; `Other` with both resume flags false is
            // the same value the candidate's `RenderableAIError::other("boom",
            // false)` builds.
            error: RenderableAIError::Other {
                error_message: "boom".to_string(),
                will_attempt_resume: false,
                waiting_for_network: false,
            },
        }
    }

    /// The user stopped the response mid-tool-call, so the call never reached
    /// the action queue and never will. `action_icon` reads this as its cue to
    /// paint the cancelled icon instead of leaving the row running forever --
    /// but that reading is the call site's, not this test's.
    #[test]
    fn statusless_action_on_cancelled_block_is_orphaned() {
        assert!(is_orphaned_by_finished_output(None, cancelled_block));
    }

    /// The other terminal block status: the stream died with an error before
    /// the call was queued. Same verdict.
    #[test]
    fn statusless_action_on_failed_block_is_orphaned() {
        assert!(is_orphaned_by_finished_output(None, failed_block));
    }

    /// While the block is still streaming, a statusless action is one the model
    /// is writing out right now, so it must not be declared dead.
    #[test]
    fn statusless_action_on_unfinished_block_is_not_orphaned() {
        for block_status in [
            AIBlockOutputStatus::Pending,
            AIBlockOutputStatus::PartiallyReceived {
                output: partial_output(),
            },
        ] {
            let label = format!("{block_status:?}");
            assert!(
                !is_orphaned_by_finished_output(None, move || block_status),
                "{label} should not orphan the row"
            );
        }
    }

    /// Deliberate divergence from the re-pin candidate, documented on the
    /// predicate itself. The candidate's RunAgents-card version orphans only on
    /// `Cancelled`/`Failed`, leaving a statusless action on a `Complete` block
    /// un-orphaned. What this fork ships in `action_icon` -- and what the
    /// candidate's own doc comment names as the mirror -- keys off
    /// `is_streaming()`, so `Complete` orphans too: a block whose output
    /// finished cleanly without ever queuing the call is never going to queue
    /// it.
    #[test]
    fn statusless_action_on_successful_block_is_orphaned() {
        assert!(is_orphaned_by_finished_output(None, || {
            AIBlockOutputStatus::Complete {
                output: partial_output(),
            }
        }));
    }

    /// An action that reached the queue gets a real result even when the
    /// conversation is cancelled, so a live status must not be overridden by
    /// the block's. (That the *icon* then follows the action's status is
    /// `action_icon`'s ordering, which nothing here executes.)
    #[test]
    fn action_with_status_on_cancelled_block_is_not_orphaned() {
        for action_status in [
            AIActionStatus::Preprocessing,
            AIActionStatus::Queued,
            AIActionStatus::Blocked,
            AIActionStatus::RunningAsync,
        ] {
            assert!(
                !is_orphaned_by_finished_output(Some(&action_status), cancelled_block),
                "{action_status:?} should not orphan the row"
            );
        }
    }

    /// The laziness of `block_status` is a performance contract, not a
    /// courtesy: `AIBlockModel::status` walks every task and every exchange in
    /// the conversation, and `action_icon` reaches this predicate once per
    /// action row per render.
    ///
    /// This exists because the obvious "restore parity" edit -- widening the
    /// parameter to `&AIBlockOutputStatus`, which is literally the re-pin
    /// candidate's signature (`4111d08f9:.../run_agents_card_view.rs:1670`,
    /// called eagerly at `:1264`) -- passes every other test in this module
    /// while adding that scan to every row of every frame. Counting the
    /// closure's invocations is the only thing that catches it.
    #[test]
    fn block_status_is_not_computed_when_the_action_has_a_status() {
        let calls = Cell::new(0usize);
        let action_status = AIActionStatus::RunningAsync;
        assert!(!is_orphaned_by_finished_output(
            Some(&action_status),
            || {
                calls.set(calls.get() + 1);
                cancelled_block()
            }
        ));
        assert_eq!(
            calls.get(),
            0,
            "an action with a status must short-circuit before the block status \
             is computed; taking it by value or reference instead of lazily \
             reintroduces a per-row, per-frame conversation scan"
        );

        // The counterpart, so the assertion above cannot pass vacuously by the
        // closure having become unreachable in every case.
        let calls = Cell::new(0usize);
        assert!(is_orphaned_by_finished_output(None, || {
            calls.set(calls.get() + 1);
            cancelled_block()
        }));
        assert_eq!(
            calls.get(),
            1,
            "a statusless action must consult the block status exactly once"
        );
    }
}
