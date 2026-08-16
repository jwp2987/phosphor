//! [`TuiSessions`]: registry and foreground selection for live TUI sessions.
//!
//! Each session retains a terminal view with its manager. The container owns
//! session lifetime and focus; the root view renders and routes input only to
//! the focused session.

#![allow(dead_code)]
// Staged port: this module came across from the pinned oracle (see `8c6d3a4c
// feat(tui): stage warp_tui crate ... (phase 0)` and the `port(tui)` commits) with
// the upstream API surface intact, but only the paths the TUI actually drives are
// wired up yet. The unused items here are upstream's, not ours.
//
// Kept rather than pruned because this fork re-pins against upstream roughly
// weekly (`ORACLE.md`); deleting upstream's helpers would turn each one into a
// re-pin conflict for no gain. Drop this attribute once the module is fully wired
// and check what is genuinely dead then.

use std::collections::HashMap;
use std::ffi::OsString;
use std::path::PathBuf;

use pathfinder_geometry::vector::Vector2F;
use warp::tui_export::{
    AIConversation, AIConversationAutoexecuteMode, AIConversationId, BannerState,
    BlocklistAIHistoryModel, GlobalResourceHandlesProvider, LocalTtyTerminalManager,
    ServerConversationToken, TerminalManagerTrait, TerminalSurfaceResult,
};
use warpui::SingletonEntity;
use warpui_core::runtime::TuiDriverHandle;
use warpui_core::{AppContext, Entity, EntityId, ModelContext, ModelHandle, ViewHandle, WindowId};

use crate::orchestration_model::{TuiOrchestrationEvent, TuiOrchestrationModel};
use crate::resume::TuiExitSummaryHandle;
use crate::terminal_session_view::TuiTerminalSessionView;

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

    pub(crate) fn refresh_orchestration_tab_state(&self, ctx: &mut AppContext) {
        match self {
            Self::Terminal(view) => {
                view.update(ctx, |view, ctx| view.refresh_orchestration_tab_state(ctx));
            }
        }
    }

    pub(crate) fn set_orchestration_tab_focus(&self, focused: bool, ctx: &mut AppContext) {
        match self {
            Self::Terminal(view) => {
                view.update(ctx, |view, ctx| {
                    view.set_orchestration_tab_focus(focused, ctx);
                });
            }
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
    /// drop, so the app-lifetime session singleton must retain it, and it
    /// carries the live repaint-scheduling switches.
    driver: Option<TuiDriverHandle>,
    keyboard_enhancement_supported: bool,
    exit_summary: TuiExitSummaryHandle,
    sessions: Vec<TuiSession>,
    focused_session_id: Option<TuiSessionId>,
    resume_token: Option<ServerConversationToken>,
    /// Launch-wide autoexecute default (`--auto-approve`), handed to every
    /// session this container creates.
    default_autoexecute_mode: AIConversationAutoexecuteMode,
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
        Self::create_local_terminal_session_with_env(
            sessions,
            window_id,
            focus,
            startup_directory,
            HashMap::new(),
            ctx,
        )
    }

    /// Creates and registers a full local terminal session, merging
    /// `extra_env_vars` on top of the process's own environment. Used by
    /// [`crate::pane_group::TuiPaneGroup`] to hand a hidden child-agent
    /// session the `OZ_RUN_ID`/`OZ_PARENT_RUN_ID`/model env vars
    /// `local_harness_launch` prepares -- mirroring how the GUI's
    /// `insert_terminal_pane_hidden_for_child_agent` seeds a new pane's PTY
    /// environment before its shell starts (env vars set after the shell is
    /// already running are invisible to the process the user's typed command
    /// launches).
    pub(crate) fn create_local_terminal_session_with_env(
        sessions: &ModelHandle<Self>,
        window_id: WindowId,
        focus: bool,
        startup_directory: Option<PathBuf>,
        extra_env_vars: HashMap<OsString, OsString>,
        ctx: &mut AppContext,
    ) -> (TuiSessionId, ViewHandle<TuiTerminalSessionView>) {
        let (
            exit_summary,
            keyboard_enhancement_supported,
            is_first_session,
            default_autoexecute_mode,
        ) = sessions.read(ctx, |sessions, _| {
            (
                sessions.exit_summary.clone(),
                sessions.keyboard_enhancement_supported,
                sessions.is_empty(),
                sessions.default_autoexecute_mode,
            )
        });
        // Only the first session reports the startup settings-file failure;
        // later sessions would repeat a hint about an event the user has
        // already been told about.
        let initial_settings_file_error = is_first_session
            .then(|| {
                GlobalResourceHandlesProvider::as_ref(ctx)
                    .get()
                    .settings_file_error
                    .clone()
            })
            .flatten();
        let mut env_vars = HashMap::<OsString, OsString>::from_iter(std::env::vars_os());
        env_vars.extend(extra_env_vars);
        // The manager uses this internal model for unsupported-shell state; the
        // TUI does not render a separate banner surface.
        let banner = ctx.add_model(|_| BannerState::default());
        let manager = LocalTtyTerminalManager::<TuiTerminalSessionView>::create_tui_model(
            startup_directory,
            env_vars,
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
                        initial_settings_file_error,
                        default_autoexecute_mode,
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

    /// Creates an unfocused local terminal session for a restored child and
    /// restores its persisted transcript onto it, without relaunching the child
    /// or resending its prompt. Ported from `40ac1d4b1`.
    pub(crate) fn create_restored_local_child_session(
        sessions: &ModelHandle<Self>,
        window_id: WindowId,
        startup_directory: Option<PathBuf>,
        conversation: AIConversation,
        ctx: &mut AppContext,
    ) -> (TuiSessionId, ViewHandle<TuiTerminalSessionView>) {
        let (session_id, surface) =
            Self::create_local_terminal_session(sessions, window_id, false, startup_directory, ctx);
        surface.update(ctx, |view, ctx| {
            view.restore_orchestrated_child_conversation(conversation, ctx);
        });
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

    /// Subscribes the session owner to orchestration lifecycle requests: the
    /// focused session's tab bar refreshes when the orchestration model
    /// notifies (topology changed) or when focus moves to a different
    /// session; restored child sessions are materialized on request, and
    /// child-session kill and deferred teardown are drained here -- see
    /// [`TuiOrchestrationModel`]'s module doc for why this is still a trimmed
    /// subset of the pin's `wire_orchestration`. What remains absent is
    /// *launch* materialization: nothing in this fork emits a request to
    /// create a child from scratch through the executor, and `/orchestrate`
    /// children are materialized directly by
    /// [`crate::pane_group::TuiPaneGroup`].
    pub(crate) fn wire_orchestration(
        sessions: &ModelHandle<Self>,
        orchestration: &ModelHandle<TuiOrchestrationModel>,
        ctx: &mut AppContext,
    ) {
        // Keep the model's child-session bookkeeping in step with the
        // registry: a session removed for any reason must stop being
        // projected into the tab bar.
        let orchestration_for_removals = orchestration.clone();
        ctx.subscribe_to_model(sessions, move |_, event, ctx| {
            let TuiSessionsEvent::SessionRemoved(session_id) = event else {
                return;
            };
            let session_id = *session_id;
            orchestration_for_removals.update(ctx, |orchestration, ctx| {
                orchestration.handle_session_removed(session_id, ctx);
            });
        });

        let sessions_for_events = sessions.clone();
        let orchestration_for_events = orchestration.clone();
        ctx.subscribe_to_model(orchestration, move |_, event, ctx| match event {
            TuiOrchestrationEvent::RestoreLocalChildSession {
                root_session_id,
                conversation,
            } => {
                let Some(window_id) = sessions_for_events
                    .as_ref(ctx)
                    .session(*root_session_id)
                    .map(|session| session.view().window_id(ctx))
                else {
                    return;
                };
                let conversation_id = conversation.id();
                let startup_directory = conversation
                    .current_working_directory()
                    .or_else(|| conversation.initial_working_directory())
                    .map(PathBuf::from);
                let (session_id, _session_view) = Self::create_restored_local_child_session(
                    &sessions_for_events,
                    window_id,
                    startup_directory,
                    (**conversation).clone(),
                    ctx,
                );
                orchestration_for_events.update(ctx, |orchestration, ctx| {
                    orchestration.register_restored_local_oz_child_session(
                        session_id,
                        conversation_id,
                        ctx,
                    );
                });
            }
            TuiOrchestrationEvent::KillLocalChildSession {
                session_id,
                conversation_id,
            } => {
                let child_view = sessions_for_events
                    .as_ref(ctx)
                    .session(*session_id)
                    .map(|session| session.view().clone());
                if let Some(TuiSessionView::Terminal(view)) = child_view {
                    view.update(ctx, |view, ctx| {
                        view.cancel_active_conversation(ctx);
                    });
                }
                orchestration_for_events.update(ctx, |orchestration, ctx| {
                    orchestration.cleanup_child(conversation_id, ctx);
                });
            }
            TuiOrchestrationEvent::RemoveChildSession(session_id) => {
                let session_id = *session_id;
                sessions_for_events.update(ctx, |sessions, ctx| {
                    sessions.remove_session(session_id, ctx);
                });
            }
        });

        let sessions_for_model_updates = sessions.clone();
        ctx.observe_model(orchestration, move |_, ctx| {
            let focused_view = sessions_for_model_updates
                .as_ref(ctx)
                .focused_session()
                .map(|session| session.view().clone());
            if let Some(focused_view) = focused_view {
                focused_view.refresh_orchestration_tab_state(ctx);
            }
        });

        let sessions_for_focus_updates = sessions.clone();
        ctx.subscribe_to_model(sessions, move |_, event, ctx| {
            let TuiSessionsEvent::FocusChanged(session_id) = event else {
                return;
            };
            let focused_view = sessions_for_focus_updates
                .as_ref(ctx)
                .session(*session_id)
                .map(|session| session.view().clone());
            if let Some(focused_view) = focused_view {
                focused_view.refresh_orchestration_tab_state(ctx);
            }
        });
    }

    /// Creates the app's session container.
    pub(crate) fn new(
        driver: TuiDriverHandle,
        exit_summary: TuiExitSummaryHandle,
        resume_token: Option<ServerConversationToken>,
        default_autoexecute_mode: AIConversationAutoexecuteMode,
    ) -> Self {
        let keyboard_enhancement_supported = driver.keyboard_enhancement_supported();
        Self {
            driver: Some(driver),
            keyboard_enhancement_supported,
            exit_summary,
            sessions: Vec::new(),
            focused_session_id: None,
            resume_token,
            default_autoexecute_mode,
        }
    }

    /// Creates a driverless container for unit tests.
    #[cfg(test)]
    pub(crate) fn new_for_test() -> Self {
        Self {
            driver: None,
            keyboard_enhancement_supported: false,
            exit_summary: TuiExitSummaryHandle::default(),
            sessions: Vec::new(),
            focused_session_id: None,
            resume_token: None,
            default_autoexecute_mode: AIConversationAutoexecuteMode::RespectUserSettings,
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

    /// Applies orchestration tab focus to a specific registered session's
    /// view (used when switching tab focus to a session other than the
    /// caller's own).
    pub(crate) fn set_orchestration_tab_focus(
        session_id: TuiSessionId,
        focused: bool,
        ctx: &mut AppContext,
    ) {
        let view = Self::as_ref(ctx)
            .session(session_id)
            .map(|session| session.view.clone());
        if let Some(view) = view {
            view.set_orchestration_tab_focus(focused, ctx);
        }
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
    /// Applies a live change to `appearance.zero_state.freeze_animation_when_unfocused`.
    pub(crate) fn set_freeze_repaints_when_unfocused(&mut self, freeze: bool) {
        if let Some(driver) = self.driver.as_mut() {
            driver.set_freeze_repaints_when_unfocused(freeze);
        }
    }

    pub(crate) fn take_resume_token(&mut self) -> Option<ServerConversationToken> {
        self.resume_token.take()
    }
}

#[cfg(test)]
#[path = "session_registry_tests.rs"]
mod tests;
