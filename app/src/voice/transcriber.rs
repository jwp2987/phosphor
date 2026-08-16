use std::sync::Arc;

use async_trait::async_trait;
use warpui::{Entity, SingletonEntity};

#[derive(thiserror::Error, Debug)]
pub enum TranscribeError {
    #[error("Request failed due to lack of Voice quota.")]
    QuotaLimit,

    #[error("Phosphor is currently overloaded. Please try again later.")]
    ServerOverloaded,

    #[error("Internal error occurred at transport layer.")]
    Transport,

    #[error("Failed to deserialize JSON.")]
    Deserialization,

    /// Zap has disabled voice transcription (the BYOP genai protocol can't carry audio).
    #[error("Voice transcription is unavailable in Phosphor.")]
    Disabled,

    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Interface for transcribing voice input.
#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait Transcriber: Send + Sync {
    /// Transcribe the given base64 encoded wav file into text.
    /// This is expected to be async and called off the main thread.
    async fn transcribe(&self, wav_base64: String) -> Result<String, TranscribeError>;
}

/// A voice transcriber that is enabled or disabled.
///
/// This is a singleton model that the app can decide to enable or disable.
/// The editor does expect that it will exist as a singleton fetchable from app context
/// either way though, and depending on whether the optional transcriber is set,
/// the editor considers transcriber to be enabled or disabled.
///
/// We set it up this way to avoid the editor having a direct dependency on any server api.
pub struct VoiceTranscriber {
    /// The transcriber to use. If `None`, the transcriber is disabled.
    #[cfg_attr(not(feature = "voice_input"), allow(dead_code))]
    transcriber: Option<Arc<dyn Transcriber>>,
}

impl VoiceTranscriber {
    pub fn new(transcriber: Arc<dyn Transcriber>) -> Self {
        Self {
            transcriber: Some(transcriber),
        }
    }

    /// Zap (localization, Phase 4): creates a disabled transcriber.
    /// Originally, `Some(...)` meant a cloud STT backend was available and
    /// `None` meant "transcriber disabled"; after localization, the cloud
    /// `ServerVoiceTranscriber` (which calls server_api.transcribe to send
    /// Wispr STT) is unavailable, so this constructor is used instead.
    pub fn disabled() -> Self {
        Self { transcriber: None }
    }

    /// Returns the transcriber if one is set.
    pub fn transcriber(&self) -> Option<&Arc<dyn Transcriber>> {
        self.transcriber.as_ref()
    }
}

impl Entity for VoiceTranscriber {
    type Event = ();
}

impl SingletonEntity for VoiceTranscriber {}

/// Whether a real [`Transcriber`] implementation exists in this build.
///
/// This is hard-coded `false`: the only implementation Warp ships,
/// `ServerVoiceTranscriber`, calls `server_api.transcribe` to send audio to
/// Warp's cloud Wispr speech-to-text service, which this fork does not have
/// (the BYOP genai protocol cannot carry audio). `app/src/lib.rs` constructs
/// `VoiceTranscriber::disabled()` accordingly, so `git grep 'impl Transcriber
/// for'` returns nothing in this tree. Audio capture itself
/// (`crates/voice_input`) is real and unaffected -- only the step that turns
/// captured audio into text has no backend.
///
/// This becomes `true` only once a local/BYOP transcription engine (e.g. a
/// bundled Whisper-class model) is wired up as a real `Transcriber` impl and
/// injected in place of `VoiceTranscriber::disabled()`. Until then, settings
/// UI that depends on this should stay hidden rather than offer a control
/// that can never do anything -- see `AppAnalyticsWidget::should_render` in
/// `app/src/settings_view/privacy_page.rs` for the identical pattern applied
/// to telemetry.
pub fn voice_transcription_available() -> bool {
    false
}

#[cfg(test)]
#[path = "transcriber_tests.rs"]
mod tests;
