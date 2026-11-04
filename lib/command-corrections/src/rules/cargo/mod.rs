use crate::rules::cargo::cargo_build::Cargo;
use crate::rules::cargo::cargo_no_command::CargoNoCommand;
use crate::rules::cargo::cargo_options::CargoOption;
use crate::rules::Rule;

use super::CommandGroup;

mod cargo_build;
mod cargo_no_command;
mod cargo_options;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(
        &["cargo"],
        vec![
            CargoNoCommand.to_arc(),
            CargoOption.to_arc(),
            Cargo.to_arc(),
        ],
    )
}
