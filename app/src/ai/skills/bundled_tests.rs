use ai::skills::{ParsedSkill, SkillProvider, SkillReference, SkillScope};
use warp_core::execution_mode::ExecutionMode;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warpui::App;

use super::*;

// The pin's `bundled_tests.rs` (`02b53fcd8`) has exactly two tests:
//
// - `local_and_remote_catalogs_are_isolated` — exercises `BundledSkills`'
//   local/remote-host multiplexing. Ported below now that the SSH arm is built
//   (see `bundled.rs`'s module doc comment).
// - `unavailable_bundled_context_path_renders_as_empty_string` — exercises
//   `display_optional_path`. Ported below. An earlier note here said the helper
//   could not be ported because it only served the pin's `Option<PathBuf>`-typed
//   GUI/TUI config-dir variables (`gui_config_local_dir` / `tui_config_local_dir`),
//   which this fork does not build. That is no longer accurate: this fork's
//   `build_bundled_skill_context` renders `{{warpctrl_wrapper_path}}` from
//   `warp_core::paths::bundled_resources_dir()`, which is also `Option<PathBuf>`,
//   and now goes through `display_optional_path` — so the helper has a real
//   production caller here and the pin's test covers it.
//
// The rest of this file adds direct coverage of the `BundledSkill` catalog surface
// extracted out of `skill_manager.rs` by an earlier port — previously only exercised
// indirectly through `SkillManager`.

fn test_skill(id: &str) -> ParsedSkill {
    ParsedSkill {
        name: id.to_string(),
        description: format!("{id} description"),
        path: LocalOrRemotePath::Local(format!("/bundled/skills/{id}/SKILL.md").into()),
        content: format!("# {id}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

#[test]
fn unavailable_bundled_context_path_renders_as_empty_string() {
    assert_eq!(display_optional_path(None), "");
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
        bundled.reference_for_path(&LocalOrRemotePath::Local(
            "/not/a/bundled/skill/SKILL.md".into()
        )),
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

// ============================================================================
// The shipped `tui-settings` bundled skill
//
// Replaces the pin's `tui_migration_skill_has_tui_only_activation`, which
// asserted `TuiOnly` for `tui-migrate-setup`. That skill is not portable here
// and `tui-settings` answers the same question for this fork's one-config
// architecture -- see `activation_for_bundled_skill` in `bundled.rs` for why its
// activation is `Always` rather than `TuiOnly`. These tests pin that decision
// and the skill's rendering.
// ============================================================================

/// The directory the app ships its bundled skills from.
fn bundled_skills_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the app crate directory has a parent")
        .join("resources")
        .join("bundled")
        .join("skills")
}

#[test]
fn tui_settings_bundled_skill_is_active_in_both_frontends() {
    for execution_mode in [ExecutionMode::App, ExecutionMode::Tui] {
        App::test((), |app| async move {
            app.add_singleton_model(|ctx| AppExecutionMode::new(execution_mode, false, ctx));

            let activation = activation_for_bundled_skill("tui-settings", Path::new("/resources"));
            assert!(
                matches!(activation, BundledSkillActivation::Always),
                "tui-settings explains a settings file both frontends share, so it must not be gated on the frontend"
            );
            assert!(app.read(|ctx| activation.is_enabled(ctx)));
        });
    }
}

/// `handlebars::render_template` leaves an unknown `{{name}}` verbatim rather
/// than erroring, so a variable missing from `build_bundled_skill_context`
/// ships as visibly broken skill text instead of failing the build.
#[test]
fn tui_settings_bundled_skill_renders_every_template_variable() {
    let skills = futures::executor::block_on(read_bundled_skills(&bundled_skills_dir()));
    let skill = skills
        .get("tui-settings")
        .expect("the tui-settings skill is bundled with the app");

    assert!(
        skill.content.contains(
            &crate::settings::user_preferences_toml_file_path()
                .display()
                .to_string()
        ),
        "the shared settings file path should be rendered into the skill"
    );
    assert!(
        skill.content.contains(
            &crate::keyboard::keybinding_file_path()
                .display()
                .to_string()
        ),
        "the keybindings file path should be rendered into the skill"
    );
    assert!(
        !skill.content.contains("{{"),
        "tui-settings still contains an unrendered template variable: {}",
        skill.content
    );
}

// ============================================================================
// The shipped `agent-add-mcp` bundled skill (directory `add-mcp-server`)
//
// Issue #631: the skill told the agent that the global MCP config is
// `~/.warp/.mcp.json`. Nothing reads that path on the shipped OSS build --
// `home_config_file_path(MCPProvider::Zap)` resolves through
// `warp_core::paths::warp_home_mcp_config_file_path`, whose directory is
// channel-aware and is `~/.phosphor` on OSS -- so a server the agent added on
// the user's behalf silently never appeared.
//
// Naming `~/.phosphor/.mcp.json` in prose and letting the agent search for the
// directory on other channels was the first fix and was still wrong: on a fresh
// install `~/.phosphor` need not exist, and the home directory of a machine that
// also runs Warp has a real `.warp` with its own `skills/` and `.mcp.json`, so
// the search selects Warp's config. The path is rendered now.
// ============================================================================

#[test]
fn add_mcp_skill_documents_the_global_config_path_phosphor_actually_reads() {
    let skills = futures::executor::block_on(read_bundled_skills(&bundled_skills_dir()));
    let skill = skills
        .get("add-mcp-server")
        .expect("the add-mcp-server skill is bundled with the app");

    let config_path = warp_core::paths::warp_home_mcp_config_file_path()
        .expect("the global MCP config path resolves in the test environment");
    assert!(
        skill.content.contains(&config_path.display().to_string()),
        "the skill must name the file `home_config_file_path(MCPProvider::Zap)` resolves to ({}): {}",
        config_path.display(),
        skill.content
    );
    assert!(
        !skill.content.contains("{{mcp_config_file_path}}"),
        "the skill still contains an unrendered {{{{mcp_config_file_path}}}}: {}",
        skill.content
    );
    // Project-scoped configs are NOT channel-aware (`MCPProvider::project_config_path`), so this
    // one stays literal.
    assert!(
        skill.content.contains("{repo_root}/.warp/.mcp.json"),
        "the project-scoped path is literal `.warp/.mcp.json` on every channel: {}",
        skill.content
    );
    // The MCP settings page renders `Detected from {provider.display_name()}`, which is "Phosphor".
    assert!(
        !skill.content.contains("Detected from Warp"),
        "the settings section is labelled \"Detected from Phosphor\": {}",
        skill.content
    );
}

// ============================================================================
// Tab config locations (issue #631, same class as the MCP path above)
//
// `tab_configs_dir()` is `warp_core::paths::data_dir()/tab_configs`, and
// `data_dir()` is a channel-specific dotfile directory in `$HOME` only on
// macOS -- on Linux it is the XDG data directory. The three tab-config skills
// used to send the agent to `~/.warp/tab_configs/` and to discover it with
// `ls -d ~/.warp*/`, which is wrong on every platform for this fork and finds
// nothing at all on Linux. They now render the resolved directory instead.
// ============================================================================

#[test]
fn tab_config_skills_name_the_directory_the_app_actually_reads() {
    let skills = futures::executor::block_on(read_bundled_skills(&bundled_skills_dir()));
    let tab_configs_dir = crate::user_config::tab_configs_dir().display().to_string();

    for id in ["tab-configs", "create-tab-config", "update-tab-config"] {
        let skill = skills
            .get(id)
            .unwrap_or_else(|| panic!("the {id} skill is bundled with the app"));
        assert!(
            skill.content.contains(&tab_configs_dir),
            "`{id}` must name the tab config directory this build reads ({tab_configs_dir}): {}",
            skill.content
        );
        // `handlebars::render_template` leaves an unknown variable verbatim, so a typo'd or
        // unregistered name ships as visibly broken skill text rather than failing the build.
        for variable in [
            "{{tab_configs_dir}}",
            "{{default_tab_configs_dir}}",
            "{{worktrees_dir}}",
        ] {
            assert!(
                !skill.content.contains(variable),
                "`{id}` still contains an unrendered {variable}: {}",
                skill.content
            );
        }
    }

    // `default_tab_configs_dir()` holds `worktree.toml`, the user-editable template every
    // generated worktree config is materialized from. A skill that names only `tab_configs_dir()`
    // sends "change the default worktree template" to the wrong directory.
    let default_tab_configs_dir = crate::user_config::default_tab_configs_dir()
        .display()
        .to_string();
    for id in ["tab-configs", "update-tab-config"] {
        assert!(
            skills[id].content.contains(&default_tab_configs_dir),
            "`{id}` must name the editable template directory ({default_tab_configs_dir}): {}",
            skills[id].content
        );
    }

    // The worktree example writes a shell command the user's config will run, so it must not
    // point outside the directory `generated_worktree_repo_dir` uses either.
    let worktrees_dir = warp_core::paths::data_dir()
        .join("worktrees")
        .display()
        .to_string();
    let tab_configs = &skills["tab-configs"].content;
    assert!(
        tab_configs.contains(&worktrees_dir),
        "the worktree example must use the generated-worktree base directory ({worktrees_dir}): {tab_configs}"
    );
}

/// Nothing this app ships to the user's disk or renders into a prompt may name a literal
/// `~/.warp/<something>`: every home-anchored directory this fork reads is resolved through
/// `warp_core::paths` and is `.phosphor` on the shipped OSS channel, so a hardcoded `~/.warp/`
/// path is read by nothing (#631).
///
/// The scope is both bundled skills and `app/resources/tab_configs`, because the second is
/// the same defect through a different door: `default_worktree.toml` and
/// `new_tab_config_template.toml` are copied verbatim onto the user's disk by
/// `ensure_default_worktree_config` and the new-config flow, header comments included, and
/// invite the user to edit them. A skills-only walk cannot see them.
///
/// Two things this deliberately does NOT forbid, because both are correct:
///
/// - `{repo_root}/.warp/.mcp.json` in `add-mcp-server`. `MCPProvider::project_config_path()` is
///   a literal `.warp/.mcp.json` on every channel, so the project-scoped path really is that.
///   It is not home-anchored, so the `~/` and `$HOME/` prefixes below already exclude it.
/// - A bare `.warp` mentioned in prose as the thing NOT to go looking for (`add-mcp-server`
///   warns that a machine with Warp installed has one). Naming the wrong directory in order to
///   steer away from it is the opposite of hardcoding it, and the trailing slash excludes it.
#[test]
fn shipped_resources_do_not_hardcode_a_home_warp_directory() {
    let mut sources: Vec<(String, String)> =
        futures::executor::block_on(read_bundled_skills(&bundled_skills_dir()))
            .into_iter()
            .map(|(id, skill)| (format!("bundled skill `{id}`"), skill.content))
            .collect();
    assert!(!sources.is_empty(), "no bundled skills were read");

    let tab_config_resources = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("tab_configs");
    let mut resource_count = 0;
    for entry in std::fs::read_dir(&tab_config_resources).expect("app resources/tab_configs exists")
    {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        sources.push((
            path.display().to_string(),
            std::fs::read_to_string(&path).unwrap(),
        ));
        resource_count += 1;
    }
    assert!(
        resource_count >= 2,
        "expected the shipped tab config templates to be walked, saw {resource_count}"
    );

    for (label, text) in &sources {
        for spelling in ["~/.warp/", "$HOME/.warp/"] {
            assert!(
                !text.contains(spelling),
                "{label} hardcodes `{spelling}`, which nothing reads on the shipped build: {text}"
            );
        }
    }
}

/// A bundled skill's `description:` is user-visible -- it is what the skills picker and the
/// system prompt's `<available_skills>` block show. `script/check_brand_strings` does not read
/// Markdown, so this is where issue #631's rebranding is held in place.
#[test]
fn bundled_skill_descriptions_do_not_name_the_product_warp() {
    let skills = futures::executor::block_on(read_bundled_skills(&bundled_skills_dir()));
    assert!(!skills.is_empty(), "no bundled skills were read");
    for (id, skill) in &skills {
        for brand in ["Warp", "Zap", "Oz"] {
            assert!(
                !skill.description.contains(brand),
                "bundled skill `{id}` describes the product as {brand}: {}",
                skill.description
            );
        }
    }
}

fn bundled_skill_with_content(content: &str) -> BundledSkill {
    let mut bundled_skill = BundledSkill::default();
    bundled_skill.insert_for_testing(
        "test-skill",
        ParsedSkill {
            name: "test-skill".to_string(),
            description: "Test skill".to_string(),
            path: LocalOrRemotePath::Local("/bundled/skills/test-skill/SKILL.md".into()),
            content: content.to_string(),
            line_range: None,
            provider: SkillProvider::Zap,
            scope: SkillScope::Bundled,
        },
        BundledSkillActivation::Always,
    );
    bundled_skill
}

fn remote_content<'a>(bundled_skills: &'a BundledSkills, host_id: &HostId) -> Option<&'a str> {
    bundled_skills
        .remote(host_id)?
        .skill("test-skill")
        .map(|skill| skill.content.as_str())
}

#[test]
fn local_and_remote_catalogs_are_isolated() {
    let first_host_id = HostId::new("first-host".to_string());
    let second_host_id = HostId::new("second-host".to_string());
    let mut bundled_skills = BundledSkills::default();
    bundled_skills.set_local(bundled_skill_with_content("local"));
    bundled_skills.insert_remote(first_host_id.clone(), bundled_skill_with_content("first"));
    bundled_skills.insert_remote(second_host_id.clone(), bundled_skill_with_content("second"));

    assert_eq!(
        bundled_skills
            .local_skill("test-skill")
            .map(|skill| skill.content.as_str()),
        Some("local")
    );
    assert_eq!(
        remote_content(&bundled_skills, &first_host_id),
        Some("first")
    );
    assert_eq!(
        remote_content(&bundled_skills, &second_host_id),
        Some("second")
    );

    // A reconnect refresh replaces the host's catalog wholesale.
    bundled_skills.insert_remote(
        first_host_id.clone(),
        bundled_skill_with_content("first-refreshed"),
    );
    assert_eq!(
        remote_content(&bundled_skills, &first_host_id),
        Some("first-refreshed")
    );

    // Disconnecting one host leaves the local and sibling-host catalogs intact.
    bundled_skills.remove_remote(&first_host_id);
    assert_eq!(
        bundled_skills
            .local_skill("test-skill")
            .map(|skill| skill.content.as_str()),
        Some("local")
    );
    assert_eq!(remote_content(&bundled_skills, &first_host_id), None);
    assert_eq!(
        remote_content(&bundled_skills, &second_host_id),
        Some("second")
    );
}
