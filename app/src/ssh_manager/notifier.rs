//! Global SSH tree-changed broadcast — whenever any view modifies the tree
//! structure (add/remove/rename/change server field), it calls `notify`
//! once, and subscribers like SshManagerPanel refresh accordingly.
//!
//! Same pattern as `KeybindingChangedNotifier`
//! (`app/src/settings_view/keybindings.rs:72`): an empty struct +
//! SingletonEntity + a single Event variant.

use warpui::{Entity, SingletonEntity};

#[derive(Default)]
pub struct SshTreeChangedNotifier {}

impl SshTreeChangedNotifier {
    pub fn new() -> Self {
        Default::default()
    }
}

#[derive(Clone, Debug)]
pub enum SshTreeChangedEvent {
    /// The node list / server details have changed; list_nodes needs to be re-run.
    TreeChanged,
}

impl Entity for SshTreeChangedNotifier {
    type Event = SshTreeChangedEvent;
}

impl SingletonEntity for SshTreeChangedNotifier {}
