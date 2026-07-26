//! SSH manager UI (left-side Tool Panel). Currently a skeleton, with content
//! to be implemented in Commit 2b: a tree of folders/servers plus a details
//! form on the right.
//!
//! The data layer lives in the separate `warp_ssh_manager` crate
//! (`crates/warp_ssh_manager/`).

pub mod candidates;
pub mod notifier;
pub mod onekey;
pub mod panel;
pub mod password_prompt;
pub mod secret_injector;
pub mod server_view;
pub mod shell_prompt;
pub mod startup_command_injector;
pub mod su_password_injector;

// `CandidatesViewModel` is currently only referenced by `panel.rs`;
// `CandidateRow` is merely an intermediate representation used for panel's
// internal layout and doesn't need exporting. Add a re-export if it needs to
// be consumed externally.
#[allow(unused_imports)]
pub use candidates::CandidatesViewModel;
pub use notifier::{SshTreeChangedEvent, SshTreeChangedNotifier};
pub use panel::SshManagerPanel;
// Re-exports for downstream UI consumers (Commit 2b).
#[allow(unused_imports)]
pub use panel::{SshManagerPanelAction, SshManagerPanelEvent};
