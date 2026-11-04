use crate::rules::generic::{
    cd_parent::CdParent, chmod_x::ChmodX, history::History,
    leading_shell_prompt::LeadingShellPrompt, no_command::NoCommand, python::Python,
    repetition::Repetition, ssh_add::SshAdd, sudo::Sudo,
};
use crate::rules::Rule;
use std::sync::Arc;

pub mod cd_parent;
pub mod chmod_x;
pub mod history;
pub mod leading_shell_prompt;
pub mod no_command;
pub mod python;
pub mod repetition;
pub mod ssh_add;
pub mod sudo;

pub(crate) fn rules() -> Vec<Arc<dyn Rule>> {
    vec![
        SshAdd.to_arc(),
        ChmodX.to_arc(),
        LeadingShellPrompt.to_arc(),
        Repetition.to_arc(),
        Python.to_arc(),
        CdParent.to_arc(),
        Sudo.to_arc(),
        History.to_arc(),
        NoCommand.to_arc(),
        History.to_arc(),
    ]
}
