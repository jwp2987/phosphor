//! Shared fixtures for `warp_tui` unit tests.
use std::any::Any;
use std::sync::Arc;

use parking_lot::FairMutex;
use warp::settings::SettingsFileError;
use warp::tui_export::{
    ActiveSession, AgentViewState, Appearance, BlocklistAIActionModel, BlocklistAIHistoryModel,
    ConversationSelection, ConversationSelectionHandle, IgnoredSuggestionsModel,
    ModelEventDispatcher, Sessions, TerminalManagerTrait, TerminalModel, TerminalSurfaceInit,
};
use warp_core::execution_mode::{AppExecutionMode, ExecutionMode};
use warp_core::semantic_selection::SemanticSelection;
use warpui::{AddSingletonModel, App, EntityId, ModelHandle};
use warpui_core::elements::tui::{TuiElement, TuiText};
use warpui_core::{AppContext, Entity, TuiView, TypedActionView, ViewHandle, WindowId};

use crate::conversation_selection::TuiConversationSelection;
use crate::resume::TuiExitSummaryHandle;
use crate::terminal_session_view::TuiTerminalSessionView;

struct TestTerminalManager(Arc<FairMutex<TerminalModel>>);

impl TerminalManagerTrait for TestTerminalManager {
    fn model(&self) -> Arc<FairMutex<TerminalModel>> {
        self.0.clone()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Pumps the foreground test executor so that work spawned via `ctx.spawn`
/// runs to completion before the calling test makes assertions.
///
/// `queue_tui_permission_action` (and the wider action pipeline) enqueue action
/// preprocessing through `ctx.spawn`, which only runs when the single-threaded
/// test executor is ticked. Synchronous test bodies never reach an `.await`
/// point on their own, so without this the queued action never lands in the
/// model's pending queue, leaving the permission prompt inactive (no focus, no
/// pending action) or — worse — deadlocking a test that then `.await`s a result
/// that can never arrive. A handful of yields covers multi-stage spawn chains
/// (preprocess future -> relay callback -> effect flush); extra yields are a
/// no-op once the executor is parked.
pub(crate) async fn settle() {
    for _ in 0..8 {
        futures_lite::future::yield_now().await;
    }
}

/// Pumps the foreground executor and relinquishes the OS thread until `done`
/// holds (or a generous bound elapses, which panics).
///
/// Prefer this over a fixed number of [`settle`] yields whenever the work being
/// awaited completes on the **background** executor. `ModelContext::spawn` runs
/// its future on a background thread and hands the result back to the foreground
/// via a channel, so the whole blocked-action pipeline (`queue_actions` ->
/// preprocess -> execute -> `FinishedAction`) resolves across threads. A fixed
/// yield count races that cross-thread hand-off; worse, a tight foreground
/// yield-loop keeps the block-on thread busy and starves the single-threaded
/// test background executor of CPU. Under parallel test execution that made
/// these tests flaky (and, when combined with a blocking `.await`, could
/// deadlock via a lost `async_io::block_on` wakeup).
///
/// Each iteration runs [`settle`] (which self-wakes via `yield_now`, keeping
/// `block_on` in its notified re-poll fast path rather than parking, so no
/// cross-thread wake can be lost) and then briefly *sleeps* the block-on thread.
/// The sleep is deliberate: it frees the CPU so the (single-worker, in tests)
/// background executor can actually run — a tight foreground spin-loop instead
/// starves it and livelocks the wait. The total bound stays well under a second.
pub(crate) async fn settle_until(app: &mut App, mut done: impl FnMut(&mut App) -> bool) {
    for _ in 0..600 {
        settle().await;
        if done(app) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    panic!("settle_until: condition was not met within the iteration bound");
}

/// A trivial typed-action root view for tests that need a TUI window whose
/// real subject is a non-root child view.
pub(crate) struct TestHostView;

impl Entity for TestHostView {
    type Event = ();
}

impl TuiView for TestHostView {
    fn ui_name() -> &'static str {
        "TestHostView"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn TuiElement> {
        Box::new(TuiText::new(""))
    }
}

impl TypedActionView for TestHostView {
    type Action = ();
}
/// Registers the singletons the editor-backed TUI views read during render:
/// semantic-selection settings and the ignored-suggestions model. Idempotent, so
/// it composes with `register_tui_session_view_test_singletons` (whose
/// `register_all_settings` also registers `SemanticSelection`) without
/// double-registration panics.
pub(crate) fn add_test_semantic_selection(ctx: &mut AppContext) {
    if !ctx.has_singleton_model::<SemanticSelection>() {
        ctx.add_singleton_model(|_| SemanticSelection::mock(true, ""));
    }
    if !ctx.has_singleton_model::<IgnoredSuggestionsModel>() {
        ctx.add_singleton_model(|_| IgnoredSuggestionsModel::new(Vec::new()));
    }
}

pub(crate) fn add_test_conversation_selection(ctx: &mut AppContext) -> ConversationSelectionHandle {
    if !ctx.has_singleton_model::<AppExecutionMode>() {
        ctx.add_singleton_model(|ctx| AppExecutionMode::new(ExecutionMode::App, false, ctx));
    }
    if !ctx.has_singleton_model::<BlocklistAIHistoryModel>() {
        ctx.add_singleton_model(|_| BlocklistAIHistoryModel::default());
    }
    let terminal_surface_id = EntityId::new();
    let mut terminal_model = TerminalModel::mock(None, None);
    terminal_model
        .block_list_mut()
        .set_agent_view_state(AgentViewState::Inactive);
    let terminal_model = Arc::new(FairMutex::new(terminal_model));
    ctx.add_model(|ctx| {
        Box::new(TuiConversationSelection::new(
            terminal_surface_id,
            terminal_model,
            ctx,
        )) as Box<dyn ConversationSelection>
    })
}

/// Builds the action model injected into stateful TUI tool-call views.
pub(crate) fn add_test_action_model(app: &mut App) -> ModelHandle<BlocklistAIActionModel> {
    add_test_action_model_and_events(app).0
}

/// Builds the action model and terminal-event dispatcher injected into TUI agent blocks.
pub(crate) fn add_test_action_model_and_events(
    app: &mut App,
) -> (
    ModelHandle<BlocklistAIActionModel>,
    ModelHandle<ModelEventDispatcher>,
) {
    if !app.read(|ctx| ctx.has_singleton_model::<Appearance>()) {
        app.add_singleton_model(|_| Appearance::mock());
    }
    // The execute/preprocess pipeline reads the app-wide execution mode when
    // deciding whether an action auto-executes or must block on confirmation.
    // Register it here so every action-model-backed view test is hermetic rather
    // than depending on a sibling test to have registered it first.
    if !app.read(|ctx| ctx.has_singleton_model::<AppExecutionMode>()) {
        app.add_singleton_model(|ctx| AppExecutionMode::new(ExecutionMode::App, false, ctx));
    }
    app.update(|ctx| add_test_semantic_selection(ctx));
    // Read as a singleton by the action model's executors. Guarded so tests that
    // provision the full harness (`register_tui_session_view_test_singletons`,
    // which also registers this) don't double-register.
    if !app.read(|ctx| ctx.has_singleton_model::<BlocklistAIHistoryModel>()) {
        app.add_singleton_model(|_| BlocklistAIHistoryModel::default());
    }
    let terminal_model = Arc::new(FairMutex::new(TerminalModel::mock(None, None)));
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let (_tx, model_events_rx) = async_channel::unbounded();
    let dispatcher =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let active_session =
        app.add_model(|ctx| ActiveSession::new(sessions.clone(), dispatcher.clone(), ctx));
    let terminal_surface_id = EntityId::new();
    let action_model = app.add_model(|ctx| {
        BlocklistAIActionModel::new(
            terminal_model,
            active_session,
            &dispatcher,
            terminal_surface_id,
            ctx,
        )
    });
    (action_model, dispatcher)
}

/// Builds a full session view against mock terminal plumbing.
pub(crate) fn add_test_terminal_session(
    app: &mut App,
    window_id: WindowId,
) -> (
    ViewHandle<TuiTerminalSessionView>,
    ModelHandle<Box<dyn TerminalManagerTrait>>,
) {
    add_test_terminal_session_with_settings_file_error(app, window_id, None)
}

pub(crate) fn add_test_terminal_session_with_settings_file_error(
    app: &mut App,
    window_id: WindowId,
    initial_settings_file_error: Option<SettingsFileError>,
) -> (
    ViewHandle<TuiTerminalSessionView>,
    ModelHandle<Box<dyn TerminalManagerTrait>>,
) {
    app.update(|ctx| {
        // `TuiTerminalSessionView::new` (via `TuiZeroStateView::new`) reads
        // the zero-state animation config singleton unconditionally, exactly
        // as the real app does after `session::init` registers it — see
        // `zero_state_animation_config.rs` and #384. Guarded so tests that
        // already provisioned it (e.g. via their own
        // `ZeroStateAnimationConfig::register` call) don't double-register.
        if !ctx.has_singleton_model::<crate::zero_state_animation::ZeroStateAnimationConfig>() {
            crate::zero_state_animation::ZeroStateAnimationConfig::register(ctx);
        }
        let surface_init = TerminalSurfaceInit::new_for_test(ctx);
        let terminal_model = surface_init.model.clone();
        let view = ctx.add_typed_action_tui_view(window_id, |ctx| {
            TuiTerminalSessionView::new(
                surface_init,
                TuiExitSummaryHandle::default(),
                false,
                initial_settings_file_error,
                ctx,
            )
        });
        let manager = ctx.add_model(|_| {
            Box::new(TestTerminalManager(terminal_model)) as Box<dyn TerminalManagerTrait>
        });
        (view, manager)
    })
}
