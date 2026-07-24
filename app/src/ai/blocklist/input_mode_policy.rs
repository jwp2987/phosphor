//! Per-view input-mode policy consulted for decisions the input model cannot make
//! view-agnostically.
//!
//! Ported from Warp OSS. BYOP adaptation: Zap has no `InputTypeAutoDetectionSource` (a
//! telemetry-only decision tag), so `PolicyConfigUpdate` carries only the config and the
//! autodetection-suppression flag, and the source-recording `with_source` constructor is
//! dropped.

use std::rc::Rc;

use warpui::AppContext;

use super::conversation_selection::ConversationSelectionEvent;
use super::input_model::InputConfig;
use crate::settings::AISettingsChangedEvent;

/// A config write produced by an [`InputModePolicy`] decision.
pub struct PolicyConfigUpdate {
    /// The config to apply.
    pub config: InputConfig,
    /// Whether to briefly suppress autodetection before applying.
    pub temporarily_disable_autodetection: bool,
}

impl PolicyConfigUpdate {
    /// An update with no autodetection suppression.
    pub fn new(config: InputConfig) -> Self {
        Self {
            config,
            temporarily_disable_autodetection: false,
        }
    }
}

/// Per-view policy consulted by the input model for decisions it cannot make
/// view-agnostically: lock gating, the autodetection setting for the surface's current
/// context, and reactive config transitions driven by conversation-selection and settings
/// events.
pub trait InputModePolicy: 'static {
    /// The config the surface starts with.
    fn initial_config(&self, app: &AppContext) -> InputConfig;

    /// Whether the input may currently be locked to AI. When this returns `false`,
    /// `{AI, locked}` config writes are rejected.
    fn allows_locked_ai_input(&self, app: &AppContext) -> bool;

    /// Whether NL autodetection is enabled for the surface's current context.
    fn is_autodetection_enabled(&self, app: &AppContext) -> bool;

    /// The config to apply in response to a conversation-selection event, or `None` to
    /// leave the config unchanged.
    fn config_on_conversation_selection_changed(
        &self,
        event: &ConversationSelectionEvent,
        current: InputConfig,
        app: &AppContext,
    ) -> Option<PolicyConfigUpdate>;

    /// The config to apply when AI settings change, or `None` to leave the config unchanged.
    fn config_on_ai_settings_changed(
        &self,
        event: &AISettingsChangedEvent,
        current: InputConfig,
        is_autodetection_enabled_for_current_context: bool,
        app: &AppContext,
    ) -> Option<PolicyConfigUpdate>;
}

/// Shared handle to a view-supplied [`InputModePolicy`].
pub type InputModePolicyHandle = Rc<dyn InputModePolicy>;
