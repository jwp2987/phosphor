use super::*;
use crate::ai::skills::bundled::activation_for_bundled_skill;
use crate::settings::AISettings;
use crate::warp_managed_paths_watcher::WarpManagedPathsWatcher;
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use ai::skills::{ParsedSkill, SkillProvider, SkillReference, SkillScope};
use repo_metadata::{repositories::DetectedRepositories, DirectoryWatcher, RepoMetadataModel};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tempfile::TempDir;
use warp_core::channel::ChannelState;
use warp_core::features::FeatureFlag;
use warp_util::host_id::HostId;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;
use warpui::App;
use watcher::HomeDirectoryWatcher;

// ============================================================================
// Tests for get_skills_for_working_directory subdirectory scoping
// ============================================================================

#[test]
fn list_skill_inventory_groups_same_name_skills_and_marks_default() {
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let repo = PathBuf::from("/repo");
        let agents_path = repo.join(".agents/skills/deploy/SKILL.md");
        let codex_path = repo.join(".codex/skills/deploy/SKILL.md");

        let agents_skill = ParsedSkill {
            name: "deploy".to_string(),
            description: "Agents copy".to_string(),
            path: LocalOrRemotePath::Local(agents_path.clone()),
            content: "# Deploy".to_string(),
            line_range: None,
            provider: SkillProvider::Agents,
            scope: SkillScope::Project,
        };
        let codex_skill = ParsedSkill {
            name: "deploy".to_string(),
            description: "Codex copy".to_string(),
            path: LocalOrRemotePath::Local(codex_path),
            content: "# Deploy".to_string(),
            line_range: None,
            provider: SkillProvider::Codex,
            scope: SkillScope::Project,
        };

        handle.update(&mut app, |manager, ctx| {
            manager.handle_skills_added(vec![agents_skill, codex_skill], ctx);
        });

        let inventory = handle.read(&app, |manager, ctx| manager.list_skill_inventory(ctx));

        assert_eq!(inventory.len(), 1);
        assert_eq!(inventory[0].name, "deploy");
        assert_eq!(inventory[0].duplicates.len(), 2);
        assert_eq!(inventory[0].default_skill.path, agents_path);
        assert_eq!(inventory[0].default_skill.content, "# Deploy");
        assert!(inventory[0].has_duplicates());
    });
}

#[test]
fn skill_manager_emits_inventory_changed_when_skills_change() {
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);
        let saw_inventory_changed = Arc::new(AtomicBool::new(false));

        app.update(|ctx| {
            let saw_inventory_changed = saw_inventory_changed.clone();
            ctx.subscribe_to_model(&handle, move |_, event, _| {
                if matches!(event, SkillManagerEvent::InventoryChanged) {
                    saw_inventory_changed.store(true, Ordering::SeqCst);
                }
            });
        });

        let skill_path = PathBuf::from("/repo/.agents/skills/deploy/SKILL.md");
        let skill = ParsedSkill {
            name: "deploy".to_string(),
            description: "Deploy skill".to_string(),
            path: LocalOrRemotePath::Local(skill_path),
            content: "# Deploy".to_string(),
            line_range: None,
            provider: SkillProvider::Agents,
            scope: SkillScope::Project,
        };

        handle.update(&mut app, |manager, ctx| {
            manager.handle_skills_added(vec![skill], ctx);
        });

        assert!(saw_inventory_changed.load(Ordering::SeqCst));
    });
}

#[test]
fn get_skills_for_working_directory_scopes_subdirectory_skills() {
    // This test verifies the key scoping behavior:
    // - Root skills are visible from anywhere in the repo
    // - Subdirectory skills are only visible when working_directory is within that subdirectory

    // Use real temp directories so DetectedRepositories can canonicalize paths
    // and correctly report repo_root, which controls ancestor-vs-descendant scoping.
    // Canonicalize the temp base to avoid macOS /var -> /private/var symlink mismatches.
    let temp = TempDir::new().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let repo = base.join("repo");
    let frontend_dir = repo.join("packages/frontend");
    let backend_dir = repo.join("packages/backend");
    fs::create_dir_all(&frontend_dir).unwrap();
    fs::create_dir_all(&backend_dir).unwrap();

    // Create mock skills
    let root_skill_path = repo.join(".agents/skills/root-skill/SKILL.md");
    let frontend_skill_path = frontend_dir.join(".agents/skills/frontend-skill/SKILL.md");

    let root_skill = ParsedSkill {
        name: "root-skill".to_string(),
        description: "A root skill".to_string(),
        path: LocalOrRemotePath::Local(root_skill_path.clone()),
        content: "# Root skill".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let frontend_skill = ParsedSkill {
        name: "frontend-skill".to_string(),
        description: "A frontend skill".to_string(),
        path: LocalOrRemotePath::Local(frontend_skill_path.clone()),
        content: "# Frontend skill".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    // Build the internal state manually
    let mut directory_skills: HashMap<LocalOrRemotePath, HashSet<LocalOrRemotePath>> =
        HashMap::new();
    directory_skills
        .entry(LocalOrRemotePath::Local(repo.clone()))
        .or_default()
        .insert(LocalOrRemotePath::Local(root_skill_path.clone()));
    directory_skills
        .entry(LocalOrRemotePath::Local(frontend_dir.clone()))
        .or_default()
        .insert(LocalOrRemotePath::Local(frontend_skill_path.clone()));

    let mut skills_by_path: HashMap<LocalOrRemotePath, ParsedSkill> = HashMap::new();
    skills_by_path.insert(LocalOrRemotePath::Local(root_skill_path.clone()), root_skill);
    skills_by_path.insert(
        LocalOrRemotePath::Local(frontend_skill_path.clone()),
        frontend_skill,
    );

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        let repo_handle = app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let skill_manager_handle = app.add_singleton_model(SkillManager::new);

        // Register the repo root so get_root_for_path returns Some.
        let canonical_repo =
            warp_util::standardized_path::StandardizedPath::from_local_canonicalized(&repo)
                .unwrap();
        repo_handle.update(&mut app, |repos, _ctx| {
            repos.insert_test_repo_root(canonical_repo);
        });

        // Inject the test state
        skill_manager_handle.update(&mut app, |manager, _ctx| {
            manager.directory_skills = directory_skills;
            manager.skills_by_path = skills_by_path;
        });

        // Test 1: From frontend directory, should see both root and frontend skills
        let skills_from_frontend = skill_manager_handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(
                Some(&LocalOrRemotePath::Local(frontend_dir.clone())),
                ctx,
            )
        });
        let names_from_frontend: Vec<&str> = skills_from_frontend
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            names_from_frontend.contains(&"root-skill"),
            "Root skill should be visible from frontend dir"
        );
        assert!(
            names_from_frontend.contains(&"frontend-skill"),
            "Frontend skill should be visible from frontend dir"
        );

        // Test 2: From backend directory, should only see root skill (not frontend skill)
        let skills_from_backend = skill_manager_handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(
                Some(&LocalOrRemotePath::Local(backend_dir.clone())),
                ctx,
            )
        });
        let names_from_backend: Vec<&str> = skills_from_backend
            .iter()
            .map(|s| s.name.as_str())
            .collect();
        assert!(
            names_from_backend.contains(&"root-skill"),
            "Root skill should be visible from backend dir"
        );
        assert!(
            !names_from_backend.contains(&"frontend-skill"),
            "Frontend skill should NOT be visible from backend dir"
        );

        // Test 3: From repo root, should only see root skill (not frontend skill)
        let skills_from_root = skill_manager_handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&LocalOrRemotePath::Local(repo.clone())), ctx)
        });
        let names_from_root: Vec<&str> = skills_from_root.iter().map(|s| s.name.as_str()).collect();
        assert!(
            names_from_root.contains(&"root-skill"),
            "Root skill should be visible from repo root"
        );
        assert!(
            !names_from_root.contains(&"frontend-skill"),
            "Frontend skill should NOT be visible from repo root"
        );
    });
}

#[test]
fn get_skills_for_working_directory_name_collision_returns_both() {
    // When the same skill name exists at root and subdirectory, both should be returned.
    // The caller (agent) is responsible for precedence based on path proximity.

    // Use real temp directories so DetectedRepositories can canonicalize paths.
    // Canonicalize the temp base to avoid macOS /var -> /private/var symlink mismatches.
    let temp = TempDir::new().unwrap();
    let base = dunce::canonicalize(temp.path()).unwrap();
    let repo = base.join("repo");
    let subdir = repo.join("packages/frontend");
    fs::create_dir_all(&subdir).unwrap();

    let root_skill_path = repo.join(".agents/skills/deploy/SKILL.md");
    let subdir_skill_path = subdir.join(".agents/skills/deploy/SKILL.md");

    let root_skill = ParsedSkill {
        name: "deploy".to_string(),
        description: "Root deploy".to_string(),
        path: LocalOrRemotePath::Local(root_skill_path.clone()),
        content: "# Root deploy".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let subdir_skill = ParsedSkill {
        name: "deploy".to_string(),
        description: "Subdir deploy".to_string(),
        path: LocalOrRemotePath::Local(subdir_skill_path.clone()),
        content: "# Subdir deploy".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let mut directory_skills: HashMap<LocalOrRemotePath, HashSet<LocalOrRemotePath>> =
        HashMap::new();
    directory_skills
        .entry(LocalOrRemotePath::Local(repo.clone()))
        .or_default()
        .insert(LocalOrRemotePath::Local(root_skill_path.clone()));
    directory_skills
        .entry(LocalOrRemotePath::Local(subdir.clone()))
        .or_default()
        .insert(LocalOrRemotePath::Local(subdir_skill_path.clone()));

    let mut skills_by_path: HashMap<LocalOrRemotePath, ParsedSkill> = HashMap::new();
    skills_by_path.insert(LocalOrRemotePath::Local(root_skill_path.clone()), root_skill);
    skills_by_path.insert(
        LocalOrRemotePath::Local(subdir_skill_path.clone()),
        subdir_skill,
    );

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        let repo_handle = app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let skill_manager_handle = app.add_singleton_model(SkillManager::new);

        // Register the repo root so get_root_for_path returns Some.
        let canonical_repo =
            warp_util::standardized_path::StandardizedPath::from_local_canonicalized(&repo)
                .unwrap();
        repo_handle.update(&mut app, |repos, _ctx| {
            repos.insert_test_repo_root(canonical_repo);
        });

        skill_manager_handle.update(&mut app, |manager, _ctx| {
            manager.directory_skills = directory_skills;
            manager.skills_by_path = skills_by_path;
        });

        // From subdir: should see both "deploy" skills (root + subdir)
        let skills = skill_manager_handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&LocalOrRemotePath::Local(subdir.clone())), ctx)
        });
        let deploy_skills: Vec<_> = skills.iter().filter(|s| s.name == "deploy").collect();
        assert_eq!(
            deploy_skills.len(),
            2,
            "Both deploy skills should be visible from subdir"
        );

        // From repo root: should only see root "deploy"
        let skills = skill_manager_handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&LocalOrRemotePath::Local(repo.clone())), ctx)
        });
        let deploy_skills: Vec<_> = skills.iter().filter(|s| s.name == "deploy").collect();
        assert_eq!(
            deploy_skills.len(),
            1,
            "Only root deploy should be visible from repo root"
        );
        assert_eq!(deploy_skills[0].description, "Root deploy");
    });
}

#[test]
fn test_read_bundled_skills_with_variable_substitution() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    // Create a test skill with variables
    let skill_dir = skills_dir.join("test-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");
    fs::write(
        &skill_file,
        r#"---
name: test-skill
description: Test skill with variables
---

Run `{{warp_cli_binary_name}}` to connect to {{warp_server_url}}.
"#,
    )
    .unwrap();

    let skills = futures::executor::block_on(read_bundled_skills(skills_dir));

    assert_eq!(skills.len(), 1);
    let skill = skills.get("test-skill").unwrap();

    let expected_cli = ChannelState::channel().cli_command_name();
    assert!(skill
        .content
        .contains(&format!("Run `{expected_cli}` to connect to .")));
}

#[test]
fn test_read_bundled_skills_preserves_other_content() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    // Create a test skill with both warp and non-warp variables
    let skill_dir = skills_dir.join("test-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");
    fs::write(
        &skill_file,
        r#"---
name: test-skill
description: Test skill with mixed variables
---

Use {{other_var}} and {{warp_cli_binary_name}} together.
"#,
    )
    .unwrap();

    let skills = futures::executor::block_on(read_bundled_skills(skills_dir));

    assert_eq!(skills.len(), 1);
    let skill = skills.get("test-skill").unwrap();

    let expected_cli = ChannelState::channel().cli_command_name();
    assert!(skill.content.contains(&format!(
        "Use {{{{other_var}}}} and {expected_cli} together."
    )));
}

#[test]
fn test_read_bundled_skills_no_variables() {
    let temp_dir = TempDir::new().unwrap();
    let skills_dir = temp_dir.path();

    // Create a test skill with no variables
    let skill_dir = skills_dir.join("test-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");
    fs::write(
        &skill_file,
        r#"---
name: test-skill
description: Test skill without variables
---

Plain content with no variables.
"#,
    )
    .unwrap();

    let skills = futures::executor::block_on(read_bundled_skills(skills_dir));

    assert_eq!(skills.len(), 1);
    let skill = skills.get("test-skill").unwrap();
    assert!(skill.content.contains("Plain content with no variables."));
}

#[test]
fn test_build_bundled_skill_context() {
    let context = build_bundled_skill_context();

    // At least 6 entries: server_url, cli_binary_name, warpctrl_binary_name,
    // warpctrl_wrapper_path, url_scheme, settings_file_path, keybindings_file_path.
    // settings_schema_path is only present when bundled_resources_dir() returns Some.
    assert!(context.len() >= 6);
    assert!(context.contains_key("warp_server_url"));
    assert!(context.contains_key("warp_cli_binary_name"));
    assert!(context.contains_key("warp_url_scheme"));
    assert!(context.contains_key("settings_file_path"));
    // #370: the `warpctrl` and `change-keybinding` bundled skills render these,
    // and `handlebars::render_template` leaves an unknown `{{name}}` verbatim in
    // the skill text rather than erroring, so a missing key is silent.
    assert!(context.contains_key("warpctrl_binary_name"));
    assert!(context.contains_key("warpctrl_wrapper_path"));
    assert!(context.contains_key("keybindings_file_path"));

    assert_eq!(context.get("warp_server_url").unwrap(), "");
    assert_eq!(
        context.get("warp_cli_binary_name").unwrap(),
        ChannelState::channel().cli_command_name()
    );
    assert_eq!(
        context.get("warp_url_scheme").unwrap(),
        ChannelState::url_scheme()
    );
    assert_eq!(
        context.get("settings_file_path").unwrap(),
        &crate::settings::user_preferences_toml_file_path()
            .display()
            .to_string()
    );
    assert_eq!(
        context.get("warpctrl_binary_name").unwrap(),
        ChannelState::channel().warpctrl_command_name()
    );
    assert_eq!(
        context.get("keybindings_file_path").unwrap(),
        &crate::keyboard::keybinding_file_path()
            .display()
            .to_string()
    );
}

// ============================================================================
// Tests for best_supported_provider
// ============================================================================

/// Helper: creates a ParsedSkill under a given provider directory.
fn make_skill(name: &str, provider_dir: &str) -> ParsedSkill {
    let path = PathBuf::from(format!("/repo/{provider_dir}/skills/{name}/SKILL.md"));
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} skill"),
        path: LocalOrRemotePath::Local(path),
        content: format!("# {name}"),
        line_range: None,
        provider: get_provider_for_path(&LocalOrRemotePath::Local(PathBuf::from(format!(
            "/repo/{provider_dir}/skills/{name}/SKILL.md"
        ))))
        .unwrap_or(SkillProvider::Zap),
        scope: SkillScope::Project,
    }
}

#[test]
fn best_supported_provider_fast_path_returns_deduped_provider() {
    // When the deduped provider is already in the supported set, return it immediately.
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let claude_skill = make_skill("deploy", ".claude");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(claude_skill.clone());
        });

        let descriptor = SkillDescriptor::from(claude_skill);
        let result = handle.read(&app, |manager, _| {
            manager.best_supported_provider(&descriptor, &[SkillProvider::Claude])
        });
        assert_eq!(result, SkillProvider::Claude);
    });
}

#[test]
fn best_supported_provider_remaps_to_supported_provider() {
    // Skill exists under both .agents and .claude. Dedup picked Agents (higher priority).
    // When supported set is [Claude], should re-map to Claude.
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let agents_skill = make_skill("deploy", ".agents");
        let claude_skill = make_skill("deploy", ".claude");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(agents_skill.clone());
            manager.add_skill_for_testing(claude_skill.clone());
        });

        // Descriptor has provider = Agents (the dedup winner).
        let descriptor = SkillDescriptor::from(agents_skill);
        assert_eq!(descriptor.provider, SkillProvider::Agents);

        let result = handle.read(&app, |manager, _| {
            manager.best_supported_provider(&descriptor, &[SkillProvider::Claude])
        });
        assert_eq!(result, SkillProvider::Claude);
    });
}

#[test]
fn best_supported_provider_falls_back_when_no_match() {
    // Skill only exists under .agents, but the supported set is [Claude].
    // Should fall back to the original deduped provider (Agents).
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let agents_skill = make_skill("deploy", ".agents");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(agents_skill.clone());
        });

        let descriptor = SkillDescriptor::from(agents_skill);
        let result = handle.read(&app, |manager, _| {
            manager.best_supported_provider(&descriptor, &[SkillProvider::Claude])
        });
        // No .claude path exists, so falls back to the deduped provider.
        assert_eq!(result, SkillProvider::Agents);
    });
}

// ============================================================================
// Tests for find_skill_by_name (the BYOP read_skill tool looking up ParsedSkill by name)
// ============================================================================

#[test]
fn find_skill_by_name_returns_filesystem_skill() {
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let deploy_skill = make_skill("deploy", ".agents");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(deploy_skill.clone());
        });

        let resolved = handle.read(&app, |manager, _| {
            manager.find_skill_by_name("deploy").cloned()
        });
        let resolved = resolved.expect("expected to find skill by name");
        assert_eq!(resolved.name, "deploy");
        assert_eq!(resolved.path, deploy_skill.path);
    });
}

#[test]
fn find_skill_by_name_prefers_higher_priority_provider() {
    // When a same-name skill is registered in both .agents and .claude,
    // find_skill_by_name should return the .agents (rank 0) copy, per
    // SKILL_PROVIDER_DEFINITIONS order.
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        let agents_skill = make_skill("deploy", ".agents");
        let claude_skill = make_skill("deploy", ".claude");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(claude_skill.clone());
            manager.add_skill_for_testing(agents_skill.clone());
        });

        let resolved = handle.read(&app, |manager, _| {
            manager.find_skill_by_name("deploy").cloned()
        });
        let resolved = resolved.expect("expected to find skill by name");
        assert_eq!(resolved.provider, SkillProvider::Agents);
        assert_eq!(resolved.path, agents_skill.path);
    });
}

#[test]
fn find_skill_by_name_returns_none_for_unknown_name() {
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        // Registers a skill to ensure the manager's index is non-empty — otherwise
        // the `mut app` warning would get flagged by unused_mut (no other manager
        // operation here modifies app itself).
        let known = make_skill("deploy", ".agents");
        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(known);
        });

        let resolved = handle.read(&app, |manager, _| {
            manager.find_skill_by_name("nope").cloned()
        });
        assert!(resolved.is_none());
    });
}

// ============================================================================
// Tests for the SkillPathOrigin-aware remote-catalog surface (#487).
//
// Ported (and adapted) from the pin's `skill_manager_tests.rs` (`02b53fcd8`). Adapted
// because `handle_skills_added` / `set_remote_home_skills` / `remove_remote_home_skills`
// take a `&mut ModelContext<Self>` here (unlike the pin) so they can emit
// `SkillManagerEvent::InventoryChanged` for the fork-original inventory panel — every
// pin call site below threads `ctx` through accordingly. `cloud_environment_skills_
// always_included` is not ported: it exercises `SkillManager::is_cloud_environment` /
// `set_cloud_environment`, which are cloud-runner-only and not built here (see this
// file's module doc comment).
// ============================================================================

fn remote_test_path(host_id: &HostId, path: &str) -> LocalOrRemotePath {
    LocalOrRemotePath::Remote(RemotePath::new(
        host_id.clone(),
        StandardizedPath::try_new(path).unwrap(),
    ))
}

fn make_remote_home_skill(host_id: &HostId, name: &str, content: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} remote home skill"),
        path: remote_test_path(
            host_id,
            format!("/home/user/.agents/skills/{name}/SKILL.md").as_str(),
        ),
        content: content.to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Home,
    }
}

fn make_remote_skill(host_id: &HostId, name: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} remote skill"),
        path: LocalOrRemotePath::Remote(RemotePath::new(
            host_id.clone(),
            StandardizedPath::try_new(format!("/repo/.agents/skills/{name}/SKILL.md").as_str())
                .unwrap(),
        )),
        content: format!("# {name}"),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    }
}

fn bundled_test_skill(id: &str, description: &str) -> ParsedSkill {
    ParsedSkill {
        name: id.to_owned(),
        description: description.to_owned(),
        path: LocalOrRemotePath::Local(format!("/bundled/skills/{id}/SKILL.md").into()),
        content: format!("# {id}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

#[test]
fn get_skills_for_working_directory_respects_location() {
    let same_host_id = HostId::new("same-host".to_string());
    let other_host_id = HostId::new("other-host".to_string());
    let home_dir = LocalOrRemotePath::Local(dirs::home_dir().unwrap());
    let local_project_dir =
        LocalOrRemotePath::Local(std::env::temp_dir().join("skill-path-scope-project"));
    let same_host_dir = LocalOrRemotePath::Remote(RemotePath::new(
        same_host_id.clone(),
        StandardizedPath::try_new("/repo").unwrap(),
    ));
    let other_host_dir = LocalOrRemotePath::Remote(RemotePath::new(
        other_host_id.clone(),
        StandardizedPath::try_new("/repo").unwrap(),
    ));

    let local_home_skill = ParsedSkill {
        name: "local-home".to_string(),
        description: "local home skill".to_string(),
        path: home_dir.join(".agents/skills/local-home/SKILL.md"),
        content: "# local-home".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Home,
    };
    let local_project_skill = ParsedSkill {
        name: "local-project".to_string(),
        description: "local project skill".to_string(),
        path: local_project_dir.join(".agents/skills/local-project/SKILL.md"),
        content: "# local-project".to_string(),
        line_range: None,
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };
    let same_host_skill = make_remote_skill(&same_host_id, "same-host-project");
    let other_host_skill = make_remote_skill(&other_host_id, "other-host-project");
    let bundled_skill = bundled_test_skill("bundled", "bundled skill");
    // A bundled skill from the remote host's daemon-pushed catalog.
    let remote_bundled_skill = ParsedSkill {
        name: "remote-bundled".to_string(),
        description: "remote bundled skill".to_string(),
        path: LocalOrRemotePath::Remote(RemotePath::new(
            same_host_id.clone(),
            StandardizedPath::try_new(
                "/home/user/.warp/remote-server/bundled_resources/bundled/skills/remote-bundled/SKILL.md",
            )
            .unwrap(),
        )),
        content: "# remote-bundled".to_string(),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    };

    let remote_bundled_path = remote_bundled_skill.path.clone();

    let mut directory_skills = HashMap::new();
    let mut skills_by_path = HashMap::new();
    for (dir, skill) in [
        (home_dir, local_home_skill),
        (local_project_dir.clone(), local_project_skill),
        (same_host_dir.clone(), same_host_skill),
        (other_host_dir.clone(), other_host_skill),
    ] {
        directory_skills
            .entry(dir)
            .or_insert_with(HashSet::new)
            .insert(skill.path.clone());
        skills_by_path.insert(skill.path.clone(), skill);
    }

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);
        let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);

        handle.update(&mut app, |manager, _| {
            manager.directory_skills = directory_skills;
            manager.skills_by_path = skills_by_path;
            manager.add_bundled_skill_for_testing(
                "bundled",
                bundled_skill,
                BundledSkillActivation::Always,
            );
            manager.set_remote_bundled_skill(
                same_host_id.clone(),
                BundledSkill::from_definitions([(
                    "remote-bundled".to_string(),
                    remote_bundled_skill,
                    BundledSkillActivation::Always,
                )]),
            );
        });

        // A remote working directory sees the remote host's bundled catalog,
        // never the local client's.
        let remote_skills = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&same_host_dir), ctx)
        });
        let remote_names: HashSet<_> = remote_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert!(remote_names.contains("same-host-project"));
        assert!(remote_names.contains("remote-bundled"));
        assert!(!remote_names.contains("bundled"));
        assert!(!remote_names.contains("local-home"));
        assert!(!remote_names.contains("local-project"));
        assert!(!remote_names.contains("other-host-project"));
        let other_remote_skills = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&other_host_dir), ctx)
        });
        let other_remote_names: HashSet<_> = other_remote_skills
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert!(other_remote_names.contains("other-host-project"));
        assert!(!other_remote_names.contains("bundled"));

        // Remote catalog descriptors are path-referenced, and that reference
        // resolves back to the remote host's catalog entry. A
        // `BundledSkillId` reference would resolve against the local catalog
        // and serve the wrong content.
        let remote_bundled_descriptor = remote_skills
            .iter()
            .find(|skill| skill.name == "remote-bundled")
            .unwrap();
        assert_eq!(
            remote_bundled_descriptor.reference,
            SkillReference::Path(remote_bundled_path.clone())
        );
        // Path-referenced remote bundled descriptors keep their bundled scope:
        // filters that hide non-invokable bundled skills (e.g. the `/open-skill`
        // selector) key off the scope, not the reference variant.
        assert_eq!(remote_bundled_descriptor.scope, SkillScope::Bundled);
        let resolved_content = handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(
                    &remote_bundled_descriptor.reference,
                    &SkillPathOrigin::Remote {
                        host_id: same_host_id.clone(),
                    },
                    ctx,
                )
                .map(|skill| skill.content.clone())
        });
        assert_eq!(resolved_content, Ok("# remote-bundled".to_string()));
        let wrong_origin_content = handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(
                    &remote_bundled_descriptor.reference,
                    &SkillPathOrigin::Local,
                    ctx,
                )
                .map(|skill| skill.content.clone())
        });
        assert!(matches!(
            wrong_origin_content,
            Err(ActiveSkillLookupError::NotFound { .. })
        ));

        let local_skills_without_cwd = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(None, ctx)
        });
        let local_names_without_cwd: HashSet<_> = local_skills_without_cwd
            .iter()
            .map(|skill| skill.name.as_str())
            .collect();
        assert_eq!(
            local_names_without_cwd,
            HashSet::from(["local-home", "bundled"])
        );
        let unavailable_remote_skills = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory_with_origin(
                None,
                &SkillPathOrigin::Unavailable,
                ctx,
            )
        });
        assert!(
            unavailable_remote_skills.is_empty(),
            "Unavailable remote sessions must not fall back to client-global bundles"
        );

        let local_skills = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&local_project_dir), ctx)
        });
        let local_names: HashSet<_> = local_skills.iter().map(|skill| skill.name.as_str()).collect();
        assert!(local_names.contains("local-home"));
        assert!(local_names.contains("local-project"));
        assert!(local_names.contains("bundled"));
        assert!(!local_names.contains("remote-bundled"));
        assert!(!local_names.contains("same-host-project"));
        assert!(!local_names.contains("other-host-project"));

        // Local catalog descriptors keep their ID-based references.
        let local_bundled_descriptor = local_skills
            .iter()
            .find(|skill| skill.name == "bundled")
            .unwrap();
        assert_eq!(
            local_bundled_descriptor.reference,
            SkillReference::BundledSkillId("bundled".to_string())
        );
    });
}

#[test]
fn remote_home_skills_are_host_scoped_replaceable_and_path_invokable() {
    let first_host = HostId::new("first-host".to_string());
    let second_host = HostId::new("second-host".to_string());
    let first_skill = make_remote_home_skill(&first_host, "deploy", "first host content");
    let second_skill = make_remote_home_skill(&second_host, "deploy", "second host content");
    let first_skill_path = first_skill.path.clone();
    let second_skill_path = second_skill.path.clone();
    let first_reference = SkillReference::Path(first_skill.path.clone());
    let second_reference = SkillReference::Path(second_skill.path.clone());
    let first_cwd = remote_test_path(&first_host, "/work/repo");
    let second_cwd = remote_test_path(&second_host, "/other/repo");

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, ctx| {
            manager.set_remote_home_skills(
                first_host.clone(),
                remote_test_path(&first_host, "/home/user"),
                vec![first_skill],
                ctx,
            );
            manager.set_remote_home_skills(
                second_host.clone(),
                remote_test_path(&second_host, "/home/user"),
                vec![second_skill],
                ctx,
            );
        });
        assert_eq!(
            handle
                .read(&app, |manager, _| manager.skill_paths_by_name("deploy"))
                .into_iter()
                .collect::<HashSet<_>>(),
            HashSet::from([first_skill_path.clone(), second_skill_path.clone()])
        );

        for (cwd, host_id, expected_content, reference) in [
            (
                &first_cwd,
                &first_host,
                "first host content",
                &first_reference,
            ),
            (
                &second_cwd,
                &second_host,
                "second host content",
                &second_reference,
            ),
        ] {
            let descriptors = handle.read(&app, |manager, ctx| {
                manager.get_skills_for_working_directory(Some(cwd), ctx)
            });
            assert_eq!(
                descriptors
                    .iter()
                    .filter(|skill| skill.name == "deploy")
                    .count(),
                1
            );
            assert_eq!(
                handle.read(&app, |manager, ctx| {
                    manager
                        .active_skill_by_reference_with_origin(
                            reference,
                            &SkillPathOrigin::Remote {
                                host_id: host_id.clone(),
                            },
                            ctx,
                        )
                        .ok()
                        .map(|skill| skill.content.clone())
                }),
                Some(expected_content.to_string())
            );
        }

        let first_host_without_cwd = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory_with_origin(
                None,
                &SkillPathOrigin::Remote {
                    host_id: first_host.clone(),
                },
                ctx,
            )
        });
        assert_eq!(
            first_host_without_cwd
                .iter()
                .filter(|skill| skill.name == "deploy")
                .count(),
            1
        );
        assert_eq!(
            first_host_without_cwd
                .iter()
                .find(|skill| skill.name == "deploy")
                .map(|skill| &skill.reference),
            Some(&first_reference)
        );
        assert!(
            handle
                .read(&app, |manager, ctx| manager
                    .get_skills_for_working_directory(None, ctx))
                .iter()
                .all(|skill| skill.name != "deploy")
        );

        handle.update(&mut app, |manager, ctx| {
            manager.set_remote_home_skills(
                first_host.clone(),
                remote_test_path(&first_host, "/home/user"),
                Vec::new(),
                ctx,
            );
        });
        assert!(handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(
                    &first_reference,
                    &SkillPathOrigin::Remote {
                        host_id: first_host.clone(),
                    },
                    ctx,
                )
                .is_err()
        }));
        assert!(handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(
                    &second_reference,
                    &SkillPathOrigin::Remote {
                        host_id: second_host.clone(),
                    },
                    ctx,
                )
                .is_ok()
        }));
        assert_eq!(
            handle.read(&app, |manager, _| manager.skill_paths_by_name("deploy")),
            vec![second_skill_path]
        );
    });
}

#[test]
fn removing_remote_home_skills_preserves_project_skills_below_home() {
    let host_id = HostId::new("remote-host".to_string());
    let home_dir = remote_test_path(&host_id, "/home/user");
    let home_skill = make_remote_home_skill(&host_id, "home", "home content");
    let home_skill_path = home_skill.path.clone();
    let project_dir = remote_test_path(&host_id, "/home/user/repo");
    let project_skill = ParsedSkill {
        path: project_dir.join(".agents/skills/project/SKILL.md"),
        ..make_remote_skill(&host_id, "project")
    };
    let project_skill_path = project_skill.path.clone();

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, ctx| {
            manager.handle_skills_added(vec![project_skill], ctx);
            manager.set_remote_home_skills(host_id.clone(), home_dir, vec![home_skill], ctx);
            manager.remove_remote_home_skills(&host_id, ctx);
        });

        handle.read(&app, |manager, _| {
            assert!(manager.skill_by_path(&home_skill_path).is_none());
            assert!(manager.skill_by_path(&project_skill_path).is_some());
            assert!(manager.skill_paths_by_name("home").is_empty());
            assert_eq!(
                manager.skill_paths_by_name("project"),
                vec![project_skill_path]
            );
        });
    });
}

// ============================================================================
// Bundled-skill activation gating (#370)
//
// Ported from the pin's `skill_manager_tests.rs` (`02b53fcd8`) alongside the
// `warpctrl` and `change-keybinding` skill directories. The pin's
// `tui_migration_skill_has_tui_only_activation` is NOT ported: it asserts on
// `activation_for_bundled_skill("tui-migrate-setup", ..)`, and this fork does
// not ship that skill -- see the reasoning on `activation_for_bundled_skill`
// in `bundled.rs`. Its replacement, `tui-settings`, is deliberately `Always`
// rather than `TuiOnly`, and is covered by
// `tui_settings_bundled_skill_is_active_in_both_frontends` in
// `bundled_tests.rs`. `TuiOnly` itself is still covered, by
// `tui_only_bundled_skill_is_listed_and_resolved_only_in_tui` below, which
// drives the variant directly rather than through a shipped skill -- no
// bundled skill in this fork uses it.
// ============================================================================

#[test]
fn feature_gated_bundled_skill_is_listed_only_when_enabled() {
    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);
        let bundled_skills_guard = FeatureFlag::BundledSkills.override_enabled(true);
        let warp_control_cli = FeatureFlag::WarpControlCli.override_enabled(false);

        handle.update(&mut app, |manager, _| {
            manager.add_bundled_skill_for_testing(
                "warpctrl",
                bundled_test_skill("warpctrl", "Control Phosphor"),
                BundledSkillActivation::RequiresFeature(FeatureFlag::WarpControlCli),
            );
            manager.add_bundled_skill_for_testing(
                "always",
                bundled_test_skill("always", "Always available"),
                BundledSkillActivation::Always,
            );
        });

        let disabled_names = handle.read(&app, |manager, ctx| {
            manager
                .get_skills_for_working_directory(None, ctx)
                .into_iter()
                .map(|skill| skill.name)
                .collect::<HashSet<_>>()
        });
        assert!(!disabled_names.contains("warpctrl"));
        assert!(disabled_names.contains("always"));

        drop(warp_control_cli);
        let warp_control_cli_enabled = FeatureFlag::WarpControlCli.override_enabled(true);
        let enabled_names = handle.read(&app, |manager, ctx| {
            manager
                .get_skills_for_working_directory(None, ctx)
                .into_iter()
                .map(|skill| skill.name)
                .collect::<HashSet<_>>()
        });
        assert!(enabled_names.contains("warpctrl"));
        assert!(enabled_names.contains("always"));
        drop(warp_control_cli_enabled);
        drop(bundled_skills_guard);
    });
}

#[test]
fn tui_only_bundled_skill_is_listed_and_resolved_only_in_tui() {
    for (execution_mode, expected_active) in
        [(ExecutionMode::App, false), (ExecutionMode::Tui, true)]
    {
        App::test((), |mut app| async move {
            app.add_singleton_model(|ctx| AppExecutionMode::new(execution_mode, false, ctx));
            app.add_singleton_model(DirectoryWatcher::new);
            app.add_singleton_model(AISettings::new_with_defaults);
            app.add_singleton_model(|_| DetectedRepositories::default());
            app.add_singleton_model(RepoMetadataModel::new);
            app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
            app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
            let handle = app.add_singleton_model(SkillManager::new);
            let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
            let reference = SkillReference::BundledSkillId("tui-only-fixture".to_owned());

            handle.update(&mut app, |manager, _| {
                manager.add_bundled_skill_for_testing(
                    "tui-only-fixture",
                    bundled_test_skill("tui-only-fixture", "TuiOnly activation fixture"),
                    BundledSkillActivation::TuiOnly,
                );
            });

            let listed = handle.read(&app, |manager, ctx| {
                manager
                    .get_skills_for_working_directory(None, ctx)
                    .iter()
                    .any(|skill| skill.name == "tui-only-fixture")
            });
            let resolved = handle.read(&app, |manager, ctx| {
                manager.active_skill_by_reference(&reference, ctx).is_some()
            });

            assert_eq!(listed, expected_active);
            assert_eq!(resolved, expected_active);
        });
    }
}

#[test]
fn warp_control_bundled_skill_activations_track_warp_control_feature() {
    App::test((), |app| async move {
        let settings = app.add_singleton_model(AISettings::new_with_defaults);
        let warp_control_cli = FeatureFlag::WarpControlCli.override_enabled(false);
        let activations = ["warpctrl"]
            .map(|skill_id| activation_for_bundled_skill(skill_id, Path::new("/resources")));
        for activation in &activations {
            assert!(!settings.read(&app, |_, ctx| activation.is_enabled(ctx)));
        }

        drop(warp_control_cli);
        let warp_control_cli_enabled = FeatureFlag::WarpControlCli.override_enabled(true);
        for activation in &activations {
            assert!(settings.read(&app, |_, ctx| activation.is_enabled(ctx)));
        }
        drop(warp_control_cli_enabled);
    });
}

#[test]
fn warp_control_direct_read_respects_warp_control_feature() {
    let reference = SkillReference::BundledSkillId("warpctrl".to_owned());

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);
        let warp_control_cli = FeatureFlag::WarpControlCli.override_enabled(false);

        handle.update(&mut app, |manager, _| {
            manager.add_bundled_skill_for_testing(
                "warpctrl",
                bundled_test_skill("warpctrl", "Control Phosphor"),
                BundledSkillActivation::RequiresFeature(FeatureFlag::WarpControlCli),
            );
        });

        assert!(handle.read(&app, |manager, _| {
            manager.skill_by_reference(&reference).is_some()
        }));
        assert!(handle.read(&app, |manager, ctx| {
            manager.active_skill_by_reference(&reference, ctx).is_none()
        }));

        drop(warp_control_cli);
        let warp_control_cli_enabled = FeatureFlag::WarpControlCli.override_enabled(true);
        assert!(handle.read(&app, |manager, ctx| {
            manager.active_skill_by_reference(&reference, ctx).is_some()
        }));
        drop(warp_control_cli_enabled);
    });
}

// ── Ported from the pinned oracle (02b53fcd8),
// `app/src/ai/skills/skill_manager_tests.rs` ──
//
// `SkillReference::Path` lookup for a REMOTE skill. Everything here already
// covered the local-path and bundled-id cases; the remote-path case had no test,
// and it is the one that can go wrong quietly. `LocalOrRemotePath::Remote`
// compares host *and* path, so two SSH hosts with an identically-named skill at
// an identical path must not resolve to each other's copy -- the failure mode is
// running the wrong host's skill, with no error anywhere.

#[test]
fn active_skill_by_reference_resolves_exact_remote_identity() {
    let remote_skill = make_remote_skill(&HostId::new("remote-host".to_string()), "deploy");
    let reference = SkillReference::Path(remote_skill.path.clone());

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(remote_skill.clone());
        });

        let resolved = handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference(&reference, ctx)
                .map(|skill| skill.path.clone())
        });

        assert_eq!(resolved, Some(remote_skill.path));
    });
}

#[test]
fn active_skill_by_reference_distinguishes_remote_hosts_with_the_same_display_path() {
    let first_skill = make_remote_skill(&HostId::new("first-host".to_string()), "deploy");
    let second_skill = make_remote_skill(&HostId::new("second-host".to_string()), "deploy");
    let first_path = first_skill.path.clone();
    let second_path = second_skill.path.clone();
    let first_reference = SkillReference::Path(first_skill.path.clone());
    let second_reference = SkillReference::Path(second_skill.path.clone());

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, _| {
            manager.add_skill_for_testing(first_skill);
            manager.add_skill_for_testing(second_skill);
        });

        let resolved = handle.read(&app, |manager, ctx| {
            (
                manager
                    .active_skill_by_reference(&first_reference, ctx)
                    .map(|skill| skill.path.clone()),
                manager
                    .active_skill_by_reference(&second_reference, ctx)
                    .map(|skill| skill.path.clone()),
            )
        });
        assert_eq!(resolved, (Some(first_path), Some(second_path)));
    });
}

/// Origin-aware lookup reports *why* a skill could not be resolved: a bundled id
/// on a session with no bundled catalog is `BundledSkillsUnavailable`, not a
/// generic "not found". The distinction is what the agent surfaces to the user.
#[test]
fn active_skill_by_reference_with_origin_returns_typed_lookup_errors() {
    App::test((), |app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);
        let reference = SkillReference::BundledSkillId("missing".to_string());

        let unavailable_error = handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(
                    &reference,
                    &SkillPathOrigin::Unavailable,
                    ctx,
                )
                .unwrap_err()
        });
        assert_eq!(
            unavailable_error,
            ActiveSkillLookupError::BundledSkillsUnavailable
        );

        let not_found_error = handle.read(&app, |manager, ctx| {
            manager
                .active_skill_by_reference_with_origin(&reference, &SkillPathOrigin::Local, ctx)
                .unwrap_err()
        });
        assert_eq!(
            not_found_error,
            ActiveSkillLookupError::NotFound { reference }
        );
    });
}

// ============================================================================
// Tests for remote home skill provider-variant resolution (ported from the pin)
// ============================================================================

#[test]
fn remote_home_provider_variants_are_available_for_provider_selection() {
    let host_id = HostId::new("remote-host".to_string());
    let agents_skill = make_remote_home_skill(&host_id, "deploy", "shared content");
    let claude_skill = ParsedSkill {
        path: remote_test_path(&host_id, "/home/user/.claude/skills/deploy/SKILL.md"),
        provider: SkillProvider::Claude,
        ..agents_skill.clone()
    };

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, ctx| {
            manager.set_remote_home_skills(
                host_id.clone(),
                remote_test_path(&host_id, "/home/user"),
                vec![agents_skill, claude_skill],
                ctx,
            );
        });
        let descriptor = handle
            .read(&app, |manager, ctx| {
                manager.get_skills_for_working_directory_with_origin(
                    None,
                    &SkillPathOrigin::Remote {
                        host_id: host_id.clone(),
                    },
                    ctx,
                )
            })
            .into_iter()
            .find(|skill| skill.name == "deploy")
            .unwrap();

        assert_eq!(descriptor.provider, SkillProvider::Agents);
        assert!(handle.read(&app, |manager, _| {
            manager.skill_exists_for_any_provider(&descriptor, &[SkillProvider::Claude])
        }));
        assert_eq!(
            handle.read(&app, |manager, _| manager
                .best_supported_provider(&descriptor, &[SkillProvider::Claude])),
            SkillProvider::Claude
        );
    });
}

#[test]
fn remote_home_provider_variants_are_scoped_to_the_descriptor_host() {
    let first_host = HostId::new("first-host".to_string());
    let second_host = HostId::new("second-host".to_string());
    let first_skill = make_remote_home_skill(&first_host, "deploy", "shared content");
    let second_skill = ParsedSkill {
        path: remote_test_path(&second_host, "/home/user/.claude/skills/deploy/SKILL.md"),
        provider: SkillProvider::Claude,
        ..make_remote_home_skill(&second_host, "deploy", "shared content")
    };

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, ctx| {
            manager.set_remote_home_skills(
                first_host.clone(),
                remote_test_path(&first_host, "/home/user"),
                vec![first_skill],
                ctx,
            );
            manager.set_remote_home_skills(
                second_host.clone(),
                remote_test_path(&second_host, "/home/user"),
                vec![second_skill],
                ctx,
            );
        });
        let descriptor = handle
            .read(&app, |manager, ctx| {
                manager.get_skills_for_working_directory_with_origin(
                    None,
                    &SkillPathOrigin::Remote {
                        host_id: first_host,
                    },
                    ctx,
                )
            })
            .into_iter()
            .find(|skill| skill.name == "deploy")
            .unwrap();

        assert!(!handle.read(&app, |manager, _| {
            manager.skill_exists_for_any_provider(&descriptor, &[SkillProvider::Claude])
        }));
        assert_eq!(
            handle.read(&app, |manager, _| manager
                .best_supported_provider(&descriptor, &[SkillProvider::Claude])),
            SkillProvider::Agents
        );
    });
}

#[test]
fn remote_home_skill_replaces_an_overlapping_index_entry() {
    let host_id = HostId::new("remote-host".to_string());
    let home_dir = remote_test_path(&host_id, "/home/user");
    let working_directory = home_dir.join("repo");
    let home_skill = make_remote_home_skill(&host_id, "deploy", "shared content");
    let project_skill = ParsedSkill {
        scope: SkillScope::Project,
        ..home_skill.clone()
    };

    App::test((), |mut app| async move {
        app.add_singleton_model(DirectoryWatcher::new);
        app.add_singleton_model(AISettings::new_with_defaults);
        app.add_singleton_model(|_| DetectedRepositories::default());
        app.add_singleton_model(RepoMetadataModel::new);
        app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
        app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
        let handle = app.add_singleton_model(SkillManager::new);

        handle.update(&mut app, |manager, ctx| {
            manager
                .directory_skills
                .entry(home_dir.clone())
                .or_default()
                .insert(project_skill.path.clone());
            manager
                .skills_by_path
                .insert(project_skill.path.clone(), project_skill);
            manager.set_remote_home_skills(host_id, home_dir, vec![home_skill], ctx);
        });

        let descriptors = handle.read(&app, |manager, ctx| {
            manager.get_skills_for_working_directory(Some(&working_directory), ctx)
        });
        assert_eq!(
            descriptors
                .iter()
                .filter(|skill| skill.name == "deploy")
                .count(),
            1,
            "the indexed remote home skill should be listed once"
        );
    });
}
