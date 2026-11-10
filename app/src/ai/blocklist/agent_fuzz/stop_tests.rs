//! Stop means stop (#1942, owner decision 2026-11-09), end to end on the L1 rig, counted on the
//! requests the model received (the seam's own record, not the controller's bookkeeping).
//!
//! The reply asks for a file read and then a command. The read finishes; the command's card
//! waits. The user presses Stop. Before #1942 the one finished result made the controller plan
//! an automatic follow-up, so a request went out AFTER the stop (and its reply could queue
//! actions that then ran). Now nothing is sent; both results (the read's, and the cancelled
//! command's) ride on the user's next message, in one request.

use std::time::{Duration, Instant};

use fake_llm::scenario::*;
use warpui::App;

use super::actor;
use super::rig::{self, pump_for, settle_until};
use crate::ai::agent::CancellationReason;
use crate::ai::agent::conversation::ConversationStatus;
use crate::ai::agent_providers::fake_stream_seam::RecordedRole;
use crate::ai::blocklist::action_model::{PendingStage, fuzz_action_kind};
use crate::features::FeatureFlag;

fn read_then_command() -> Scenario {
    let mut scenario = Scenario::new(7);
    scenario.turns.push(Turn {
        when: Match::Nth { n: 0 },
        reply: Reply::ToolCalls {
            calls: vec![
                Call::new(CallId::Fresh, "read_files", Args::Valid),
                Call::new(
                    CallId::Fresh,
                    "run_shell_command",
                    Args::Command {
                        cmd: "c1".to_owned(),
                    },
                ),
            ],
        },
        chaos: None,
    });
    scenario.turns.push(Turn {
        when: Match::Default,
        reply: Reply::Text {
            text: "done".to_owned(),
        },
        chaos: None,
    });
    scenario.env.commands.insert(
        "c1".to_owned(),
        CmdBehaviour::Quick {
            out: "ok".to_owned(),
        },
    );
    scenario
}

/// Fails on the pre-#1942 code at the first count: the stop opened request 1.
#[test]
fn a_stop_after_a_finished_result_opens_no_request_and_the_results_ride_on_the_next_message() {
    App::test((), |mut app| async move {
        let _queue_slash = FeatureFlag::QueueSlashCommand.override_enabled(true);
        let _queue_v2 = FeatureFlag::QueuedPromptsV2.override_enabled(true);
        let _ask_question = FeatureFlag::AskUserQuestion.override_enabled(true);
        let scenario = read_then_command();
        let mut rig = rig::setup(&mut app, &scenario, false);
        let conversation_id = rig.conversation_id;
        rig.send(&mut app, "start the task");

        // The read finishes (approved, if it asks); the command's card is then all that is held.
        let deadline = Instant::now() + Duration::from_secs(15);
        let mut command_waits = false;
        while Instant::now() < deadline {
            pump_for(&mut app, Duration::from_millis(10)).await;
            let obs = rig.observe(&app);
            let front = rig
                .front_action(&app)
                .map(|action| fuzz_action_kind(&action.action));
            if front == Some("run_shell_command")
                && obs.snapshot.len() == 1
                && obs.snapshot[0].stage == PendingStage::Blocked
                && !obs.stream_in_flight
            {
                command_waits = true;
                break;
            }
            if front.is_some() && front != Some("run_shell_command") && !obs.cards.is_empty() {
                actor::perform(&mut app, &mut rig, &Op::Approve);
            }
        }
        assert!(
            command_waits,
            "the read never finished with the command's card waiting: {:?}",
            rig.observe(&app).snapshot
        );
        let finished_before_stop = rig.action_model.read(&app, |model, _| {
            model
                .get_finished_action_results(conversation_id)
                .map(|results| {
                    results
                        .iter()
                        .filter(|result| !result.result.is_cancelled())
                        .count()
                })
                .unwrap_or(0)
        });
        assert_eq!(
            finished_before_stop, 1,
            "one result (the read) is recorded before the stop"
        );
        let requests_before_stop = rig.seam.request_count();
        assert_eq!(requests_before_stop, 1, "only the opening request so far");

        // The status bar's Stop button.
        rig.controller.update(&mut app, |controller, ctx| {
            controller.cancel_conversation_progress(
                conversation_id,
                CancellationReason::ManuallyCancelled,
                ctx,
            );
        });
        pump_for(&mut app, Duration::from_millis(500)).await;

        assert_eq!(
            rig.seam.request_count(),
            requests_before_stop,
            "Stop means stop: no request may be opened after it"
        );
        let obs = rig.observe(&app);
        assert_eq!(obs.status, Some(ConversationStatus::Cancelled));
        assert!(obs.snapshot.is_empty(), "held: {:?}", obs.snapshot);
        assert!(!obs.stream_in_flight);

        // The user's next message: exactly one request, carrying a result for both calls.
        rig.send(&mut app, "carry on please");
        let sent = settle_until(&mut app, Duration::from_secs(10), |app| {
            rig.seam.request_count() > requests_before_stop
                && rig.observe(app).status == Some(ConversationStatus::Success)
        })
        .await;
        assert!(
            sent,
            "the next message never reached the model and finished"
        );
        pump_for(&mut app, Duration::from_millis(300)).await;
        let requests = rig.seam.requests();
        assert_eq!(
            requests.len(),
            requests_before_stop + 1,
            "one message, one request"
        );
        let next = &requests[requests_before_stop];
        let newest = next.messages.last().expect("the request has messages");
        assert_eq!(newest.role, RecordedRole::User);
        assert!(newest.text.contains("carry on please"), "{:?}", newest.text);
        let emitted = rig.player.emitted();
        let calls = &emitted[0].call_ids;
        assert_eq!(calls.len(), 2);
        let answered = next.tool_response_ids();
        for call in calls {
            assert!(
                answered.contains(&call.as_str()),
                "the next request must carry a result for {call}: it has {answered:?}"
            );
        }
    });
}
