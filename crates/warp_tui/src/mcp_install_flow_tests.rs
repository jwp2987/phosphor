use uuid::Uuid;
use warp::appearance::Appearance;
use warp::editor::CodeEditorModel;
use warp::tui_export::{TuiMcpInstallRequest, TuiMcpServerId, TuiMcpTemplateVariable};
use warp_editor::model::CoreEditorModel;
use warpui_core::{App, AppContext, ModelHandle};

use super::{TuiMcpInstallFlowAction, TuiMcpInstallFlowModel, TuiMcpInstallStep, input_text};
use crate::inline_menu::TuiInlineMenuInputOwnership;
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};

fn input_editor(ctx: &mut warpui_core::AppContext) -> warpui_core::ModelHandle<CodeEditorModel> {
    ctx.add_singleton_model(|_| Appearance::mock());
    ctx.add_model(|ctx| CodeEditorModel::new_tui(80, ctx))
}

fn free_text(key: &str) -> TuiMcpTemplateVariable {
    TuiMcpTemplateVariable {
        key: key.to_owned(),
        allowed_values: None,
    }
}

fn dropdown(key: &str, allowed_values: &[&str]) -> TuiMcpTemplateVariable {
    TuiMcpTemplateVariable {
        key: key.to_owned(),
        allowed_values: Some(
            allowed_values
                .iter()
                .map(|value| (*value).to_owned())
                .collect(),
        ),
    }
}

/// Builds a flow over a fresh shared editor and starts it on `variables`.
fn started_flow(
    ctx: &mut AppContext,
    variables: Vec<TuiMcpTemplateVariable>,
) -> (
    ModelHandle<CodeEditorModel>,
    ModelHandle<TuiInputSuggestionsModeModel>,
    ModelHandle<TuiMcpInstallFlowModel>,
) {
    let editor = input_editor(ctx);
    let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
    let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor.clone(), mode.clone()));
    flow.update(ctx, |flow, ctx| {
        assert!(flow.start(request(variables), ctx), "the flow should open");
    });
    (editor, mode, flow)
}

fn request(variables: Vec<TuiMcpTemplateVariable>) -> TuiMcpInstallRequest {
    TuiMcpInstallRequest {
        id: TuiMcpServerId::Template(Uuid::from_u128(1)),
        name: "Example".to_owned(),
        variables,
    }
}

#[test]
fn zero_variable_request_skips_the_install_flow() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let editor = input_editor(ctx);
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor, mode));
            assert!(!flow.update(ctx, |flow, ctx| flow.start(request(Vec::new()), ctx)));
            assert!(matches!(&flow.as_ref(ctx).step, TuiMcpInstallStep::Closed));
            assert!(!flow.as_ref(ctx).is_open(ctx));
        });
    });
}

#[test]
fn collected_value_actions_are_redacted_from_debug_output() {
    let action = TuiMcpInstallFlowAction::ProvideValue {
        key: "TOKEN".to_owned(),
        value: "do-not-log-this".to_owned(),
    };

    let debug = format!("{action:?}");

    assert!(debug.contains("TOKEN"));
    assert!(debug.contains("[REDACTED]"));
    assert!(!debug.contains("do-not-log-this"));
}

#[test]
fn allowed_values_are_presented_as_selectable_rows() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let editor = input_editor(ctx);
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor, mode));
            let variable = TuiMcpTemplateVariable {
                key: "REGION".to_owned(),
                allowed_values: Some(vec!["us".to_owned(), "eu".to_owned()]),
            };

            flow.update(ctx, |flow, ctx| {
                assert!(flow.start(request(vec![variable]), ctx));
            });
            let snapshot = flow.as_ref(ctx).snapshot(ctx).expect("flow is visible");
            assert_eq!(
                snapshot
                    .rows
                    .iter()
                    .map(|row| row.title.as_str())
                    .collect::<Vec<_>>(),
                vec!["us", "eu"]
            );
            assert_eq!(snapshot.selected_index, Some(0));
            assert_eq!(
                flow.as_ref(ctx).primary_action_hint(),
                Some("to install and enable")
            );
        });
    });
}

#[test]
fn final_value_completes_installation_without_confirmation() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let editor = input_editor(ctx);
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor, mode));
            let variable = TuiMcpTemplateVariable {
                key: "TOKEN".to_owned(),
                allowed_values: None,
            };
            let completion = flow.update(ctx, |flow, ctx| {
                assert!(flow.start(request(vec![variable]), ctx));
                flow.apply_value("TOKEN".to_owned(), "secret".to_owned(), ctx)
                    .expect("value is accepted")
                    .expect("the final value completes installation")
            });

            assert_eq!(completion.name, "Example");
            assert_eq!(completion.values.len(), 1);
        });
    });
}

// ── Input ownership and masking (#602) ────────────────────────────────────────
//
// The flow collects MCP template values, which are routinely API tokens, into
// the shared inline-menu buffer. Before this it declared no ownership at all,
// so the buffer defaulted to `Composer` and every typed character was painted
// into the grid in the clear. The three tests that assert
// `InlineMenuMasked`/`InlineMenuPlainText` are the ones that fail against that
// rendering -- it can only report `Composer`. The remaining three pin the
// invariants that make masking safe rather than merely present: it starts off,
// it ends only once the buffer no longer holds the secret, and a parked flow
// never speaks for the shared editor.

#[test]
fn an_unstarted_flow_leaves_the_input_to_the_composer() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let editor = input_editor(ctx);
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor, mode));

            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::Composer
            );
        });
    });
}

#[test]
fn free_text_variable_masks_the_shared_input() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let (editor, _mode, flow) = started_flow(ctx, vec![free_text("GITHUB_TOKEN")]);

            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuMasked,
                "a free-text template value can be a credential, so it must not be painted \
                 into the grid"
            );

            editor.update(ctx, |editor, ctx| {
                editor.user_insert("ghp-do-not-show-this", ctx);
            });

            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuMasked
            );
            // Masking is paint-only: the model keeps the real text, which is
            // what leaves the value editable and submittable.
            assert_eq!(input_text(&editor, ctx), "ghp-do-not-show-this");
            let action = flow
                .as_ref(ctx)
                .accept(ctx)
                .expect("a non-empty typed value is acceptable");
            let TuiMcpInstallFlowAction::ProvideValue { key, value } = action;
            assert_eq!(key, "GITHUB_TOKEN");
            assert_eq!(value, "ghp-do-not-show-this");
        });
    });
}

#[test]
fn allowed_values_variable_owns_the_input_as_plain_text() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let (_editor, _mode, flow) = started_flow(ctx, vec![dropdown("REGION", &["us", "eu"])]);

            // The value comes from the selected row, so nothing typed here ever
            // becomes a value: concealing the buffer would hide keystrokes
            // without protecting anything.
            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuPlainText
            );
        });
    });
}

#[test]
fn advancing_past_a_free_text_variable_unmasks_only_an_empty_buffer() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let (editor, _mode, flow) = started_flow(
                ctx,
                vec![free_text("GITHUB_TOKEN"), dropdown("REGION", &["us", "eu"])],
            );
            editor.update(ctx, |editor, ctx| {
                editor.user_insert("ghp-do-not-show-this", ctx);
            });
            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuMasked
            );

            flow.update(ctx, |flow, ctx| {
                assert!(
                    flow.apply_value(
                        "GITHUB_TOKEN".to_owned(),
                        "ghp-do-not-show-this".to_owned(),
                        ctx,
                    )
                    .expect("value is accepted")
                    .is_none(),
                    "one variable remains, so this is not a completion"
                );
            });

            // The next variable is a dropdown, so the buffer stops being
            // masked -- which is only safe because the secret left with the
            // step that owned it.
            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::InlineMenuPlainText
            );
            assert_eq!(input_text(&editor, ctx), "");
        });
    });
}

#[test]
fn dismissing_returns_the_input_to_the_composer_with_nothing_left_to_reveal() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let (editor, _mode, flow) = started_flow(ctx, vec![free_text("GITHUB_TOKEN")]);
            editor.update(ctx, |editor, ctx| {
                editor.user_insert("ghp-do-not-show-this", ctx);
            });

            flow.update(ctx, |flow, ctx| flow.dismiss(ctx));

            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::Composer
            );
            assert_eq!(input_text(&editor, ctx), "");
        });
    });
}

#[test]
fn a_flow_that_lost_the_shared_menu_mode_masks_nothing() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let (_editor, mode, flow) = started_flow(ctx, vec![free_text("GITHUB_TOKEN")]);

            // `try_open` refuses to displace an active `McpInstall` mode, so
            // production cannot reach this; the invariant is asserted anyway
            // because a parked flow must never speak for the shared editor --
            // whichever way, masking or unmasking.
            mode.update(ctx, |mode, ctx| {
                mode.set_mode(TuiInputSuggestionsMode::ModelSelector, ctx);
            });

            assert!(!flow.as_ref(ctx).is_open(ctx));
            assert_eq!(
                flow.as_ref(ctx).input_ownership(ctx),
                TuiInlineMenuInputOwnership::Composer
            );
        });
    });
}

#[test]
fn cancellation_discards_collected_values() {
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let editor = input_editor(ctx);
            let mode = ctx.add_model(|_| TuiInputSuggestionsModeModel::new());
            let flow = ctx.add_model(|_| TuiMcpInstallFlowModel::new(editor, mode));
            let variables = vec![
                TuiMcpTemplateVariable {
                    key: "TOKEN".to_owned(),
                    allowed_values: None,
                },
                TuiMcpTemplateVariable {
                    key: "REGION".to_owned(),
                    allowed_values: None,
                },
            ];

            flow.update(ctx, |flow, ctx| {
                assert!(flow.start(request(variables), ctx));
                assert!(
                    flow.apply_value("TOKEN".to_owned(), "secret".to_owned(), ctx)
                        .expect("value is accepted")
                        .is_none()
                );
                flow.dismiss(ctx);
            });

            assert!(matches!(&flow.as_ref(ctx).step, TuiMcpInstallStep::Closed));
            assert!(flow.as_ref(ctx).request.is_none());
            assert!(flow.as_ref(ctx).values.is_empty());
        });
    });
}
