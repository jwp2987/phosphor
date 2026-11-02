use std::sync::{Arc, OnceLock};

use async_trait::async_trait;
use input_classifier::{
    ClassificationResult, Context, HeuristicClassifier, InputClassificationResult, InputClassifier,
    SafetyGatedClassifier,
};
use warp_completer::ParsedTokensSnapshot;
use warpui::{Entity, ModelContext, SingletonEntity};

pub struct InputClassifierModel {
    pub classifier: Arc<dyn InputClassifier>,
}

/// Picks and loads the concrete classifier. Loading the ONNX model (tokenizer parse + inference
/// session) takes ~25 ms of CPU, so [`InputClassifierModel::new`] runs this on a background thread
/// instead of the UI thread during startup.
//
// Every branch below wraps its concrete classifier in `SafetyGatedClassifier` before storing
// it: whichever model is loaded (ONNX by default, then fasttext, then this heuristic
// fallback), the safety invariant that a Shell result requires real first-token command
// evidence has to be enforced once, centrally, here — not inside any one classifier, since a
// model-specific gate is dead code the moment a different model is loaded (#696).
fn load_classifier(_nld_classifier_enabled: bool) -> Arc<dyn InputClassifier> {
    #[cfg(feature = "nld_onnx_model")]
    match input_classifier::OnnxClassifier::new(input_classifier::OnnxModel::BertTiny) {
        Ok(classifier) => {
            log::info!("Loaded onnx classifier");
            return Arc::new(SafetyGatedClassifier::new(classifier));
        }
        Err(e) => log::warn!("Failed to load onnx classifier: {e:#}"),
    }

    #[cfg(feature = "nld_fasttext_model")]
    if _nld_classifier_enabled {
        match input_classifier::FasttextClassifier::new() {
            Ok(classifier) => {
                log::info!("Loaded fasttext classifier");
                return Arc::new(SafetyGatedClassifier::new(classifier));
            }
            Err(e) => log::warn!("Failed to load fasttext classifier: {e:#}"),
        }
    }

    Arc::new(SafetyGatedClassifier::new(HeuristicClassifier))
}

/// The classifier handed out to consumers: it loads the real one once, on whichever comes first,
/// the startup warm-up thread or the first classification, and every classification after that
/// goes straight through. Same classifier and same results as loading it eagerly; only the load
/// no longer sits on the startup critical path. A classification that arrives before the load
/// finishes waits for it (it never falls back to a different classifier).
struct DeferredClassifier {
    loaded: OnceLock<Arc<dyn InputClassifier>>,
    nld_classifier_enabled: bool,
}

impl DeferredClassifier {
    fn get(&self) -> &Arc<dyn InputClassifier> {
        self.loaded
            .get_or_init(|| load_classifier(self.nld_classifier_enabled))
    }
}

#[async_trait]
impl InputClassifier for DeferredClassifier {
    async fn detect_input_type(
        &self,
        input: ParsedTokensSnapshot,
        context: &Context,
    ) -> InputClassificationResult {
        let classifier = self.get().clone();
        classifier.detect_input_type(input, context).await
    }

    async fn classify_input(
        &self,
        input: ParsedTokensSnapshot,
        context: &Context,
    ) -> anyhow::Result<ClassificationResult> {
        let classifier = self.get().clone();
        classifier.classify_input(input, context).await
    }
}

impl InputClassifierModel {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        #[cfg(feature = "nld_fasttext_model")]
        let nld_classifier_enabled = is_nld_classifier_enabled(_ctx);
        #[cfg(not(feature = "nld_fasttext_model"))]
        let nld_classifier_enabled = false;

        let deferred = Arc::new(DeferredClassifier {
            loaded: OnceLock::new(),
            nld_classifier_enabled,
        });

        // Warm it up off the UI thread so it is ready long before the first keystroke. On wasm
        // there are no threads, so the first classification loads it.
        {
            let warm_up = deferred.clone();
            if let Err(e) = std::thread::Builder::new()
                .name("input-classifier-load".to_owned())
                .spawn(move || {
                    warm_up.get();
                })
            {
                log::warn!("Could not start the classifier warm-up thread: {e}");
            }
        }

        Self {
            classifier: deferred,
        }
    }

    pub fn classifier(&self) -> Arc<dyn InputClassifier> {
        self.classifier.clone()
    }
}

impl Entity for InputClassifierModel {
    type Event = ();
}

impl SingletonEntity for InputClassifierModel {}

#[cfg(feature = "nld_fasttext_model")]
/// Returns true iff the NLD classifier model is enabled.
pub fn is_nld_classifier_enabled(ctx: &warpui::AppContext) -> bool {
    use warp_core::user_preferences::GetUserPreferences as _;
    use warp_core::{channel::ChannelState, features::FeatureFlag};

    if ChannelState::channel().is_dogfood() {
        // The `EnableNLDClassifierModel` can be used to force enable / disable
        // use if it is set.
        ctx.private_user_preferences()
            .read_value("EnableNLDClassifierModel")
            .ok()
            .flatten()
            .and_then(|s| s.parse().ok())
            .unwrap_or(FeatureFlag::NLDClassifierModelEnabled.is_enabled())
    } else {
        FeatureFlag::NLDClassifierModelEnabled.is_enabled()
    }
}

#[cfg(test)]
#[path = "input_classifier_tests.rs"]
mod tests;
