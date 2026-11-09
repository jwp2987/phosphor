use std::cell::RefCell;
use std::sync::Arc;
use warp_core::SessionId;
use warpui::{AppContext, Entity, ModelContext, ModelHandle, SingletonEntity};

use crate::{
    ai::execution_context::WarpAiExecutionContext,
    terminal::{
        ShellLaunchData,
        model::session::SessionsEvent,
        model_events::{ModelEvent, ModelEventDispatcher},
        shell::ShellType,
    },
};

use super::{Session, SessionType, Sessions};

pub struct ActiveSession {
    model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
    sessions: ModelHandle<Sessions>,

    /// The current working directory of the terminal session.
    current_working_directory: Option<String>,

    /// The most recently successfully resolved execution environment (shell / os).
    ///
    /// `session()` returns `None` when the active session id is missing or the id
    /// doesn't resolve to a session (this can happen briefly during focus switches
    /// or session rebuilds). If we returned `None` directly in that case,
    /// `input_context_for_request` wouldn't push an `ExecutionEnvironment`, and the
    /// system prompt's <env> section would drop the Shell/Platform lines entirely —
    /// the model loses environment info, and a system section that changes turn to
    /// turn busts the prompt cache. We cache the last known value as a fallback.
    last_execution_environment: RefCell<Option<WarpAiExecutionContext>>,
}

impl ActiveSession {
    pub fn new(
        sessions: ModelHandle<Sessions>,
        model_event_dispatcher: ModelHandle<ModelEventDispatcher>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model_event_dispatcher, move |me, event, ctx| {
            // Both the precmd path (`BlockMetadataReceived`) and the OSC 7 path
            // (`BlockWorkingDirectoryUpdated`) carry a fresh cwd for the active
            // block, so both must refresh the session's known directory.
            let block_metadata = match event {
                ModelEvent::BlockMetadataReceived(e) => Some(&e.block_metadata),
                ModelEvent::BlockWorkingDirectoryUpdated(e) => Some(&e.block_metadata),
                _ => None,
            };
            if let Some(block_metadata) = block_metadata {
                // Sticky update: block metadata without a cwd should not clear the
                // known directory. See the comment on
                // `BlocklistAIContextModel::update_directory_context` — if this got
                // cleared here, `list_skills` would silently degrade (see the
                // cwd-based skill-discovery call in `controller/input_context.rs`).
                let new_pwd = block_metadata
                    .current_working_directory()
                    .map(|cwd| cwd.to_owned());
                if new_pwd.is_some() && me.current_working_directory != new_pwd {
                    me.current_working_directory = new_pwd;
                    ctx.emit(ActiveSessionEvent::UpdatedPwd);
                }
            }
        });

        ctx.subscribe_to_model(&sessions, |me, event, ctx| {
            if let SessionsEvent::SessionBootstrapped(bootstrap_event) = event {
                if Some(bootstrap_event.session_id)
                    == me.model_event_dispatcher.as_ref(ctx).active_session_id()
                {
                    ctx.emit(ActiveSessionEvent::Bootstrapped);
                }
            }
        });

        Self {
            sessions,
            model_event_dispatcher,
            current_working_directory: None,
            last_execution_environment: RefCell::new(None),
        }
    }

    /// The active session's id, independent of whether that id currently
    /// resolves to a live [`Session`].
    ///
    /// Prefer this over `session(app).map(|s| s.id())` for anything keyed by
    /// session id. As the struct comment above notes, `session()` also returns
    /// `None` when the id is present but does not resolve — briefly, during
    /// focus switches and session rebuilds. Callers that only need the id would
    /// silently get `None` in those windows.
    ///
    /// That is not hypothetical: the TUI up-arrow history menu was ported using
    /// `session(ctx).map(|s| s.id())` because this accessor did not exist, and
    /// its command history (which is keyed by session id, unlike prompts, which
    /// are not) came back empty whenever the lookup missed.
    pub fn session_id(&self, app: &AppContext) -> Option<SessionId> {
        self.model_event_dispatcher.as_ref(app).active_session_id()
    }

    pub fn session(&self, app: &AppContext) -> Option<Arc<Session>> {
        self.model_event_dispatcher
            .as_ref(app)
            .active_session_id()
            .and_then(|session_id| self.sessions.as_ref(app).get(session_id))
    }

    pub fn session_type(&self, app: &AppContext) -> Option<SessionType> {
        self.session(app).map(|session| session.session_type())
    }

    pub fn shell_type(&self, app: &AppContext) -> Option<ShellType> {
        self.session(app)
            .as_ref()
            .map(|session| session.shell().shell_type())
    }

    pub fn shell_launch_data(&self, app: &AppContext) -> Option<ShellLaunchData> {
        self.session(app)
            .as_ref()
            .and_then(|session| session.launch_data().cloned())
    }

    pub fn current_working_directory(&self) -> Option<&String> {
        self.current_working_directory.as_ref()
    }

    /// Test-only seam for the working directory that production sets from
    /// `BlockMetadataReceived` / `BlockWorkingDirectoryUpdated` events (see the
    /// subscription in `new`). The field is private, so tests in other modules — e.g.
    /// `read_skill_tests.rs`, which needs a session scope for the cache-miss skill
    /// read — cannot reach it the way this file's own tests do.
    #[cfg(test)]
    pub(crate) fn set_current_working_directory_for_test(&mut self, cwd: impl Into<String>) {
        self.current_working_directory = Some(cwd.into());
    }

    /// Returns the current working directory as a [`LocalOrRemotePath`]: `Remote` when the
    /// active session is a connected `WarpifiedRemote` (SSH) session, `Local` otherwise.
    ///
    /// A `WarpifiedRemote` session with no helper but an out-of-band channel (shell mode) is
    /// `Remote` under its `shell_mode_host_id` stand-in.
    ///
    /// Returns `None` for a `WarpifiedRemote` session whose `host_id` hasn't resolved yet
    /// (the remote-server handshake hasn't completed — see [`SessionType::WarpifiedRemote`]'s
    /// doc comment) rather than falling back to treating the cwd as local: a remote cwd string
    /// interpreted as a *local* path can spuriously match an unrelated local directory of the
    /// same name (e.g. `find_rules_with_fast_path` would stat/read local files that have
    /// nothing to do with the remote session). Used by skill/slash-command surfaces that key
    /// off the working directory.
    pub fn current_working_directory_location(
        &self,
        ctx: &AppContext,
    ) -> Option<warp_util::local_or_remote_path::LocalOrRemotePath> {
        let cwd = self.current_working_directory.as_ref()?;
        match self.session(ctx).as_deref().map(Session::session_type) {
            Some(SessionType::WarpifiedRemote {
                host_id: Some(host_id),
            }) => {
                let path = warp_util::standardized_path::StandardizedPath::try_new(cwd).ok()?;
                Some(warp_util::local_or_remote_path::LocalOrRemotePath::Remote(
                    warp_util::remote_path::RemotePath::new(
                        crate::code::buffer_location::core_host_id_to_util(&host_id),
                        path,
                    ),
                ))
            }
            // No helper connection (the primary, no-helper design): when the session has an
            // out-of-band channel its host is the shell-mode stand-in, the id its skills,
            // rules and file tree are filed under. Otherwise the host has not resolved yet.
            Some(SessionType::WarpifiedRemote { host_id: None }) => {
                let session_id = self.session(ctx).as_deref().map(Session::id)?;
                // A missing manager (unit tests) means nothing serves the session.
                let manager = crate::remote_server::manager::RemoteServerManager::try_as_ref(ctx)?;
                if !manager.is_shell_mode_session(session_id) {
                    return None;
                }
                let path = warp_util::standardized_path::StandardizedPath::try_new(cwd).ok()?;
                Some(warp_util::local_or_remote_path::LocalOrRemotePath::Remote(
                    warp_util::remote_path::RemotePath::new(
                        crate::code::buffer_location::core_host_id_to_util(
                            &remote_server::manager::shell_mode_host_id(session_id),
                        ),
                        path,
                    ),
                ))
            }
            Some(SessionType::Local) | None => Some(
                warp_util::local_or_remote_path::LocalOrRemotePath::Local(cwd.into()),
            ),
        }
    }

    /// Returns the `WarpAiExecutionContext` for the active session.
    ///
    /// Falls back to the last known value (see
    /// [`Self::last_execution_environment`]) when active-session resolution fails,
    /// avoiding losing the Shell/Platform lines from the system prompt's <env>
    /// section mid-conversation.
    pub fn ai_execution_environment(&self, app: &AppContext) -> Option<WarpAiExecutionContext> {
        if let Some(session) = self.session(app).as_ref() {
            let env = WarpAiExecutionContext::new(session);
            *self.last_execution_environment.borrow_mut() = Some(env.clone());
            return Some(env);
        }
        self.last_execution_environment.borrow().clone()
    }
}

pub enum ActiveSessionEvent {
    /// The active session's working directory changed.
    UpdatedPwd,
    /// The active session finished bootstrapping.
    Bootstrapped,
}

impl Entity for ActiveSession {
    type Event = ActiveSessionEvent;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terminal::model::session::{BootstrapSessionType, SessionInfo};
    use warp_util::local_or_remote_path::LocalOrRemotePath;
    use warpui::App;

    const REMOTE_CWD: &str = "/work/repo";

    /// Builds a registered session of `session_type` plus the `Sessions` /
    /// `ModelEventDispatcher` / `ActiveSession` trio needed for
    /// `ActiveSession::session` to resolve it, with `current_working_directory`
    /// pre-populated (normally only set via `BlockMetadataReceived`/
    /// `BlockWorkingDirectoryUpdated` events — set directly here via the private
    /// field, since this test module is a descendant of `active_session`).
    /// `resolved_host_id`, when `Some`, calls `Session::set_remote_host_id` after
    /// registration, mirroring what `Sessions`'s own `RemoteServerManager`
    /// subscription does once the remote-server handshake completes.
    fn build_active_session(
        app: &mut App,
        session_type: BootstrapSessionType,
        resolved_host_id: Option<warp_core::HostId>,
    ) -> ModelHandle<ActiveSession> {
        let session_id = SessionId::from(1);
        let sessions = app.add_model(|_| Sessions::new_for_test());
        sessions.update(app, |sessions, _| {
            sessions.register_session_for_test(
                SessionInfo::new_for_test()
                    .with_id(session_id)
                    .with_session_type(session_type),
            );
            if let Some(host_id) = resolved_host_id {
                sessions
                    .get(session_id)
                    .expect("just registered")
                    .set_remote_host_id(Some(host_id));
            }
        });

        let (_events_tx, events_rx) = async_channel::unbounded();
        let model_events =
            app.add_model(|ctx| ModelEventDispatcher::new(events_rx, sessions.clone(), ctx));
        model_events.update(app, |dispatcher, _| {
            dispatcher.set_active_session_id(session_id);
        });

        let active_session = app.add_model(|ctx| ActiveSession::new(sessions, model_events, ctx));
        active_session.update(app, |active_session, _ctx| {
            active_session.current_working_directory = Some(REMOTE_CWD.to_owned());
        });
        active_session
    }

    #[test]
    fn remote_session_with_resolved_host_returns_remote_location() {
        App::test((), |mut app| async move {
            let host_id = warp_core::HostId::new("prod-1".to_owned());
            let active_session = build_active_session(
                &mut app,
                BootstrapSessionType::WarpifiedRemote,
                Some(host_id.clone()),
            );

            let location = active_session
                .read(&app, |active_session, ctx| {
                    active_session.current_working_directory_location(ctx)
                })
                .expect("resolved remote host should produce a location");

            match location {
                LocalOrRemotePath::Remote(remote) => {
                    assert_eq!(
                        remote.host_id,
                        crate::code::buffer_location::core_host_id_to_util(&host_id)
                    );
                    assert_eq!(remote.path.as_str(), REMOTE_CWD);
                }
                LocalOrRemotePath::Local(path) => {
                    panic!("expected a remote location, got local path {path:?}")
                }
            }
        });
    }

    #[test]
    fn remote_session_without_resolved_host_returns_none() {
        // A `WarpifiedRemote` session whose `host_id` hasn't resolved yet (the
        // remote-server handshake hasn't completed) must not be treated as local:
        // the cwd string is a path on the not-yet-identified remote host, not this
        // machine.
        App::test((), |mut app| async move {
            // The location lookup asks the manager whether the session has a shell-mode
            // channel; this one has none (setup only: the assertion is unchanged).
            app.add_singleton_model(remote_server::manager::RemoteServerManager::new);
            let active_session =
                build_active_session(&mut app, BootstrapSessionType::WarpifiedRemote, None);

            let location = active_session.read(&app, |active_session, ctx| {
                active_session.current_working_directory_location(ctx)
            });

            assert!(location.is_none());
        });
    }

    /// A remote session with no helper but an out-of-band channel (shell mode) is `Remote` under
    /// the shell-mode stand-in host, the id its skills, rules and file tree are filed under.
    #[test]
    fn shell_mode_session_returns_the_stand_in_host_location() {
        #[derive(Debug)]
        struct Channel;
        impl remote_server::backend::OutOfBandExecutor for Channel {
            fn channel_kind(&self) -> remote_server::backend::ChannelKind {
                remote_server::backend::ChannelKind::LocalSh
            }
            fn shell_kind(&self) -> remote_server::shellcmd::ShellKind {
                remote_server::shellcmd::ShellKind::Posix
            }
            fn execute(
                &self,
                _request: remote_server::backend::ExecRequest,
            ) -> futures_util::future::BoxFuture<
                '_,
                Result<remote_server::backend::ExecOutput, remote_server::backend::ExecError>,
            > {
                Box::pin(async { Err(remote_server::backend::ExecError::Failed("unused".into())) })
            }
        }
        App::test((), |mut app| async move {
            let manager = app.add_singleton_model(remote_server::manager::RemoteServerManager::new);
            manager.read(&app, |manager, _| {
                manager
                    .router()
                    .register_out_of_band(SessionId::from(1), std::sync::Arc::new(Channel));
            });
            let active_session =
                build_active_session(&mut app, BootstrapSessionType::WarpifiedRemote, None);

            let location = active_session.read(&app, |active_session, ctx| {
                active_session.current_working_directory_location(ctx)
            });

            let Some(LocalOrRemotePath::Remote(remote)) = location else {
                panic!("expected a remote location");
            };
            assert_eq!(
                remote.host_id.as_str(),
                remote_server::manager::shell_mode_host_id(SessionId::from(1)).as_str()
            );
        });
    }

    #[test]
    fn local_session_returns_local_location() {
        App::test((), |mut app| async move {
            let active_session = build_active_session(&mut app, BootstrapSessionType::Local, None);

            let location = active_session.read(&app, |active_session, ctx| {
                active_session.current_working_directory_location(ctx)
            });

            assert_eq!(location, Some(LocalOrRemotePath::Local(REMOTE_CWD.into())));
        });
    }
}
