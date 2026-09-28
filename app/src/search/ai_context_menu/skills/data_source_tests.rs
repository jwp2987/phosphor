use std::path::PathBuf;
use std::sync::Arc;

use ai::skills::{ParsedSkill, SkillProvider, SkillScope};
use warp_core::features::FeatureFlag;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warpui::windowing::WindowManager;
use warpui::{App, SingletonEntity};

use super::SkillsDataSource;
use crate::ai::skills::{BundledSkillActivation, SkillManager};
use crate::search::ai_context_menu::mixer::AIContextMenuSearchableAction;
use crate::search::data_source::Query;
use crate::search::mixer::SyncDataSource;
use crate::terminal::model::session::command_executor::testing::TestCommandExecutor;
use crate::terminal::model::session::{BootstrapSessionType, Session, SessionInfo};
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
            warp_util::standardized_path::StandardizedPath::try_new(&format!(
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
        // `run_query` reads the *active* window (`app.windows().state().active_window`)
        // to find which tab's session to resolve a host from; a freshly-added test
        // window is not automatically the active one (no platform focus event ever
        // fires in `App::test`), so without this the lookup short-circuits to `None`
        // and silently falls back to local skills regardless of the session below.
        WindowManager::handle(&app).update(&mut app, |windowing_state, _ctx| {
            windowing_state.overwrite_for_test(windowing_state.stage(), Some(window_id));
        });

        let session = Session::test_remote();
        session.set_remote_host_id(Some(warp_core::HostId::new(host_id.as_str().to_string())));
        // `ActiveSession` only keeps a `Weak<Session>` (so it doesn't keep a closed
        // session alive) -- in production the real session registry holds the strong
        // reference, but a test has to hold one itself or the session is dropped
        // (and every `Weak::upgrade()` in `current_working_directory_location` fails)
        // before `run_query` below ever reads it. See `notebooks/link_tests.rs`'s
        // `TEST_SESSION` for the same requirement.
        let session = Arc::new(session);

        ActiveSession::handle(&app).update(&mut app, |active_session, ctx| {
            active_session.set_session_for_test(
                window_id,
                session.clone(),
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

/// Regression guard (#775 follow-up): a legacy SSH session -- the user typed
/// plain `ssh host` into a local PTY, with no remote-server extension involved --
/// has a `host_id` that is *permanently* `None` (`IsLegacySSHSession::Yes`), not
/// merely unresolved during a transient handshake. There is no reliable way to
/// tell that state apart from a genuinely mid-handshake `WarpifiedRemote` session
/// at this layer, so this must fall back to the previous, pre-host-aware
/// behavior (this machine's local + bundled skills) rather than hide the menu's
/// skills entirely -- which would otherwise permanently empty every legacy SSH
/// tab's `@`-menu.
#[test]
fn legacy_ssh_session_lists_local_skills() {
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
        // See the comment above the equivalent call in
        // `resolved_remote_session_lists_remote_skills_not_local`: a freshly-added
        // test window isn't the active window until this is set explicitly.
        WindowManager::handle(&app).update(&mut app, |windowing_state, _ctx| {
            windowing_state.overwrite_for_test(windowing_state.stage(), Some(window_id));
        });

        let session = Session::new(
            SessionInfo::new_for_test()
                .with_session_type(BootstrapSessionType::WarpifiedRemote)
                .with_hostname("prod".to_string())
                .with_ssh_socket_path(PathBuf::from("~/.ssh/12345")),
            Arc::new(TestCommandExecutor::default()),
        );
        // See the comment above the equivalent binding in
        // `resolved_remote_session_lists_remote_skills_not_local`: `ActiveSession` only
        // holds a `Weak<Session>`, so this strong reference must outlive the
        // `run_query` call below or `current_working_directory_location`'s
        // `Weak::upgrade()` fails and the session is silently treated as absent.
        let session = Arc::new(session);

        ActiveSession::handle(&app).update(&mut app, |active_session, ctx| {
            active_session.set_session_for_test(
                window_id,
                session.clone(),
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
            names.contains(&"local-only".to_string()),
            "a legacy SSH tab (host_id permanently None) must still list this \
             machine's local/bundled skills rather than showing an empty menu; \
             got {names:?}"
        );
    });
}
