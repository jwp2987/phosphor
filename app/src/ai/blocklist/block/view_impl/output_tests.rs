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
