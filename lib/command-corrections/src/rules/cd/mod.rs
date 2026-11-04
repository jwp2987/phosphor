use lazy_static::lazy_static;
use regex::Regex;

use crate::rules::cd::{cd_correction::CdCorrection, cd_mkdir::CdMkdir};

use crate::rules::Rule;

use super::CommandGroup;
use crate::{Command, PlatformType, SessionMetadata};
mod cd_correction;
mod cd_mkdir;

pub(crate) fn command_group() -> CommandGroup {
    CommandGroup::new(
        &["cd", "sl", "chdir", "set-location"],
        vec![CdCorrection.to_arc(), CdMkdir.to_arc()],
    )
}

lazy_static! {
    static ref RE_PWSH_CD_DOESNT_EXIST: Regex = Regex::new(
        "(?i)(cd|chdir|sl|set-location): Cannot find path '.*' because it does not exist."
    )
    .unwrap();

    // TODO: handle -L/-P options
    static ref RE_CD_INPUT_COMMAND: Regex =
        Regex::new("(?i)(?<command>cd|chdir|sl|set-location) (?<directory>.+)").unwrap();
}

/// Returns true iff the command's output matches the output of a cd command
/// when the argument (directory) doesn't exist.
/// Note: cd corrections don't work well for remote sessions yet (since we don't support
/// dynamic rules over SSH), so don't offer them for non-local sessions.
// TODO: eventually, use a callback to just check if the directory exists
fn matches_cd_doesnt_exist_sh_posix(command: &Command, session_metadata: &SessionMetadata) -> bool {
    let lowercase_output = command.lowercase_output();
    session_metadata.session_type.is_local()
        && (lowercase_output.contains("does not exist")
            || lowercase_output.contains("no such file or directory"))
}

fn matches_cd_doesnt_exist_pwsh(command: &Command, session_metadata: &SessionMetadata) -> bool {
    session_metadata.session_type.is_local() && RE_PWSH_CD_DOESNT_EXIST.is_match(command.output)
}

fn matches_cd_doesnt_exist(command: &Command, session_metadata: &SessionMetadata) -> bool {
    use crate::Shell;
    match (&session_metadata.shell, &session_metadata.platform_type) {
        (Shell::PowerShell, PlatformType::Windows) => {
            matches_cd_doesnt_exist_pwsh(command, session_metadata)
        }
        (Shell::PowerShell, PlatformType::Posix) => {
            matches_cd_doesnt_exist_sh_posix(command, session_metadata)
                || matches_cd_doesnt_exist_pwsh(command, session_metadata)
        }
        (_, _) => matches_cd_doesnt_exist_sh_posix(command, session_metadata),
    }
}

fn parse_command_and_directory<'a>(command: &'a Command) -> Option<(&'a str, &'a str)> {
    let captures = RE_CD_INPUT_COMMAND.captures(command.input)?;
    let command = captures.name("command")?;
    let directory = captures.name("directory")?;
    Some((command.as_str(), directory.as_str()))
}

#[cfg(test)]
// On Posix systems, the powershell error message will will always
// list the command as `Set-Location`, while windows will reflect
// back the original alias.
fn get_pwsh_error_command_for_platform(input_cmd: &str, platform_type: PlatformType) -> &str {
    match platform_type {
        PlatformType::Posix => "Set-Location",
        PlatformType::Windows => input_cmd,
    }
}
