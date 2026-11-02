//! Server-side experiment identifiers.
//!
//! The client no longer applies server-side experiment state: the `ServerExperiments` model
//! (and with it the cached-row replay that could flip `FeatureFlag::AgentMode` off) was
//! removed. Only the enum and its string conversion remain because the `server_experiments`
//! sqlite table and `ModelEvent::SaveExperiments` still read and write it; dropping those
//! needs a diesel migration decision.

mod convert;

/// The known server-side experiments.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq)]
pub enum ServerExperiment {
    DisableAgentModeExperiment,
    EnvVarsEarlyAccessExperiment,
    AgentModeAnalyticsExperiment,
    WindowsLaunchExperiment,
    TmuxSshWarpificationControl,
    TmuxSshWarpificationExperiment,
    SuggestedCodeDiffsControl,
    SuggestedCodeDiffsExperiment,
    PromptSuggestionsViaMaaControl,
    PromptSuggestionsViaMaaExperiment,
    PromptSuggestionsViaMaaOutOfBandExperiment,
    OzMultiHarnessControl,
    OzMultiHarnessExperiment,
}
