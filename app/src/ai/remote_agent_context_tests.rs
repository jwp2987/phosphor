use repo_metadata::repositories::DetectedRepositories;
use repo_metadata::{DirectoryWatcher, RepoMetadataModel};
use warpui::App;
use watcher::HomeDirectoryWatcher;

use super::*;
use crate::settings::AISettings;
use crate::warp_managed_paths_watcher::WarpManagedPathsWatcher;

fn bundled_skill(
    id: &str,
    path: &str,
    content: &str,
    requires_mcp: Option<&str>,
) -> RemoteSkillProto {
    RemoteSkillProto {
        path: path.to_string(),
        content: content.to_string(),
        source: Some(remote_skill_proto::Source::Bundled(
            remote_server::proto::BundledSkillMetadata {
                id: id.to_string(),
                requires_mcp: requires_mcp.map(str::to_string),
            },
        )),
    }
}

fn home_skill(path: &str, content: &str) -> RemoteSkillProto {
    RemoteSkillProto {
        path: path.to_string(),
        content: content.to_string(),
        source: Some(remote_skill_proto::Source::Home(
            remote_server::proto::HomeSkillMetadata {},
        )),
    }
}

fn snapshot() -> RemoteAgentContextSnapshot {
    RemoteAgentContextSnapshot {
        revision: 7,
        home_dir: "/home/user".to_string(),
        skills: vec![
            bundled_skill(
                "bundled",
                "/bundled/skills/bundled/SKILL.md",
                "---\nname: bundled\ndescription: Bundled skill\n---\nbody",
                None,
            ),
            bundled_skill(
                "unknown-mcp",
                "/bundled/skills/unknown/SKILL.md",
                "# unknown",
                Some("unknown"),
            ),
            home_skill(
                "/home/user/.agents/skills/deploy/SKILL.md",
                "---\nname: deploy\ndescription: Deploy things\n---\nbody",
            ),
            home_skill("/repo/.agents/skills/project/SKILL.md", "outside home"),
            home_skill("/home/user/not-a-skill.md", "unknown provider"),
        ],
        // #353's producer serializes global rules unconditionally, but nothing
        // client-side consumes them yet (see this module's doc comment) — kept
        // populated here anyway so `snapshot_decoding_keeps_valid_context_from_
        // each_source` documents that `parse_snapshot` ignores this field rather
        // than erroring on it.
        global_rules: vec![],
    }
}

fn setup_context_models(app: &mut App) {
    app.add_singleton_model(DirectoryWatcher::new);
    app.add_singleton_model(AISettings::new_with_defaults);
    app.add_singleton_model(|_| DetectedRepositories::default());
    app.add_singleton_model(RepoMetadataModel::new);
    app.add_singleton_model(HomeDirectoryWatcher::new_for_test);
    app.add_singleton_model(WarpManagedPathsWatcher::new_for_testing);
    app.add_singleton_model(SkillManager::new);
}

#[test]
fn snapshot_decoding_keeps_valid_context_from_each_source() {
    let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
    let host_id = HostId::new("remote-host".to_string());
    let state = parse_snapshot(&host_id, snapshot());
    let bundled = state.bundled_skills.unwrap();
    let home = state.home_skills.unwrap();

    assert_eq!(bundled.skill("bundled").unwrap().name, "bundled");
    assert!(bundled.skill("unknown-mcp").is_none());
    assert_eq!(home.home_dir, remote_path(&host_id, "/home/user").unwrap());
    assert_eq!(home.skills.len(), 1);
    assert_eq!(home.skills[0].name, "deploy");
    assert_eq!(home.skills[0].provider, SkillProvider::Agents);
    assert_eq!(home.skills[0].scope, SkillScope::Home);
    for path in home.skills.iter().map(|skill| &skill.path) {
        assert_eq!(path.as_remote().unwrap().host_id, host_id);
        assert!(path.starts_with(&home.home_dir));
    }
}

#[test]
fn invalid_home_directory_drops_only_home_context() {
    let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
    let host_id = HostId::new("remote-host".to_string());
    let mut snapshot = snapshot();
    snapshot.home_dir = "relative/home".to_string();

    let state = parse_snapshot(&host_id, snapshot);
    assert!(state.bundled_skills.unwrap().skill("bundled").is_some());
    assert!(state.home_skills.is_none());
}

#[test]
fn reconcile_snapshot_fully_replaces_all_host_context() {
    let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
    let host_id = HostId::new("remote-host".to_string());
    let bundled_path = remote_path(&host_id, "/bundled/skills/bundled/SKILL.md").unwrap();
    let home_path = remote_path(&host_id, "/home/user/.agents/skills/deploy/SKILL.md").unwrap();

    App::test((), |mut app| async move {
        setup_context_models(&mut app);
        let skills = SkillManager::handle(&app);
        let context = app.add_singleton_model(|_| RemoteAgentContext);

        context.update(&mut app, |context, ctx| {
            context.reconcile_snapshot(host_id.clone(), snapshot(), ctx);
        });
        skills.read(&app, |manager, _| {
            assert!(manager.skill_by_path(&bundled_path).is_some());
            assert!(manager.skill_by_path(&home_path).is_some());
        });

        context.update(&mut app, |context, ctx| {
            context.reconcile_snapshot(
                host_id.clone(),
                RemoteAgentContextSnapshot {
                    revision: 8,
                    home_dir: "/home/user".to_string(),
                    skills: Vec::new(),
                    global_rules: Vec::new(),
                },
                ctx,
            );
        });
        skills.read(&app, |manager, _| {
            assert!(manager.skill_by_path(&bundled_path).is_none());
            assert!(manager.skill_by_path(&home_path).is_none());
        });
    });
}

#[test]
fn remove_host_context_clears_only_the_matching_host() {
    let _bundled_skills = FeatureFlag::BundledSkills.override_enabled(true);
    let first_host = HostId::new("first-host".to_string());
    let second_host = HostId::new("second-host".to_string());
    let first_bundled_path = remote_path(&first_host, "/bundled/skills/bundled/SKILL.md").unwrap();
    let second_bundled_path =
        remote_path(&second_host, "/bundled/skills/bundled/SKILL.md").unwrap();

    App::test((), |mut app| async move {
        setup_context_models(&mut app);
        let skills = SkillManager::handle(&app);
        let context = app.add_singleton_model(|_| RemoteAgentContext);

        for host_id in [&first_host, &second_host] {
            context.update(&mut app, |context, ctx| {
                context.reconcile_snapshot(host_id.clone(), snapshot(), ctx);
            });
        }
        context.update(&mut app, |context, ctx| {
            context.remove_host_context(&first_host, ctx);
        });

        skills.read(&app, |manager, _| {
            assert!(manager.skill_by_path(&first_bundled_path).is_none());
            assert!(manager.skill_by_path(&second_bundled_path).is_some());
        });
    });
}
