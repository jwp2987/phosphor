// Zap Wave 7-2: `first_time_setup` was physically removed along with the ambient
// agent UI.
mod harness_selector;
mod host_selector;
mod model;
mod model_selector;
mod progress_ui_state;
mod tips;

pub use harness_selector::{HarnessSelector, HarnessSelectorAction, HarnessSelectorEvent};
pub use host_selector::{
    Host, HostSelector, HostSelectorAction, HostSelectorEvent, NakedHeaderButtonTheme,
};
pub use model::{AgentProgress, AmbientAgentViewModel, AmbientAgentViewModelEvent, Status};
pub use model_selector::{ModelSelector, ModelSelectorAction, ModelSelectorEvent};
pub use progress_ui_state::AmbientAgentProgressUIState;
pub use tips::{AmbientAgentTip, get_ambient_agent_tips};
