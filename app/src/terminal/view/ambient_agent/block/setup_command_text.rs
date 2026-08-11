// No current caller: the pin's `CloudModeSetupTextBlock` view that drives this state
// was not ported (see docs/sweep/outcome-tail.md), so this state machine is currently
// unused outside its own tests. Kept `pub`/allowed-dead so a future block view can pick
// it up without re-deriving it.
#![allow(dead_code)]

use std::collections::HashMap;

/// Identifies one group of ambient-agent setup commands. Groups are created every time
/// a new batch of setup commands starts running (e.g. after a harness restart), so that
/// each batch's expanded/collapsed and running state can be tracked independently of any
/// other batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SetupCommandGroupId(u64);

impl SetupCommandGroupId {
    fn initial() -> Self {
        Self(0)
    }
}

/// Tracks the expanded/collapsed and running state of ambient-agent setup command
/// groups. Ported from the pin (02b53fcd8) leaf-only: the pin's `CloudModeSetupTextBlock`
/// view wiring around this state is not ported here — see docs/sweep/outcome-tail.md.
#[derive(Debug, Clone)]
pub struct SetupCommandState {
    did_execute_a_setup_command: bool,
    current_group_id: SetupCommandGroupId,
    next_group_id: u64,
    expanded_groups: HashMap<SetupCommandGroupId, bool>,
    running_group_id: Option<SetupCommandGroupId>,
}

impl Default for SetupCommandState {
    fn default() -> Self {
        let current_group_id = SetupCommandGroupId::initial();
        let mut expanded_groups = HashMap::new();
        expanded_groups.insert(current_group_id, true);
        Self {
            did_execute_a_setup_command: false,
            current_group_id,
            next_group_id: 1,
            expanded_groups,
            running_group_id: Some(current_group_id),
        }
    }
}

impl SetupCommandState {
    pub fn current_group_id(&self) -> SetupCommandGroupId {
        self.current_group_id
    }
    pub fn did_execute_a_setup_command(&self) -> bool {
        self.did_execute_a_setup_command
    }

    pub fn set_did_execute_a_setup_command(&mut self, value: bool) {
        self.did_execute_a_setup_command = value;
    }

    pub fn should_expand(&self, group_id: SetupCommandGroupId) -> bool {
        self.expanded_groups.get(&group_id).copied().unwrap_or(true)
    }

    pub fn set_should_expand(&mut self, group_id: SetupCommandGroupId, value: bool) {
        self.expanded_groups.insert(group_id, value);
    }

    pub fn is_running(&self, group_id: SetupCommandGroupId) -> bool {
        self.running_group_id == Some(group_id)
    }

    pub fn should_suppress_input_sync_for_current_group(&self) -> bool {
        self.current_group_id != SetupCommandGroupId::initial()
            && self.is_running(self.current_group_id)
    }

    pub fn start_new_group(&mut self) -> SetupCommandGroupId {
        let group_id = SetupCommandGroupId(self.next_group_id);
        self.next_group_id += 1;
        self.current_group_id = group_id;
        self.did_execute_a_setup_command = false;
        self.expanded_groups.insert(group_id, true);
        self.running_group_id = Some(group_id);
        group_id
    }

    pub fn finish_group(&mut self, group_id: SetupCommandGroupId) {
        if self.running_group_id == Some(group_id) {
            self.running_group_id = None;
        }
    }
}

#[cfg(test)]
#[path = "setup_command_text_tests.rs"]
mod tests;
