use ai::skills::{ParsedSkill, SkillProvider, SkillReference, SkillScope};
use warpui::App;

use super::*;

// The pin's `bundled_tests.rs` (`02b53fcd8`) has exactly two tests, neither of which
// survives the port — see `bundled.rs`'s module doc comment for the remote-arm decision:
//
// - `local_and_remote_catalogs_are_isolated` — exercises the pin's `BundledSkills`
//   local/remote-host multiplexing. Dropped along with that wrapper (no remote-skill
//   daemon here).
// - `unavailable_bundled_context_path_renders_as_empty_string` — exercises
//   `display_optional_path`, a helper that only exists because the pin's
//   `build_bundled_skill_context` has `Option<PathBuf>`-typed GUI/TUI config-dir
//   variables this fork doesn't build yet (no `gui_config_local_dir`/
//   `tui_config_local_dir` — separate from this issue's scope). Not ported for the
//   same reason `build_bundled_skill_context` here stays at its current (fork-original,
//   already-tested) variable set.
//
// In their place, this adds direct coverage of the `BundledSkill` catalog surface
// extracted out of `skill_manager.rs` by this port — previously only exercised
// indirectly through `SkillManager`.

fn test_skill(id: &str) -> ParsedSkill {
    ParsedSkill {
        name: id.to_string(),
        description: format!("{id} description"),
        path: format!("/bundled/skills/{id}/SKILL.md").into(),
        content: format!("# {id}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

#[test]
fn active_descriptors_includes_only_enabled_definitions() {
    App::test((), |app| async move {
        let descriptors = app.read(|ctx| {
            let mut bundled = BundledSkill::default();
            bundled.insert_for_testing(
                "always-on",
                test_skill("always-on"),
                BundledSkillActivation::Always,
            );
            bundled.insert_for_testing(
                "requires-missing-file",
                test_skill("requires-missing-file"),
                BundledSkillActivation::RequiresFile(
                    "/definitely/does/not/exist/on/this/host".into(),
                ),
            );

            bundled.active_descriptors(ctx)
        });

        let names: Vec<&str> = descriptors.iter().map(|d| d.name.as_str()).collect();
        assert!(names.contains(&"always-on"));
        assert!(!names.contains(&"requires-missing-file"));
    });
}

#[test]
fn reference_for_path_resolves_to_bundled_skill_id() {
    let mut bundled = BundledSkill::default();
    let skill = test_skill("modify-settings");
    let path = skill.path.clone();
    bundled.insert_for_testing("modify-settings", skill, BundledSkillActivation::Always);

    assert_eq!(
        bundled.reference_for_path(&path),
        Some(SkillReference::BundledSkillId(
            "modify-settings".to_string()
        ))
    );
    assert_eq!(
        bundled.reference_for_path(std::path::Path::new("/not/a/bundled/skill/SKILL.md")),
        None
    );
}

#[test]
fn skill_and_active_skill_by_id() {
    App::test((), |app| async move {
        let mut bundled = BundledSkill::default();
        bundled.insert_for_testing(
            "gated",
            test_skill("gated"),
            BundledSkillActivation::RequiresFile("/definitely/does/not/exist".into()),
        );

        // `skill` ignores activation; `active_skill` honors it.
        assert!(bundled.skill("gated").is_some());
        let gated_inactive = app.read(|ctx| bundled.active_skill("gated", ctx).is_some());
        assert!(
            !gated_inactive,
            "gated skill behind a missing file should not be active"
        );

        bundled.insert_for_testing("gated", test_skill("gated"), BundledSkillActivation::Always);
        let gated_active = app.read(|ctx| bundled.active_skill("gated", ctx).is_some());
        assert!(gated_active, "gated skill should become active once Always");

        assert!(bundled.skill("missing").is_none());
    });
}

#[test]
fn iter_yields_every_definition_regardless_of_activation() {
    let mut bundled = BundledSkill::default();
    bundled.insert_for_testing(
        "one",
        test_skill("one"),
        BundledSkillActivation::RequiresFile("/definitely/does/not/exist".into()),
    );
    bundled.insert_for_testing("two", test_skill("two"), BundledSkillActivation::Always);

    let mut ids: Vec<&str> = bundled.iter().map(|(id, _)| id).collect();
    ids.sort_unstable();
    assert_eq!(ids, vec!["one", "two"]);
}
