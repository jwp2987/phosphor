use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Weak},
};

use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;
use warpui::{Entity, EntityId, ModelContext, SingletonEntity, WindowId};

use crate::code::buffer_location::core_host_id_to_util;
use crate::terminal::model::session::{Session, SessionType};

/// The active terminal session in each window. The active session of a window is the current
/// session of the most-recently-focused terminal pane of the active tab of the window's workspace.
///
/// #### When to use `ActiveSession`
/// Generally, if a more specific session is available, it should be preferred. For example, when
/// opening a Markdown file from a file link in a block's output, that block's session should be
/// the basis. However, sometimes there is no contextual session (such as when opening a file
/// in Zap from Finder, or when starting from a cloud object). In that case, the `ActiveSession`
/// might be used, but it's often still better to be context-independent.
#[derive(Default)]
pub struct ActiveSession {
    window_sessions: HashMap<WindowId, WindowActiveSession>,
}

/// Active session information for an individual window.
#[derive(Default)]
struct WindowActiveSession {
    /// The [`Session`] model for the active session. This is a weak reference so that it doesn't
    /// prevent cleaning up the session when it closes, in case no other session is activated.
    session: Option<Weak<Session>>,
    /// The active session's working directory, if it's local.
    path_if_local: Option<PathBuf>,
    /// The active session's raw working directory string, regardless of whether the
    /// session is local or remote. Paired with `session`'s `SessionType` in
    /// [`ActiveSession::current_working_directory_location`] to build a host-aware
    /// [`LocalOrRemotePath`] for consumers (e.g. skill-listing menus) that must not
    /// silently treat a remote cwd as local.
    pwd: Option<String>,
    /// The [`EntityId`]` for the [`TerminalView`] for the active session, if there is one.
    terminal_view_id: Option<EntityId>,
}

impl ActiveSession {
    /// The workspace's active session, if there is one.
    pub fn session(&self, window_id: WindowId) -> Option<Arc<Session>> {
        self.window_sessions
            .get(&window_id)?
            .session
            .as_ref()?
            .upgrade()
    }

    pub fn terminal_view_id(&self, window_id: WindowId) -> Option<EntityId> {
        self.window_sessions.get(&window_id)?.terminal_view_id
    }

    /// The current working directory of the active session, if it's local.
    pub fn path_if_local(&self, window_id: WindowId) -> Option<&Path> {
        self.window_sessions
            .get(&window_id)?
            .path_if_local
            .as_deref()
    }

    /// The active session's working directory as a [`LocalOrRemotePath`]: `Remote` for a
    /// connected `WarpifiedRemote` (SSH) session, `Local` otherwise.
    ///
    /// Unlike [`Self::path_if_local`], this does not collapse a remote session to `None` —
    /// it distinguishes "no known cwd yet" (`None`) from "cwd known and it's on host X"
    /// (`Some(Remote(..))`), the same distinction
    /// `terminal::model::session::active_session::ActiveSession::current_working_directory_location`
    /// makes for the AI agent's own skills/rules lookups. Returns `None` for a
    /// `WarpifiedRemote` session whose `host_id` hasn't resolved yet — see
    /// [`Self::is_unresolved_remote_session`], which callers must check before treating a
    /// `None` here as "local with unknown cwd".
    pub fn current_working_directory_location(
        &self,
        window_id: WindowId,
    ) -> Option<LocalOrRemotePath> {
        let window_state = self.window_sessions.get(&window_id)?;
        let session = window_state.session.as_ref()?.upgrade()?;
        let pwd = window_state.pwd.as_ref()?;
        match session.session_type() {
            SessionType::WarpifiedRemote {
                host_id: Some(host_id),
            } => {
                let path = StandardizedPath::try_new(pwd).ok()?;
                Some(LocalOrRemotePath::Remote(RemotePath::new(
                    core_host_id_to_util(&host_id),
                    path,
                )))
            }
            SessionType::WarpifiedRemote { host_id: None } => None,
            SessionType::Local => Some(LocalOrRemotePath::Local(pwd.into())),
        }
    }

    /// Whether the active session is a `WarpifiedRemote` (SSH) session whose `host_id`
    /// hasn't resolved yet. See [`Self::current_working_directory_location`]'s doc comment:
    /// callers must skip host-aware lookups (list nothing) rather than treat this session
    /// as local just because its location isn't resolved yet.
    pub fn is_unresolved_remote_session(&self, window_id: WindowId) -> bool {
        self.window_sessions
            .get(&window_id)
            .and_then(|state| state.session.as_ref())
            .and_then(Weak::upgrade)
            .is_some_and(|session| {
                matches!(
                    session.session_type(),
                    SessionType::WarpifiedRemote { host_id: None }
                )
            })
    }

    /// Set the current session, for use in tests.
    #[cfg(test)]
    pub fn set_session_for_test(
        &mut self,
        window_id: WindowId,
        session: Arc<Session>,
        path_if_local: Option<impl Into<PathBuf>>,
        pwd: Option<impl Into<String>>,
        terminal_view_id: Option<EntityId>,
        ctx: &mut ModelContext<Self>,
    ) {
        self.set_session_state(
            window_id,
            Some(session),
            path_if_local.map(Into::into),
            pwd.map(Into::into),
            terminal_view_id,
            ctx,
        );
    }

    pub(super) fn set_session_state(
        &mut self,
        window_id: WindowId,
        session: Option<Arc<Session>>,
        path_if_local: Option<PathBuf>,
        pwd: Option<String>,
        terminal_view_id: Option<EntityId>,
        ctx: &mut ModelContext<Self>,
    ) {
        let window_state = self.window_sessions.entry(window_id).or_default();

        let session = session.map(|session| Arc::downgrade(&session));
        if window_state.session.is_some() != session.is_some() {
            window_state.session = session;
            ctx.notify();
        } else { match window_state.session.as_ref().zip(session)
        { Some((prev_session, next_session)) => {
            // Session IDs can't necessarily be compared across terminal panes, so check if the backing
            // allocation is the same. We can do this because each `Session` is a singleton.
            if !Weak::ptr_eq(prev_session, &next_session) {
                window_state.session = Some(next_session);
                ctx.notify();
            }
        } _ => {}}}

        if window_state.path_if_local != path_if_local {
            window_state.path_if_local = path_if_local;
            ctx.notify();
        }

        if window_state.pwd != pwd {
            window_state.pwd = pwd;
            ctx.notify();
        }

        if window_state.terminal_view_id != terminal_view_id {
            window_state.terminal_view_id = terminal_view_id;
            ctx.notify();
        }
    }

    pub(super) fn close_workspace(&mut self, window_id: WindowId) {
        self.window_sessions.remove(&window_id);
    }
}

impl Entity for ActiveSession {
    type Event = ();
}

impl SingletonEntity for ActiveSession {}
