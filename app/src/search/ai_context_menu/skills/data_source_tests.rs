use std::sync::Arc;

use ai::skills::{ParsedSkill, SkillProvider, SkillScope};
use warp_core::features::FeatureFlag;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warpui::{App, SingletonEntity};

use super::data_source::SkillsDataSource;
use crate::ai::skills::{BundledSkillActivation, SkillManager};
use crate::search::ai_context_menu::mixer::AIContextMenuSearchableAction;
use crate::search::data_source::Query;
use crate::search::mixer::SyncDataSource;
use crate::terminal::model::session::Session;
use crate::test_util::terminal::{
    add_window_with_id_and_terminal, initialize_app_for_terminal_view,
};
use crate::workspace::ActiveSession;

fn local_bundled_skill(name: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} local bundled skill"),
        path: LocalOrRemotePath::Local(format!("/bundled/skills/{name}/SKILL.md").into()),
        content: format!("# {name}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

fn remote_bundled_skill(host_id: &HostId, name: &str) -> ParsedSkill {
    ParsedSkill {
        name: name.to_string(),
        description: format!("{name} remote bundled skill"),
        path: LocalOrRemotePath::Remote(RemotePath::new(
            host_id.clone(),
            warp_util::standardized_path::StandardizedPath::try_new(format!(
                "/remote/skills/{name}/SKILL.md"
            ))
            .unwrap(),
        )),
        content: format!("# {name}"),
        line_range: None,
        provider: SkillProvider::Zap,
        scope: SkillScope::Bundled,
    }
}

fn skill_names(
    results: &[crate::search::data_source::QueryResult<AIContextMenuSearchableAction>],
) -> Vec<String> {
    results
        .iter()
        .map(|result| match result.accept_result() {
            AIContextMenuSearchableAction::InsertSkill { name } => name,
            other => panic!("expected InsertSkill action, got {other:?}"),
        })
        .collect()
}

/// Guards #775 (the `@`-menu Skills list following the tab's execution host): a
/// resolved SSH tab must list the *remote* host's bundled skills, never the local
/// machine's, even though `SkillManager` has both catalogs loaded at once.
#[test]
fn resolved_remote_session_lists_remote_skills_not_local() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _bundled_skills_guard = FeatureFlag::BundledSkills.override_enabled(true);

        let host_id = HostId::new("remote-host".to_string());

        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                "local-only",
                local_bundled_skill("local-only"),
                BundledSkillActivation::Always,
            );
            manager.add_remote_bundled_skill_for_testing(
                host_id.clone(),
                "remote-only",
                remote_bundled_skill(&host_id, "remote-only"),
                BundledSkillActivation::Always,
            );
        });

        let (window_id, terminal_view) = add_window_with_id_and_terminal(&mut app, None);

        let session = Session::test_remote();
        session.set_remote_host_id(Some(warp_core::HostId::new(host_id.as_str().to_string())));

        ActiveSession::handle(&app).update(&mut app, |active_session, ctx| {
            active_session.set_session_for_test(
                window_id,
                Arc::new(session),
                None::<std::path::PathBuf>,
                Some("/repo".to_string()),
                Some(terminal_view.id()),
                ctx,
            );
        });

        let data_source = SkillsDataSource::new();
        let results = app.read(|app| data_source.run_query(&Query::from(""), app).unwrap());
        let names = skill_names(&results);

        assert!(
            names.contains(&"remote-only".to_string()),
            "expected the remote host's bundled skill to be listed; got {names:?}"
        );
        assert!(
            !names.contains(&"local-only".to_string()),
            "the local machine's bundled skill must not leak into an SSH tab's skill \
             menu; got {names:?}"
        );
    });
}

/// An SSH tab whose remote host hasn't resolved yet (handshake in flight) must show
/// no skills at all, rather than falling back to this machine's local catalog.
#[test]
fn unresolved_remote_session_lists_no_skills() {
    App::test((), |mut app| async move {
        initialize_app_for_terminal_view(&mut app);
        let _bundled_skills_guard = FeatureFlag::BundledSkills.override_enabled(true);

        SkillManager::handle(&app).update(&mut app, |manager, _ctx| {
            manager.add_bundled_skill_for_testing(
                "local-only",
                local_bundled_skill("local-only"),
                BundledSkillActivation::Always,
            );
        });

        let (window_id, terminal_view) = add_window_with_id_and_terminal(&mut app, None);

        // `Session::test_remote()` starts as `WarpifiedRemote { host_id: None }` --
        // the remote-server handshake hasn't completed.
        let session = Session::test_remote();

        ActiveSession::handle(&app).update(&mut app, |active_session, ctx| {
            active_session.set_session_for_test(
                window_id,
                Arc::new(session),
                None::<std::path::PathBuf>,
                Some("/repo".to_string()),
                Some(terminal_view.id()),
                ctx,
            );
        });

        let data_source = SkillsDataSource::new();
        let results = app.read(|app| data_source.run_query(&Query::from(""), app).unwrap());
        let names = skill_names(&results);

        assert!(
            names.is_empty(),
            "an unresolved-remote tab must list no skills rather than falling back \
             to the local catalog; got {names:?}"
        );
    });
}
