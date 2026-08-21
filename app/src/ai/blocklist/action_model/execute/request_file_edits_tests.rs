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

#[test]
fn discard_pending_drops_state_in_any_state() {
    App::test((), |mut app| async move {
        let executor = add_executor(&mut app);

        // Registered storage entry (e.g. rejected during review).
        // NOTE: Warp keeps one unified `diff_storages` map; the fork splits it
        // into `diff_views` (GUI) and `tui_diff_storages` (TUI), and
        // `register_storage` above registers into the latter.
        let storage_id = AIAgentActionId::from("edit-storage".to_owned());
        register_storage(&mut app, &executor, &storage_id);
        executor.update(&mut app, |executor, _| {
            executor.discard_pending(&storage_id);
            assert!(!executor.tui_diff_storages.contains_key(&storage_id));
            assert!(!executor.diff_views.contains_key(&storage_id));
        });

        // Failed entry (diff application failed during preprocess).
        let failed_id = AIAgentActionId::from("edit-failed".to_owned());
        executor.update(&mut app, |executor, _| {
            executor
                .diff_application_failures
                .insert(failed_id.clone(), vec1![DiffApplicationError::EmptyDiff]);
            executor.discard_pending(&failed_id);
            assert!(!executor.diff_application_failures.contains_key(&failed_id));
        });
    });
}

// Ported from upstream `89f61b63ba`
// ("Limit apply diff results to changed ranges", #11987) -- see
// `updated_file_contexts_from_editor_buffers`'s doc comment in
// request_file_edits.rs. `AnyFileContent`/`FileLocations` come in via `use
// super::*` (request_file_edits.rs re-exports them from `crate::ai::agent`
// as of this same port), matching how the rest of this file's tests reach
// their fixtures.
#[test]
fn updated_file_contexts_from_editor_buffers_returns_changed_lines_with_context() {
    let updated_files = vec![(
        FileLocations {
            name: "src/main.rs".to_string(),
            lines: std::iter::once(12..13).collect(),
        },
        true,
    )];
    let content = (1..=30)
        .map(|line| format!("line {line}"))
        .collect::<Vec<_>>()
        .join("\n");
    let content_map = HashMap::from([("src/main.rs".to_string(), content)]);

    let contexts = updated_file_contexts_from_editor_buffers(&updated_files, &content_map);

    assert_eq!(contexts.len(), 1);
    assert!(contexts[0].was_edited_by_user);
    assert_eq!(contexts[0].file_context.file_name, "src/main.rs");
    assert_eq!(contexts[0].file_context.line_range, Some(2..23));
    assert_eq!(contexts[0].file_context.line_count, 30);
    assert_eq!(
        contexts[0].file_context.content,
        AnyFileContent::StringContent(
            (2..=22)
                .map(|line| format!("line {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    );
}

#[test]
fn updated_file_contexts_from_editor_buffers_preserves_full_file_when_no_ranges() {
    let updated_files = vec![(
        FileLocations {
            name: "src/main.rs".to_string(),
            lines: vec![],
        },
        false,
    )];
    let content = "line 1\nline 2\n".to_string();
    let content_map = HashMap::from([("src/main.rs".to_string(), content.clone())]);

    let contexts = updated_file_contexts_from_editor_buffers(&updated_files, &content_map);

    assert_eq!(contexts.len(), 1);
    assert!(!contexts[0].was_edited_by_user);
    assert_eq!(contexts[0].file_context.line_range, None);
    assert_eq!(contexts[0].file_context.line_count, 2);
    assert_eq!(
        contexts[0].file_context.content,
        AnyFileContent::StringContent(content)
    );
}

/// A refusal from the pre-write conflict check in
/// `warp_files::FileModel::save_if_unchanged`, in the exact shape the accept
/// path produces: a `FileSaveError::Other` whose message is a complete
/// user-facing sentence, paired with the file it was for.
fn conflict_failure(path: &str) -> FileSaveFailure {
    FileSaveFailure {
        path: Some(path.to_owned()),
        error: Rc::new(FileSaveError::Other(format!(
            "{path} changed on disk after this change was proposed, so it was not overwritten \
             and your edits are intact. Re-run the request to work from the current file."
        ))),
    }
}

fn written(path: &str) -> (FileLocations, bool) {
    (
        FileLocations {
            name: path.to_owned(),
            lines: vec![1..2],
        },
        false,
    )
}

/// Renders a result the way the conversation transcript hands it to the model.
fn as_seen_by_the_model(result: RequestFileEditsResult) -> String {
    crate::ai::agent::MarkdownActionResult(&AIAgentActionResultType::RequestFileEdits(result))
        .to_string()
}

/// Regression: the accept path reduced `save_errors` with a `filter_map` that
/// matched only `FileSaveError::IOError`, so every conflict refusal was dropped
/// and `join` on the empty iterator produced `""`. The model was handed the
/// bare string `File edits failed: ` and could not distinguish a refused
/// overwrite from a full disk.
#[test]
fn conflict_refusal_reaches_the_model_with_its_reason() {
    let result = save_failure_result(&[conflict_failure("/work/src/main.rs")], &[], &[]);

    let RequestFileEditsResult::DiffApplicationFailed { error } = &result else {
        panic!("a refused write must fail the action, got {result:?}");
    };
    assert!(!error.is_empty(), "the refusal message was dropped");
    assert!(
        error.contains("/work/src/main.rs changed on disk after this change was proposed"),
        "reason missing from the result: {error}"
    );
    assert!(
        error.contains("your edits are intact"),
        "reason missing from the result: {error}"
    );

    let rendered = as_seen_by_the_model(result);
    assert!(
        !rendered.contains("File edits failed:  "),
        "the model was told the edit failed with no reason: {rendered}"
    );
    assert!(
        rendered.contains("changed on disk after this change was proposed"),
        "reason missing at the transcript boundary: {rendered}"
    );
}

/// An `IOError` keeps its old spelling: its own `Display` is the constant
/// "IO error when saving file.", so the path and cause have to be spelled out.
#[test]
fn io_failure_still_names_the_path_and_the_cause() {
    let failure = FileSaveFailure {
        path: Some("/work/src/main.rs".to_owned()),
        error: Rc::new(FileSaveError::IOError {
            error: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "permission denied"),
            path: std::path::PathBuf::from("/work/src/main.rs"),
        }),
    };

    let result = save_failure_result(&[failure], &[], &[]);

    let RequestFileEditsResult::DiffApplicationFailed { error } = result else {
        panic!("expected a failure result");
    };
    assert_eq!(
        error,
        "Failed to save file \"/work/src/main.rs\": permission denied"
    );
}

/// Finding 1c: a five-file accept where one file conflicts used to report the
/// whole edit as failed, with an empty message, while four files were already
/// on disk. A model that re-ran the edit applied those four a second time.
/// `RequestFileEditsResult` has no partial-success variant, so the partition is
/// stated in the message: which files landed, which did not, and why.
#[test]
fn partial_accept_names_the_files_that_were_written() {
    let result = save_failure_result(
        &[conflict_failure("/work/src/c.rs")],
        &[written("/work/src/a.rs"), written("/work/src/b.rs")],
        &["/work/src/gone.rs".to_owned()],
    );

    let RequestFileEditsResult::DiffApplicationFailed { error } = &result else {
        panic!("a partially applied edit must not be reported as a plain success");
    };
    assert!(
        error.contains("This edit was applied partially."),
        "partial application not stated: {error}"
    );
    for landed in ["/work/src/a.rs", "/work/src/b.rs", "/work/src/gone.rs"] {
        assert!(
            error.contains(landed),
            "{landed} was written but the model was not told: {error}"
        );
    }
    assert!(
        error.contains("do NOT apply these edits again"),
        "nothing warns the model against re-applying what landed: {error}"
    );
    assert!(
        error.contains("Not written, still holding their previous contents: /work/src/c.rs"),
        "the unwritten file is not identified: {error}"
    );
    assert!(
        error.contains("changed on disk after this change was proposed"),
        "the refusal reason is missing: {error}"
    );
}

/// With nothing on disk there is no partition to report, so the message stays
/// the plain list of reasons rather than claiming a partial application.
#[test]
fn total_failure_reports_reasons_without_claiming_partial_application() {
    let result = save_failure_result(
        &[
            conflict_failure("/work/src/a.rs"),
            conflict_failure("/work/src/b.rs"),
        ],
        &[],
        &[],
    );

    let RequestFileEditsResult::DiffApplicationFailed { error } = &result else {
        panic!("expected a failure result");
    };
    assert!(!error.contains("applied partially"), "{error}");
    assert_eq!(error.lines().count(), 2, "one line per refusal: {error}");
    assert!(error.contains("/work/src/a.rs") && error.contains("/work/src/b.rs"));
}
