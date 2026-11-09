use super::*;
use crate::LaunchMode;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::task::TaskId;
use crate::ai::agent::{AIAgentAction, AIAgentActionId, AIAgentActionResultType};
use crate::ai::blocklist::{BlocklistAIHistoryModel, BlocklistAIPermissions};
use crate::ai::execution_profiles::{
    AskUserQuestionPermission, profiles::AIExecutionProfilesModel,
};
use crate::ai::mcp::templatable_manager::TemplatableMCPServerManager;
use crate::auth::AuthStateProvider;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::cloud_object::update_manager::UpdateManager;
use crate::network::NetworkStatus;
use crate::test_util::settings::initialize_settings_for_tests;
use crate::workspaces::user_workspaces::UserWorkspaces;
use ai::agent::action::AskUserQuestionItem;
use ai::agent::action_result::{AskUserQuestionAnswerItem, AskUserQuestionResult};
use warpui::{App, EntityId, ModelHandle};

fn build_action(action_id: &str) -> AIAgentAction {
    AIAgentAction {
        id: AIAgentActionId::from(action_id.to_string()),
        action: AIAgentActionType::AskUserQuestion {
            questions: vec![AskUserQuestionItem {
                question_id: "q1".to_string(),
                question: "What should we use?".to_string(),
                question_type: ai::agent::action::AskUserQuestionType::MultipleChoice {
                    is_multiselect: false,
                    options: vec![],
                    supports_other: true,
                },
            }],
        },
        task_id: TaskId::new(format!("task-{action_id}")),
        requires_result: false,
    }
}

#[test]
fn should_autoexecute_returns_false_when_autoapprove_is_enabled_and_profile_always_blocks() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, true, false, ctx)
        });

        profiles.update(&mut app, |profiles, ctx| {
            let profile_id = *profiles.active_profile(Some(terminal_view_id), ctx).id();
            profiles.set_ask_user_question(profile_id, AskUserQuestionPermission::AlwaysAsk, ctx);
        });

        let result = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id,
            };
            executor.should_autoexecute(input, ctx)
        });

        assert!(!result);
    });
}

fn initialize_ask_user_question_test(
    app: &mut App,
    terminal_view_id: EntityId,
) -> (
    ModelHandle<BlocklistAIHistoryModel>,
    ModelHandle<AIExecutionProfilesModel>,
) {
    initialize_settings_for_tests(app);
    let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());
    app.add_singleton_model(|_| AuthStateProvider::new_for_test());
    app.add_singleton_model(|_| NetworkStatus::new());
    app.add_singleton_model(UpdateManager::mock);
    app.add_singleton_model(ObjectStoreModel::mock);
    app.add_singleton_model(|_| TemplatableMCPServerManager::default());
    app.add_singleton_model(UserWorkspaces::default_mock);
    let profiles = app.add_singleton_model(|ctx| {
        AIExecutionProfilesModel::new(&LaunchMode::new_for_unit_test(), ctx)
    });
    app.add_singleton_model(BlocklistAIPermissions::new);
    // Ensure asking questions is allowed by default regardless of compile-time profile
    // defaults (e.g. agent_mode_evals overrides ask_user_question to Never).
    profiles.update(app, |profiles, ctx| {
        if let Some(profile_id) = profiles.create_profile(ctx) {
            profiles.set_ask_user_question(
                profile_id,
                AskUserQuestionPermission::AskExceptInAutoApprove,
                ctx,
            );
            profiles.set_active_profile(terminal_view_id, profile_id, ctx);
        }
    });
    (history, profiles)
}

#[test]
fn should_autoexecute_returns_false_when_questions_are_allowed() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let result = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id: AIConversationId::new(),
            };
            executor.should_autoexecute(input, ctx)
        });

        assert!(!result);
    });
}

// openWarp: auto-approve (ctrl+shift+i) only auto-passes shell/edit tools;
// ask_user_question must always be surfaced to the user, even in autoapprove mode.
#[test]
fn should_autoexecute_returns_false_when_autoapprove_is_enabled_with_default_profile() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, true, false, ctx)
        });
        let result = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id,
            };
            executor.should_autoexecute(input, ctx)
        });

        assert!(!result);
    });
}

#[test]
fn execute_returns_async_when_autoapprove_is_enabled_with_default_profile() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, true, false, ctx)
        });

        let execution = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id,
            };
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            result
        });

        assert!(matches!(execution, AnyActionExecution::Async { .. }));
    });
}

#[test]
fn execute_returns_async_and_resolves_on_complete() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("action-a");

        let execution = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id: AIConversationId::new(),
            };
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            result
        });

        let AnyActionExecution::Async {
            execute_future,
            on_complete,
        } = execution
        else {
            panic!("expected async execution");
        };

        executor.update(&mut app, |executor, _| {
            executor.complete(vec![AskUserQuestionAnswerItem::Skipped {
                question_id: "q1".to_string(),
            }]);
        });

        let async_result = execute_future.await;
        let result = app.update(|ctx| on_complete(async_result, ctx));
        assert!(matches!(
            result,
            AIAgentActionResultType::AskUserQuestion(AskUserQuestionResult::Success { answers })
                if answers
                    == vec![AskUserQuestionAnswerItem::Skipped {
                        question_id: "q1".to_string(),
                    }]
        ));
    });
}

#[test]
fn cancel_resolves_as_cancelled() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");

        let execution = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id: AIConversationId::new(),
            };
            let result: AnyActionExecution = executor.execute(input, ctx).into();
            result
        });

        let AnyActionExecution::Async {
            execute_future,
            on_complete,
        } = execution
        else {
            panic!("expected async execution");
        };

        executor.update(&mut app, |executor, _| {
            executor.cancel();
        });

        let async_result = execute_future.await;
        let result = app.update(|ctx| on_complete(async_result, ctx));
        assert!(matches!(
            result,
            AIAgentActionResultType::AskUserQuestion(AskUserQuestionResult::Cancelled)
        ));
    });
}

#[test]
fn should_autoexecute_uses_active_terminal_profile_permission() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        profiles.update(&mut app, |profiles, ctx| {
            let profile_id = profiles
                .create_profile(ctx)
                .expect("test profile should be created");
            profiles.set_ask_user_question(profile_id, AskUserQuestionPermission::Never, ctx);
            profiles.set_active_profile(terminal_view_id, profile_id, ctx);
        });

        let result = executor.update(&mut app, |executor, ctx| {
            let input = ExecuteActionInput {
                action: &action,
                conversation_id,
            };
            executor.should_autoexecute(input, ctx)
        });

        assert!(result);
    });
}

/// #1278: a decision queued while no card was waiting (Stop racing an answer) must not be
/// consumed by the next card. Old code left it in the shared channel.
#[test]
fn execute_drains_a_stale_decision_from_an_earlier_card() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("ask-user-question");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, true, false, ctx)
        });

        let stale_left = executor.update(&mut app, |executor, ctx| {
            executor.cancel(); // nothing is waiting: this decision is stale
            let _ = executor.execute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id,
                },
                ctx,
            );
            executor.result_rx.1.try_recv().is_ok()
        });

        assert!(!stale_left);
    });
}

fn execute_now(
    app: &mut App,
    executor: &ModelHandle<AskUserQuestionExecutor>,
    action: &AIAgentAction,
    conversation_id: AIConversationId,
) -> AnyActionExecution {
    executor.update(app, |executor, ctx| {
        executor
            .execute(
                ExecuteActionInput {
                    action,
                    conversation_id,
                },
                ctx,
            )
            .into()
    })
}

/// #1797: the card stages its answers and then executes the action. The answers come back as
/// the result at once. Before, the card wrote them to the shared decision channel, which
/// `execute` drains on entry (#1278): the answer was discarded and the action waited on an
/// empty channel forever, stuck on "Preparing question...".
#[test]
fn answers_staged_before_execute_are_its_result() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("answered");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let answers = vec![AskUserQuestionAnswerItem::Skipped {
            question_id: "q1".to_string(),
        }];

        executor.update(&mut app, |executor, _| {
            executor.stage_answers(action.id.clone(), answers.clone());
            assert!(executor.has_staged_answers(&action.id));
        });
        let execution = execute_now(&mut app, &executor, &action, conversation_id);

        assert!(
            matches!(
                &execution,
                AnyActionExecution::Sync(AIAgentActionResultType::AskUserQuestion(
                    AskUserQuestionResult::Success { answers: got },
                )) if *got == answers
            ),
            "{execution:?}"
        );
        executor.read(&app, |executor, _| {
            assert!(
                !executor.has_staged_answers(&action.id),
                "taken by the execution"
            );
        });
    });
}

/// #1797 / #1278: an answer is staged for one action only. Executing another question does not
/// take it (that one still waits for its own card), and the staged answer stays for its action.
#[test]
fn staged_answers_belong_to_their_action() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let answered = build_action("answered");
        let other = build_action("other");
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        executor.update(&mut app, |executor, _| {
            executor.stage_answers(
                answered.id.clone(),
                vec![AskUserQuestionAnswerItem::Skipped {
                    question_id: "q1".to_string(),
                }],
            );
        });
        let execution = execute_now(&mut app, &executor, &other, conversation_id);

        assert!(
            matches!(execution, AnyActionExecution::Async { .. }),
            "{execution:?}"
        );
        executor.update(&mut app, |executor, _| {
            assert!(executor.has_staged_answers(&answered.id));
            executor.discard_staged_answers(&answered.id);
            assert!(!executor.has_staged_answers(&answered.id));
        });
    });
}

/// #1797: `convert_from` drops every question that is not multiple choice, so a call can
/// arrive with none. It never waits for an answer: `should_autoexecute` lets it run without a
/// card, and it returns an error result the model can act on.
#[test]
fn a_question_list_with_nothing_to_show_returns_an_error_at_once() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        // Even a profile that always asks does not hold it: there is nothing to ask.
        profiles.update(&mut app, |profiles, ctx| {
            let profile_id = *profiles.active_profile(Some(terminal_view_id), ctx).id();
            profiles.set_ask_user_question(profile_id, AskUserQuestionPermission::AlwaysAsk, ctx);
        });
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = AIAgentAction {
            action: AIAgentActionType::AskUserQuestion { questions: vec![] },
            ..build_action("empty")
        };
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });

        let autoexecutes = executor.update(&mut app, |executor, ctx| {
            executor.should_autoexecute(
                ExecuteActionInput {
                    action: &action,
                    conversation_id,
                },
                ctx,
            )
        });
        assert!(
            autoexecutes,
            "an empty question list must not wait for approval"
        );

        let execution = execute_now(&mut app, &executor, &action, conversation_id);
        assert!(
            matches!(
                &execution,
                AnyActionExecution::Sync(AIAgentActionResultType::AskUserQuestion(
                    AskUserQuestionResult::Error(message),
                )) if message == NO_ANSWERABLE_QUESTIONS_ERROR
            ),
            "{execution:?}"
        );
    });
}

fn waits_for_answer_now(
    app: &mut App,
    executor: &ModelHandle<AskUserQuestionExecutor>,
    action: &AIAgentAction,
    conversation_id: AIConversationId,
) -> bool {
    executor.update(app, |executor, ctx| {
        executor.waits_for_answer(
            ExecuteActionInput {
                action,
                conversation_id,
            },
            ctx,
        )
    })
}

/// Owner decision 2026-11-08 (#1797): the question's confirmation rule is only "has it been
/// answered". It waits on its card until answers are staged for it and not a moment longer;
/// auto-approve ("run to completion") does not stand in for the answer.
#[test]
fn a_question_waits_for_its_answer_and_only_for_its_answer() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, _profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let action = build_action("question");
        // Auto-approve on: the most autonomous conversation there is.
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, true, false, ctx)
        });

        assert!(
            waits_for_answer_now(&mut app, &executor, &action, conversation_id),
            "no answer yet: it waits on its card, even with auto-approve on"
        );

        executor.update(&mut app, |executor, _| {
            executor.stage_answers(
                action.id.clone(),
                vec![AskUserQuestionAnswerItem::Skipped {
                    question_id: "q1".to_string(),
                }],
            );
        });
        assert!(
            !waits_for_answer_now(&mut app, &executor, &action, conversation_id),
            "answered: it runs, whoever asks it to"
        );
        assert!(
            waits_for_answer_now(
                &mut app,
                &executor,
                &build_action("another-question"),
                conversation_id
            ),
            "another question's answer is not this one's"
        );
    });
}

/// A question with nothing to show never waits (it resolves with an error result), and neither
/// does one the profile says never to ask (a skipped result). Neither is an answer the user
/// gave, and neither leaves anything pending with no card.
#[test]
fn a_question_with_nothing_to_ask_does_not_wait() {
    App::test((), |mut app| async move {
        let terminal_view_id = EntityId::new();
        let (history, profiles) = initialize_ask_user_question_test(&mut app, terminal_view_id);
        let executor = app.add_model(|_| AskUserQuestionExecutor::new(terminal_view_id));
        let conversation_id = history.update(&mut app, |history, ctx| {
            history.start_new_conversation(terminal_view_id, false, false, ctx)
        });
        let empty = AIAgentAction {
            action: AIAgentActionType::AskUserQuestion { questions: vec![] },
            ..build_action("empty")
        };
        assert!(!waits_for_answer_now(
            &mut app,
            &executor,
            &empty,
            conversation_id
        ));

        profiles.update(&mut app, |profiles, ctx| {
            let profile_id = *profiles.active_profile(Some(terminal_view_id), ctx).id();
            profiles.set_ask_user_question(profile_id, AskUserQuestionPermission::Never, ctx);
        });
        assert!(!waits_for_answer_now(
            &mut app,
            &executor,
            &build_action("never-asked"),
            conversation_id
        ));
    });
}
