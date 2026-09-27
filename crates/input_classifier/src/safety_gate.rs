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
    InputClassifierDecisionSource, InputType, util::first_token_forces_ai_override,
};

/// Wraps any [`InputClassifier`] and enforces a hard safety invariant on its `detect_input_type`
/// result: a `Shell` classification is overridden back to `AI` when the buffer's effective first
/// token has no command evidence and is itself an ordinary, capitalized English word (see
/// [`first_token_forces_ai_override`]). This holds regardless of which concrete classifier
/// produced the result, since it inspects only the buffer and the final `InputType`, never the
/// inner classifier's reasoning.
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
        let result = self.inner.detect_input_type(input.clone(), context).await;
        if matches!(result.input_type, InputType::Shell) && first_token_forces_ai_override(&input) {
            return InputClassificationResult::new(
                InputType::AI,
                InputClassifierDecisionSource::NoFirstTokenCommandEvidence,
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
