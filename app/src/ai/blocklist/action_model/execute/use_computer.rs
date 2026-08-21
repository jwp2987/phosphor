use ai::agent::action_result::AIAgentActionResultType;
use futures::FutureExt;
use futures::future::BoxFuture;
// `SingletonEntity` is what provides `BlocklistAIHistoryModel::as_ref(ctx)`.
use warpui::{Entity, ModelContext, SingletonEntity};

use super::{ActionExecution, AnyActionExecution, ExecuteActionInput, PreprocessActionInput};
use crate::ai::agent::{AIAgentActionType, UseComputerResult};
use crate::ai::blocklist::history_model::BlocklistAIHistoryModel;
use crate::features::FeatureFlag;

pub struct UseComputerExecutor;

impl UseComputerExecutor {
    pub fn new() -> Self {
        Self
    }

    pub(super) fn should_autoexecute(
        &self,
        input: ExecuteActionInput,
        ctx: &mut ModelContext<Self>,
    ) -> bool {
        let ExecuteActionInput {
            action,
            conversation_id,
        } = input;
        let AIAgentActionType::UseComputer(_) = &action.action else {
            return false;
        };

        // The pin (`42effe840`) returned `true` unconditionally here, because "this action
        // is only executed by the computer use subagent, which cannot begin without the user
        // approving it via a `RequestComputerUse` action". That premise held there because
        // the SERVER chose the tool set. It does not hold in this fork: BYOP builds the
        // tools array client-side and advertises `use_computer` alongside
        // `request_computer_use` whenever `params.computer_use_enabled` is set
        // (`agent_providers/tools/mod.rs`), and that is already true for
        // `ComputerUsePermission::AlwaysAsk` (`ai/agent/api.rs`, `execution_profiles`'
        // `is_enabled()` = "not Never/Unknown"). So the ordering the pin relied on is
        // enforced by nothing but prose in the tool description, and a model that calls
        // `use_computer` first would drive the user's real mouse and keyboard, irreversibly,
        // with no prompt, on a profile whose description promises explicit approval.
        //
        // So: keep the pin's rule but verify its premise instead of assuming it. Approval is
        // read off the conversation, where an approved `request_computer_use` leaves a
        // `RequestComputerUseResult::Approved` input (see
        // `AIConversation::has_approved_computer_use`) -- the "approval extends to the whole
        // subagent" semantics the pin describes, now actually checked.
        //
        // Returning false is not a denial: the action falls through to the normal
        // confirmation path in `try_to_execute_action`, so the unapproved case costs the user
        // one prompt -- the prompt they were promised -- rather than silent control of the
        // machine. An unknown conversation (not in memory) is treated the same way: the
        // `is_some_and` fails CLOSED, "absent" is not "ok".
        //
        // For this verdict to mean anything it has to survive
        // `try_to_execute_action`. `needs_confirmation` is `!(stood_in_for || can_auto_execute
        // || ...)`, so any caller able to stand in for the confirmation discards whatever this
        // returns. When this gate first landed, the LRC alt-screen tag-in path
        // (`queue_actions_with_options(auto_accept=true)`) reached the executor as a forged
        // `is_user_initiated=true` and did exactly that -- the gate was computed and thrown
        // away on the one path that most needed it. `ActionInitiator::AutoAcceptedTagIn` now
        // withholds that authority for `UseComputer` and `RequestComputerUse`, so on every
        // path except a literal user click on this action's own Accept button, this function's
        // answer is the one that decides. Keep the two in step: widening
        // `ActionInitiator::can_stand_in_for_confirmation` re-opens this hole silently.
        //
        // Deliberately NOT re-checked here: the profile's own always-allow setting. This
        // executor is constructed without a `terminal_view_id` (`execute.rs`:
        // `UseComputerExecutor::new()`), so it cannot resolve the right view's active
        // profile, and guessing the globally-active one could widen the gate rather than
        // narrow it. Always-allow profiles are unaffected in practice: their
        // `request_computer_use` auto-executes (`request_computer_use.rs::should_autoexecute`)
        // and records the approval this reads.
        let approved = BlocklistAIHistoryModel::as_ref(ctx)
            .conversation(&conversation_id)
            .is_some_and(|conversation| conversation.has_approved_computer_use());
        if !approved {
            log::info!(
                "[computer-use] use_computer action {:?} has no in-session approval for \
                 conversation {conversation_id:?}; routing to the confirmation prompt",
                action.id
            );
        }
        approved
    }

    pub(super) fn execute(
        &mut self,
        input: ExecuteActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> impl Into<AnyActionExecution> + use<> {
        let ExecuteActionInput {
            action,
            conversation_id,
        } = input;
        let AIAgentActionType::UseComputer(request) = &action.action else {
            return ActionExecution::InvalidAction;
        };

        let actions = request.actions.clone();
        let screenshot_params = request.screenshot_params;
        // Gate per-window targeting behind the client feature flag. When off, the actor forces the
        // legacy full-screen path so results are identical to the pre-existing implementation.
        let background_enabled = FeatureFlag::BackgroundComputerUse.is_enabled();
        // Build the actor here, in the synchronous (main-thread) body of `execute()`, and move it
        // into the async future below. On macOS, constructing the actor builds the keycode cache,
        // which calls Carbon Text Input Source APIs that assert they run on the main thread; doing
        // it inside the future would run it on a background executor thread and abort with a
        // libdispatch main-thread assertion. This mirrors `request_computer_use.rs`.
        let mut actor = computer_use::create_actor();
        // Tag this session's background-window activations with the owning conversation so its
        // teardown (on completion or cancellation) only tears down this conversation's windows.
        actor.set_background_session_owner(Some(conversation_id.to_string()));
        ActionExecution::new_async(
            async move {
                match actor
                    .perform_actions(
                        &actions,
                        computer_use::Options {
                            screenshot_params,
                            background_enabled,
                        },
                    )
                    .await
                {
                    Ok(result) => UseComputerResult::Success(result),
                    Err(error) => UseComputerResult::Error(error),
                }
            },
            |result, _ctx| AIAgentActionResultType::UseComputer(result),
        )
    }

    pub(super) fn preprocess_action(
        &mut self,
        _input: PreprocessActionInput,
        _ctx: &mut ModelContext<Self>,
    ) -> BoxFuture<'static, ()> {
        futures::future::ready(()).boxed()
    }
}

impl Entity for UseComputerExecutor {
    type Event = ();
}
