//! A single, classifier-independent safety net for Agent-input classification (#696).
//!
//! `app/src/input_classifier.rs::InputClassifierModel::new` picks whichever concrete classifier
//! is available at startup — the ONNX model, then fasttext, then
//! [`HeuristicClassifier`](crate::HeuristicClassifier) as the last-resort fallback — and every one
//! of them has its own independent path to a `Shell` result. A safety invariant implemented inside
//! only one of them (e.g. `HeuristicClassifier`) is dead code the moment a different classifier is
//! loaded, which is exactly what ships by default (the ONNX model is on by default). So the
//! invariant has to be enforced once, centrally, at the one place the app actually consumes a
//! classifier's output — which is what [`SafetyGatedClassifier`] does by wrapping *any*
//! [`InputClassifier`] and post-processing every result it returns.

use async_trait::async_trait;
use warp_completer::ParsedTokensSnapshot;

use crate::{
    ClassificationResult, Context, InputClassificationResult, InputClassifier,
    InputClassifierDecisionSource, InputType,
    util::{
        first_token_forces_ai_override, is_unambiguous_shell_command, known_command_head,
        reads_as_request_to_agent,
    },
};

/// Wraps any [`InputClassifier`] and enforces a hard safety invariant on its `detect_input_type`
/// result: a `Shell` classification is overridden back to `AI` when the buffer's effective first
/// token has no command evidence and is itself an ordinary English word, matched
/// case-insensitively (see [`first_token_forces_ai_override`]). This holds regardless of which
/// concrete classifier produced the result, since it inspects only the buffer, the `Context`, and
/// the final `InputType`, never the inner classifier's reasoning.
pub struct SafetyGatedClassifier<C> {
    inner: C,
}

impl<C> SafetyGatedClassifier<C> {
    pub fn new(inner: C) -> Self {
        Self { inner }
    }
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
impl<C: InputClassifier> InputClassifier for SafetyGatedClassifier<C> {
    async fn detect_input_type(
        &self,
        input: ParsedTokensSnapshot,
        context: &Context,
    ) -> InputClassificationResult {
        // #808: an unambiguous shell command line never reaches a classifier at all, so neither
        // a model's opinion of its words nor the prose override below can send it to the agent.
        // `BlocklistAIInputModel::detect_and_set_input_type` runs the same check synchronously
        // before it even spawns the classification task; this one covers every other consumer
        // of a classifier (e.g. the `evaluate` tool).
        if is_unambiguous_shell_command(&input) {
            return InputClassificationResult::new(
                InputType::Shell,
                InputClassifierDecisionSource::UnambiguousShellSyntax,
            );
        }
        let result = self.inner.detect_input_type(input.clone(), context).await;
        if matches!(result.input_type, InputType::Shell)
            && first_token_forces_ai_override(&input, context.commands_fully_loaded)
        {
            return InputClassificationResult::new(
                InputType::AI,
                InputClassifierDecisionSource::NoFirstTokenCommandEvidence,
            );
        }
        // #834: the head is a real command, so the check above cannot fire, but the words after
        // it read as a request (`help me`, `watch the logs for errors`). Pipelines, lists, heredocs
        // and one-off keyword heads (`echo`, `sudo`) are exempt inside the predicate.
        if matches!(result.input_type, InputType::Shell)
            && reads_as_request_to_agent(&input.buffer_text)
        {
            return InputClassificationResult::new(
                InputType::AI,
                InputClassifierDecisionSource::ProseAfterCommandHead,
            );
        }
        // #906: the other direction. A classifier that says AI for a line whose command the
        // session knows (`grep this that`, `make the build`, `nohup grep this the file`) is
        // vetoed unless the line reads as a request. Unknown heads, session data that cannot say
        // keep the classifier's verdict, so prose still routes.
        if matches!(result.input_type, InputType::AI)
            && known_command_head(&input)
            && !reads_as_request_to_agent(&input.buffer_text)
        {
            return InputClassificationResult::new(
                InputType::Shell,
                InputClassifierDecisionSource::KnownCommandHeadNotARequest,
            );
        }
        result
    }

    async fn classify_input(
        &self,
        input: ParsedTokensSnapshot,
        context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        // `classify_input` is a lower-level "give me a raw probability" API with no external
        // production callers (only `detect_input_type`, and the offline `evaluate` bin tool);
        // gating the final `InputType` decision at `detect_input_type` is what matters for safety.
        self.inner.classify_input(input, context).await
    }
}

#[cfg(test)]
#[path = "safety_gate_tests.rs"]
mod tests;
