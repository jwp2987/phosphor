use std::cell::{Cell, RefCell};
use std::rc::Rc;

use ai::diff_validation::{AIRequestedCodeDiff, DiffType};
use async_channel::unbounded;
use futures::FutureExt;
use warpui::{App, AppContext, EntityId};

use super::*;
use crate::ai::agent::task::TaskId;
use crate::ai::blocklist::diff_storage::RegisteredDiffStorage;
use crate::terminal::model::session::Sessions;
use crate::terminal::model_events::ModelEventDispatcher;

/// Shared observable state for a [`TestStorage`].
struct TestStorageState {
    diffs: RefCell<Option<(Vec<FileDiff>, DiffSessionType)>>,
    accepted: Cell<bool>,
}

impl TestStorageState {
    fn new() -> Rc<Self> {
        Rc::new(Self {
            diffs: RefCell::new(None),
            accepted: Cell::new(false),
        })
    }
}

/// A registrable storage double that records seeding and accepts immediately.
struct TestStorage(Rc<TestStorageState>);

impl RegisteredDiffStorage for TestStorage {
    fn set_candidate_diffs(
        &self,
        diffs: Vec<FileDiff>,
        session_type: DiffSessionType,
        _app: &mut AppContext,
    ) {
        *self.0.diffs.borrow_mut() = Some((diffs, session_type));
    }

    fn accept_and_save(&self, _app: &mut AppContext) -> BoxFuture<'static, RequestFileEditsResult> {
        self.0.accepted.set(true);
        futures::future::ready(RequestFileEditsResult::Success {
            diff: String::new(),
            updated_files: Vec::new(),
            deleted_files: Vec::new(),
            lines_added: 0,
            lines_removed: 0,
        })
        .boxed()
    }
}

/// Builds an executor over a minimal test session.
fn add_executor(app: &mut App) -> ModelHandle<RequestFileEditsExecutor> {
    let sessions = app.add_model(|_| Sessions::new_for_test());
    let (_, model_events_rx) = unbounded();
    let dispatcher =
        app.add_model(|ctx| ModelEventDispatcher::new(model_events_rx, sessions.clone(), ctx));
    let active_session =
        app.add_model(|ctx| ActiveSession::new(sessions.clone(), dispatcher.clone(), ctx));
    app.add_model(|ctx| RequestFileEditsExecutor::new(active_session, EntityId::new(), ctx))
}

/// Registers a `TestStorage` for `action_id` and returns its observable state.
///
/// NOTE: Warp's `RequestFileEditsExecutor::register_requested_edits` takes a
/// `Box<dyn RegisteredDiffStorage>` directly (a single unified `diff_storages`
/// map for both GUI and non-GUI surfaces). The fork split this into two maps:
/// `diff_views: HashMap<_, ViewHandle<CodeDiffView>>` for the GUI and
/// `tui_diff_storages: HashMap<_, Box<dyn RegisteredDiffStorage>>` for the TUI,
/// with the storage-registration entry point renamed to
/// `register_requested_edits_storage`. This call is adapted to the fork's name;
/// the underlying map is `tui_diff_storages`, referenced accordingly below.
fn register_storage(
    app: &mut App,
    executor: &ModelHandle<RequestFileEditsExecutor>,
    action_id: &AIAgentActionId,
) -> Rc<TestStorageState> {
    let state = TestStorageState::new();
    let storage = Box::new(TestStorage(state.clone()));
    executor.update(app, |executor, _| {
        executor.register_requested_edits_storage(action_id, storage);
    });
    state
}

/// Builds a `RequestFileEdits` action with the given id.
fn edit_action(id: &AIAgentActionId) -> AIAgentAction {
    AIAgentAction {
        id: id.clone(),
        task_id: TaskId::new("task".to_owned()),
        action: AIAgentActionType::RequestFileEdits {
            file_edits: Vec::new(),
            title: None,
        },
        requires_result: true,
    }
}

/// Runs `execute` for the given action.
fn execute(
    app: &mut App,
    executor: &ModelHandle<RequestFileEditsExecutor>,
    action_id: &AIAgentActionId,
) -> AnyActionExecution {
    let action = edit_action(action_id);
    let conversation_id = AIConversationId::new();
    executor.update(app, |executor, ctx| {
        executor
            .execute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id,
                },
                ctx,
            )
            .into()
    })
}

#[test]
fn on_diffs_applied_seeds_registered_storage() {
    App::test((), |mut app| async move {
        let executor = add_executor(&mut app);
        let action_id = AIAgentActionId::from("edit-1".to_owned());
        let storage = register_storage(&mut app, &executor, &action_id);

        let (tx, _rx) = oneshot::channel();
        executor.update(&mut app, |executor, ctx| {
            executor.on_diffs_applied(
                Ok(vec![AIRequestedCodeDiff {
                    file_name: "/tmp/x.rs".to_owned(),
                    diff_type: DiffType::creation("fn main() {}\n".to_owned()),
                    failures: None,
                    original_content: String::new(),
                }]),
                action_id.clone(),
                tx,
                ctx,
            );
        });

        let seeded = storage.diffs.borrow_mut().take();
        let (diffs, session_type) = seeded.expect("registered storage should be seeded");
        assert_eq!(diffs.len(), 1);
        assert_eq!(diffs[0].file_path(), "/tmp/x.rs");
        assert!(matches!(session_type, DiffSessionType::Local));
    });
}

#[test]
fn execute_accepts_through_registered_storage() {
    App::test((), |mut app| async move {
        app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
        let executor = add_executor(&mut app);
        let action_id = AIAgentActionId::from("edit-1".to_owned());
        let storage = register_storage(&mut app, &executor, &action_id);

        let execution = execute(&mut app, &executor, &action_id);

        assert!(matches!(execution, AnyActionExecution::Async { .. }));
        assert!(storage.accepted.get());
        // The entry stays registered until the action's terminal result
        // funnels through `discard_pending`.
        //
        // NEEDS-ADAPTATION: Warp's `RequestFileEditsExecutor` has a
        // `discard_pending` method that removes an action's entry from the
        // (unified) `diff_storages` map once its terminal result is known.
        // The fork has NO equivalent method on ANY of its three per-action
        // maps (`diff_views`, `tui_diff_storages`, `diff_application_failures`)
        // — grepping the whole file shows `.insert()`/`.remove()` calls only
        // for `diff_application_failures` (removed at the top of `execute`),
        // while `diff_views` and `tui_diff_storages` entries are inserted on
        // registration and never removed anywhere. This assertion is instead
        // checking the fork's `tui_diff_storages` map (the renamed
        // equivalent), which still holds after execute — but the fork has no
        // path that would ever clear it, unlike Warp's `discard_pending`.
        executor.update(&mut app, |executor, _| {
            assert!(executor.tui_diff_storages.contains_key(&action_id));
        });
    });
}

#[test]
fn execute_reports_preprocess_failure() {
    App::test((), |mut app| async move {
        let executor = add_executor(&mut app);
        let action_id = AIAgentActionId::from("edit-failed".to_owned());
        executor.update(&mut app, |executor, _| {
            executor
                .diff_application_failures
                .insert(action_id.clone(), vec1![DiffApplicationError::EmptyDiff]);
        });

        let execution = execute(&mut app, &executor, &action_id);

        assert!(matches!(
            execution,
            AnyActionExecution::Sync(AIAgentActionResultType::RequestFileEdits(
                RequestFileEditsResult::DiffApplicationFailed { .. }
            ))
        ));
    });
}

#[test]
fn execute_without_prepared_diffs_is_not_ready() {
    App::test((), |mut app| async move {
        let executor = add_executor(&mut app);
        let action_id = AIAgentActionId::from("edit-1".to_owned());

        let execution = execute(&mut app, &executor, &action_id);

        assert!(matches!(execution, AnyActionExecution::NotReady));
    });
}

// NEEDS-ADAPTATION (not ported): Warp's `discard_pending_drops_state_in_any_state`
// exercises `RequestFileEditsExecutor::discard_pending`, which the fork does not
// implement at all (no method of that name, under any name, on this executor —
// confirmed by grepping this file for `.remove(`, which only appears for
// `diff_application_failures`, never for `diff_views`/`tui_diff_storages`).
// This is not a rename-only mismatch: the cleanup behavior itself is absent, so
// the test cannot be ported without adding a method to product code, which is
// out of scope for a test port. Left out rather than force-adapted.
//
// This gap corroborates issue #9: entries placed in `diff_views` (via
// `register_requested_edits`) or `tui_diff_storages` (via
// `register_requested_edits_storage`) are never removed by any code path in
// this file. A rejected/cancelled edit (see the `CodeDiffViewEvent::Rejected`
// arm in `execute`, which sends `RequestFileEditsResult::Cancelled` but does
// not touch `diff_views`) leaves its `diff_views` entry (and, on the TUI path,
// its `tui_diff_storages` entry) permanently registered — an unbounded-growth
// state leak that Warp's `discard_pending` exists specifically to prevent.
