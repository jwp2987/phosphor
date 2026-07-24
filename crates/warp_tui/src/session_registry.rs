//! [`TuiSessions`]: registry and foreground selection for live TUI sessions.
//!
//! Each session retains a terminal view with its manager. The container owns
//! session lifetime and focus; the root view renders and routes input only to
//! the focused session.
use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

use pathfinder_geometry::vector::Vector2F;
use warp::tui_export::{
    AIConversationId, BannerState, BlocklistAIHistoryModel, IsSharedSessionCreator,
    LocalTtyTerminalManager, ServerConversationToken, TerminalManagerTrait, TerminalSurfaceResult,
};
use warpui::SingletonEntity;
use warpui_core::runtime::TuiDriverHandle;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle, ViewHandle, WindowId};

use crate::resume::TuiExitSummaryHandle;
use crate::terminal_session_view::TuiTerminalSessionView;
use crate::transcript_view::TRANSCRIPT_BLOCK_SPACING;

/// Identifies a TUI terminal session.
///
/// A session and its eagerly-created view have the same lifetime, so the
/// view's entity id is also the terminal surface id used by shared AI models.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct TuiSessionId(EntityId);

impl TuiSessionId {
    /// The raw entity id used at shared-model boundaries.
    pub(crate) fn surface_id(self) -> EntityId {
        self.0
    }
}

/// A retained view hosted by the TUI session registry.
#[derive(Clone)]
pub(crate) enum TuiSessionView {
    Terminal(ViewHandle<TuiTerminalSessionView>),
}

impl TuiSessionView {
    pub(crate) fn id(&self) -> EntityId {
        match self {
            Self::Terminal(view) => view.id(),
        }
    }

    pub(crate) fn window_id(&self, ctx: &AppContext) -> WindowId {
        match self {
            Self::Terminal(view) => view.window_id(ctx),
        }
    }

    pub(crate) fn activate(&self, ctx: &mut AppContext) {
        match self {
            Self::Terminal(view) => view.update(ctx, |view, ctx| view.activate(ctx)),
        }
    }
}

/// A live TUI session and any resources required to retain it.
pub(crate) struct TuiSession {
    id: TuiSessionId,
    view: TuiSessionView,
    /// Present for terminal sessions to keep their PTY and event loop alive.
    _manager: Option<ModelHandle<Box<dyn TerminalManagerTrait>>>,
}

impl TuiSession {
    pub(crate) fn view(&self) -> &TuiSessionView {
        &self.view
    }
}

/// Events emitted as the session set or focus changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TuiSessionsEvent {
    /// A session was removed from the container.
    SessionRemoved(TuiSessionId),
    /// The focused session changed to this id.
    FocusChanged(TuiSessionId),
}

/// Owns all live TUI sessions and the focused-session selection.
pub(crate) struct TuiSessions {
    /// TUI-specific process driver. Its handle restores terminal mode on
    /// drop, so the app-lifetime session singleton must retain it.
    _driver: Option<TuiDriverHandle>,
    keyboard_enhancement_supported: bool,
    exit_summary: TuiExitSummaryHandle,
    sessions: Vec<TuiSession>,
    focused_session_id: Option<TuiSessionId>,
    resume_token: Option<ServerConversationToken>,
}

impl Entity for TuiSessions {
    type Event = TuiSessionsEvent;
}

impl SingletonEntity for TuiSessions {}

impl TuiSessions {
    /// Creates and registers a full local terminal session.
    pub(crate) fn create_local_terminal_session(
        sessions: &ModelHandle<Self>,
        window_id: WindowId,
        focus: bool,
        startup_directory: Option<PathBuf>,
        ctx: &mut AppContext,
    ) -> (TuiSessionId, ViewHandle<TuiTerminalSessionView>) {
        let (exit_summary, keyboard_enhancement_supported) = sessions.read(ctx, |sessions, _| {
            (
                sessions.exit_summary.clone(),
                sessions.keyboard_enhancement_supported,
            )
        });
        // The manager uses this internal model for unsupported-shell state; the
        // TUI does not render a separate banner surface.
        let banner = ctx.add_model(|_| BannerState::default());
        let manager = LocalTtyTerminalManager::<TuiTerminalSessionView>::create_tui_model(
            startup_directory,
            HashMap::<OsString, OsString>::from_iter(std::env::vars_os()),
            // Zap's create_tui_model has no shared-session-creator or block-spacing params.
            None,
            banner.clone(),
            Vector2F::new(120., 24.),
            None,
            None,
            ctx,
            move |surface_init, ctx| {
                let surface = ctx.add_typed_action_tui_view(window_id, |ctx| {
                    TuiTerminalSessionView::new(
                        surface_init,
                        exit_summary,
                        keyboard_enhancement_supported,
                        ctx,
                    )
                });
                TerminalSurfaceResult {
                    surface,
                    post_wire: move |_manager: &mut LocalTtyTerminalManager<
                        TuiTerminalSessionView,
                    >,
                                     _surface: &ViewHandle<TuiTerminalSessionView>,
                                     _ctx: &mut AppContext| {},
                }
            },
        );

        let surface = manager.surface.clone();
        let session_id =
            Self::register_session(sessions, manager.surface, manager.manager, focus, ctx);
        (session_id, surface)
    }

    /// Registers a terminal session view with the container.
    pub(crate) fn register_session(
        sessions: &ModelHandle<Self>,
        view: ViewHandle<TuiTerminalSessionView>,
        manager: ModelHandle<Box<dyn TerminalManagerTrait>>,
        focus: bool,
        ctx: &mut AppContext,
    ) -> TuiSessionId {
        let id = TuiSessionId(view.id());
        sessions.update(ctx, |sessions, ctx| {
            debug_assert!(
                sessions.session(id).is_none(),
                "a session must not be registered twice"
            );
            sessions.sessions.push(TuiSession {
                id,
                view: TuiSessionView::Terminal(view),
                _manager: Some(manager),
            });
            if focus {
                sessions.focus_session(id, ctx);
            }
            ctx.notify();
            id
        })
    }

    /// Creates the app's session container.
    pub(crate) fn new(
        driver: TuiDriverHandle,
        exit_summary: TuiExitSummaryHandle,
        resume_token: Option<ServerConversationToken>,
    ) -> Self {
        let keyboard_enhancement_supported = driver.keyboard_enhancement_supported();
        Self {
            _driver: Some(driver),
            keyboard_enhancement_supported,
            exit_summary,
            sessions: Vec::new(),
            focused_session_id: None,
            resume_token,
        }
    }

    /// Creates a driverless container for unit tests.
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        Self {
            _driver: None,
            keyboard_enhancement_supported: false,
            exit_summary: TuiExitSummaryHandle::default(),
            sessions: Vec::new(),
            focused_session_id: None,
            resume_token: None,
        }
    }

    /// Removes a session. When the focused session is removed, focus falls
    /// back to the most recently added remaining session, if any.
    pub(crate) fn remove_session(&mut self, id: TuiSessionId, ctx: &mut ModelContext<Self>) {
        let before = self.sessions.len();
        self.sessions.retain(|session| session.id != id);
        if self.sessions.len() == before {
            return;
        }
        ctx.emit(TuiSessionsEvent::SessionRemoved(id));
        if self.focused_session_id == Some(id) {
            self.focused_session_id = None;
            if let Some(fallback) = self.sessions.last().map(|session| session.id) {
                self.focus_session(fallback, ctx);
            }
        }
        ctx.notify();
    }

    /// Removes every retained session without focusing an intermediate fallback.
    pub(crate) fn clear(&mut self, ctx: &mut ModelContext<Self>) {
        let removed_ids = self
            .sessions
            .iter()
            .map(|session| session.id)
            .collect::<Vec<_>>();
        self.focused_session_id = None;
        self.sessions.clear();
        for id in removed_ids {
            ctx.emit(TuiSessionsEvent::SessionRemoved(id));
        }
        ctx.notify();
    }
    /// Focuses a registered session. Returns whether focus changed.
    pub(crate) fn focus_session(&mut self, id: TuiSessionId, ctx: &mut ModelContext<Self>) -> bool {
        if self.focused_session_id == Some(id) || self.session(id).is_none() {
            return false;
        }
        self.focused_session_id = Some(id);
        let view = self
            .session(id)
            .expect("focused session was validated above")
            .view
            .clone();
        view.activate(ctx);
        ctx.emit(TuiSessionsEvent::FocusChanged(id));
        ctx.notify();
        true
    }

    /// The focused session's id.
    pub(crate) fn focused_session_id(&self) -> Option<TuiSessionId> {
        self.focused_session_id
    }

    /// The focused session.
    pub(crate) fn focused_session(&self) -> Option<&TuiSession> {
        self.focused_session_id.and_then(|id| self.session(id))
    }

    /// Looks up a registered session.
    pub(crate) fn session(&self, id: TuiSessionId) -> Option<&TuiSession> {
        self.sessions.iter().find(|session| session.id == id)
    }

    /// Looks up a retained session by its terminal surface id.
    pub(crate) fn session_id_for_surface(&self, surface_id: EntityId) -> Option<TuiSessionId> {
        self.sessions
            .iter()
            .find_map(|session| (session.id.surface_id() == surface_id).then_some(session.id))
    }

    /// Builds the loaded conversation-to-session index used by one topology snapshot.
    pub(crate) fn session_ids_by_conversation(
        &self,
        history: &BlocklistAIHistoryModel,
    ) -> HashMap<AIConversationId, TuiSessionId> {
        self.sessions
            .iter()
            .flat_map(|session| {
                history
                    .all_live_conversations_for_terminal_view(session.id.surface_id())
                    .map(move |conversation| (conversation.id(), session.id))
            })
            .collect()
    }

    /// Whether no session has been registered.
    pub(crate) fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.sessions.len()
    }

    /// Consumes the startup resume token.
    pub(crate) fn take_resume_token(&mut self) -> Option<ServerConversationToken> {
        self.resume_token.take()
    }
}

#[cfg(test)]
#[path = "session_registry_tests.rs"]
mod tests;
