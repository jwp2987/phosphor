pub mod render;
pub mod settings;
pub mod success_block;
pub mod trigger_state;

use crate::terminal::model::terminal_model::SubshellInitializationInfo;
use crate::terminal::shell::ShellType;
use crate::ASSETS;
use channel_versions::overrides::TargetOS;
use warpui::AssetProvider;

#[derive(Debug)]
pub enum WarpificationSource {
    Ssh,
    Subshell,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum SubshellSource {
    Command(String),
    EnvVarCollection(String),
}

/// The word that names a subshell in the chip, tab, flag and separator: the first word of the
/// spawning command that is not a leading `NAME=value` assignment or an `env` prefix
/// (`AUTOP=1 zsh`, `ZDOTDIR=/x zsh`, `env -i FOO=1 zsh` all name `zsh`) (#1037).
pub fn subshell_display_name(spawning_command: &str) -> Option<&str> {
    let mut words = spawning_command.split_whitespace().peekable();
    let mut in_env = false;
    while let Some(word) = words.peek().copied() {
        if is_env_assignment_word(word) {
            words.next();
        } else if !in_env && word == "env" && words.clone().nth(1).is_some() {
            in_env = true;
            words.next();
        } else if in_env && (word.starts_with('-') && word != "-") {
            // `env` options such as `-i`. Options taking a value (`-u NAME`, `-C dir`) also
            // consume the next word.
            words.next();
            if matches!(word, "-u" | "-C" | "-S") {
                words.next();
            }
        } else {
            break;
        }
    }
    words.next()
}

/// What the block-side subshell flag shows for `source`: a command is reduced to its
/// [`subshell_display_name`], so `AUTOP=1 zsh` is flagged `zsh`, matching the chip (#1037).
pub fn subshell_flag_source(source: &SubshellSource) -> SubshellSource {
    match source {
        SubshellSource::Command(cmd) => subshell_display_name(cmd)
            .map(|name| SubshellSource::Command(name.to_owned()))
            .unwrap_or_else(|| source.clone()),
        SubshellSource::EnvVarCollection(_) => source.clone(),
    }
}

fn is_env_assignment_word(word: &str) -> bool {
    let Some((name, _)) = word.split_once('=') else {
        return false;
    };
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c == '_' || c.is_ascii_alphabetic())
        && chars.all(|c| c == '_' || c.is_ascii_alphanumeric())
}

/// This template is for the snippet that appears in the output grid for the success block if the
/// subshell is local.
fn get_subshell_bootstrap_success_block_path(shell_type: ShellType) -> Option<&'static str> {
    match shell_type {
        ShellType::Bash | ShellType::Zsh => {
            Some("bundled/bootstrap/bash_zsh_subshell_bootstrap_block_output.txt")
        }
        ShellType::Fish => Some("bundled/bootstrap/fish_subshell_bootstrap_block_output.txt"),
        // nushell's `config.nu` snippet is nushell syntax and runs `^uname` at the next start.
        ShellType::Nu => Some("bundled/bootstrap/nu_subshell_bootstrap_block_output.txt"),
        // `uname` is a real external command under pwsh on Unix (the focus of #800), so the
        // same "bake the current uname in, leave the rest of the snippet literal for next
        // startup" trick the bash/zsh/fish templates use works here too. Not exercised for
        // Windows PowerShell, where `uname` is not generally available.
        ShellType::PowerShell => Some("bundled/bootstrap/pwsh_subshell_bootstrap_block_output.txt"),
    }
}

/// Returns OutputGrid bytes to be rendered in the hardcoded "Warpified subshell" block that's added
/// to the blocklist upon successful subshell bootstrap.
///
/// The exact block contents varies based on whether or not the session is local or remote, in
/// addition to the given `shell_type`.
pub fn subshell_bootstrap_success_block_bytes(
    subshell_initialization_info: &SubshellInitializationInfo,
    shell_type: ShellType,
    os: TargetOS,
    disable_tmux: bool,
) -> (Vec<u8>, bool) {
    let from_env_var_collection = subshell_initialization_info
        .env_var_collection_name
        .is_some();

    if from_env_var_collection {
        return (vec![], false);
    }

    let Some(subshell_bootstrap_success_block_path) =
        get_subshell_bootstrap_success_block_path(shell_type)
    else {
        return ("".into(), false);
    };

    let templated_subshell_bootstrap_success_block_output_bytes = ASSETS
        .get(subshell_bootstrap_success_block_path)
        .unwrap_or_else(|_| {
            panic!("Failed to retrieve {subshell_bootstrap_success_block_path} from assets.")
        })
        .to_vec();

    let rc_file_paths = shell_type.rc_file_paths(os);
    let mut is_executable = true;
    let commands: Vec<Vec<u8>> = rc_file_paths
        .iter()
        .map(|rc_file_path| {
            let rc_file_path = rc_file_path.to_str();
            is_executable &= rc_file_path.is_some();
            replace_template_chars_with_arguments(
                templated_subshell_bootstrap_success_block_output_bytes
                    .trim_ascii_end()
                    .to_owned()
                    .to_vec(),
                vec![
                    shell_type.name().to_owned(),
                    if disable_tmux {
                        ", \"tmux\": false"
                    } else {
                        ""
                    }
                    .to_owned(),
                    rc_file_path.unwrap_or("<Your RC file>").to_owned(),
                ],
            )
        })
        .collect();
    // `shell_type.rc_file_paths(os)` returns more than one path for PowerShell on
    // non-Windows `os` (it writes to both the PowerShell Core and Windows PowerShell
    // profile locations), so `commands` can have more than one element there. Each
    // per-path command is a complete, terminator-free statement (e.g. a single
    // `Add-Content ... -Path '<path>'` invocation): naively concatenating two of them
    // glues the end of one onto the start of the next with no statement separator,
    // which pwsh parses as a single malformed command (confirmed: it reports
    // "parameter 'Value' is specified more than once" and appends to neither profile).
    // A newline is a valid statement separator for every shell family this function
    // supports (bash/zsh/fish *and* PowerShell), and is a no-op here whenever there is
    // only one path, which is the case for every shell but PowerShell today.
    (commands.join(&b"\n"[..]), is_executable)
}

/// The "Run the following to automatically Phosphorize" command for a PowerShell subshell
/// whose shell runs on a Windows host (Windows PowerShell 5.1 or pwsh 7). Unlike the POSIX
/// flavour it targets `$PROFILE` (whatever the running edition reads, and which does not
/// live under `~/.config`), creates the profile first when absent (`Add-Content` fails if
/// its directory is missing), and bakes in a constant `uname` instead of running `uname`,
/// which does not exist on Windows. Kept free of `cfg` so it is testable on every host.
pub(crate) fn windows_powershell_subshell_success_block_bytes(disable_tmux: bool) -> Vec<u8> {
    let template = ASSETS
        .get("bundled/bootstrap/pwsh_windows_subshell_bootstrap_block_output.txt")
        .unwrap_or_else(|_| panic!("Failed to retrieve the Windows PowerShell success block."))
        .to_vec();
    replace_template_chars_with_arguments(
        template.trim_ascii_end().to_vec(),
        vec![
            ShellType::PowerShell.name().to_owned(),
            if disable_tmux {
                ", \"tmux\": false"
            } else {
                ""
            }
            .to_owned(),
        ],
    )
}

/// Replaces each instance of '%' in the given `templated_bytes` vector with `String` in
/// `arguments`, in order.
///
/// The bundled block content txt files are templated using '%' as a placeholder to be dynamically
/// replaced at runtime. This is useful to cater the exact block contents to the bootstrapped
/// subshell.
fn replace_template_chars_with_arguments(
    mut templated_bytes: Vec<u8>,
    arguments: Vec<String>,
) -> Vec<u8> {
    // This was an arbitrarily chosen character.
    const TEMPLATE_CHAR: u8 = b'%';

    for argument in arguments {
        let template_i = templated_bytes.iter().position(|b| b == &TEMPLATE_CHAR);
        if let Some(template_i) = template_i {
            templated_bytes.splice(
                template_i..template_i + 1,
                argument.into_bytes().into_iter(),
            );
        } else {
            debug_assert!(false, "Number of arguments does not match number of template chars (%) in hardcoded subshell block bytes.");
        }
    }
    templated_bytes
}

#[cfg(test)]
#[path = "mod_test.rs"]
mod tests;
