use std::sync::Arc;

use input_classifier::{HeuristicClassifier, InputClassifier, SafetyGatedClassifier};
use warpui::{Entity, ModelContext, SingletonEntity};

pub struct InputClassifierModel {
    pub classifier: Arc<dyn InputClassifier>,
}

impl InputClassifierModel {
    // Every branch below wraps its concrete classifier in `SafetyGatedClassifier` before storing
    // it: whichever model is loaded (ONNX by default, then fasttext, then this heuristic
    // fallback), the safety invariant that a Shell result requires real first-token command
    // evidence has to be enforced once, centrally, here — not inside any one classifier, since a
    // model-specific gate is dead code the moment a different model is loaded (#696).
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        #[cfg(feature = "nld_onnx_model")]
        match input_classifier::OnnxClassifier::new(input_classifier::OnnxModel::BertTiny) {
            Ok(classifier) => {
                log::info!("Loaded onnx classifier");
                return Self {
                    classifier: Arc::new(SafetyGatedClassifier::new(classifier)),
                };
            }
            Err(e) => log::warn!("Failed to load onnx classifier: {e:#}"),
        }

        #[cfg(feature = "nld_fasttext_model")]
        if is_nld_classifier_enabled(_ctx) {
            match input_classifier::FasttextClassifier::new() {
                Ok(classifier) => {
                    log::info!("Loaded fasttext classifier");
                    return Self {
                        classifier: Arc::new(SafetyGatedClassifier::new(classifier)),
                    };
                }
                Err(e) => log::warn!("Failed to load fasttext classifier: {e:#}"),
            }
        }

        Self {
            classifier: Arc::new(SafetyGatedClassifier::new(HeuristicClassifier)),
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
