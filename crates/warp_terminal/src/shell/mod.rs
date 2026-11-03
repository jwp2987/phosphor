mod unescape;

use std::collections::{HashMap, HashSet};
use std::ops::Deref;
use std::path::{Path, PathBuf};

use anyhow::Result;
use channel_versions::overrides::TargetOS;
use enum_iterator::Sequence;
use itertools::Itertools;
use lazy_static::lazy_static;
use serde::{Deserialize, Serialize};
use smol_str::SmolStr;
use typed_path::{TypedPath, TypedPathBuf, WindowsPath};
use version_compare::{Cmp, Version};
use warp_completer::completer::{CommandExitStatus, CommandOutput};
#[cfg(windows)]
use warp_core::paths::base_config_dir;
use warp_core::platform::SessionPlatform;
use warp_util::path::{
    convert_msys2_to_windows_native_path, convert_wsl_to_windows_host_path, msys2_exe_to_root,
};

use crate::model::escape_sequences;

use self::unescape::unescape_quotes;

/// How reedline's file-backed history writes a newline inside one entry.
const NU_HISTORY_NEWLINE: &str = "<\\n>";

const ZSH_META: u8 = 0x83;

/// These are file extensions of executable files on Windows.
///
/// Commands ending with any of these extensions may be executed with the extension elided, e.g.
/// you can type `git` in a shell instead of `git.exe`.
/// This is the contents of `$env:PATHEXT` on a default Windows 11 installation. See docs:
/// https://renenyffenegger.ch/notes/Windows/development/environment-variables/PATHEXT
/// TODO(CORE-2948) Fetch this dynamically instead.
const PATHEXT: [&str; 12] = [
    ".COM", ".EXE", ".BAT", ".CMD", ".VBS", ".VBE", ".JS", ".JSE", ".WSF", ".WSH", ".MSC", ".CPL",
];

lazy_static! {
    static ref BASH_INPUT_REPORTING_MINIMUM_VERSION: Version<'static> =
        Version::from("4.0").expect("version parses successfully");
}

/// Strips the extended history prefix from a zsh history line, if present.
///
/// The zsh history can have two types of output depending on if `extended_history` mode is enabled.
/// * If enabled, the history is of the form: `: <beginning time>:<elapsed seconds>;<command>`.
/// * If not enabled, history is simply the command on each line with no additional metadata.
///
/// Whether a PSReadLine history line is one the Phosphor bootstrap typed into PowerShell itself
/// (#1122, #1203): the install guard, or the `. <path>` that sources `Phosphor/pwsh.ps1` or a
/// `.tmpXXXXXX.ps1` copy of it. The bootstrap keeps them out of the file with its own history
/// handler; files written by older versions, or under a user-replaced handler, still hold them.
fn is_phosphor_powershell_bootstrap_line(line: &str) -> bool {
    let line = line.trim();
    if line.starts_with("if (-not $global:WARP_BOOTSTRAPPED) {") {
        return true;
    }
    let Some(sourced) = line.strip_prefix(". ") else {
        return false;
    };
    let sourced = sourced.trim().trim_end_matches(['"', '\'']);
    sourced.ends_with("Phosphor/pwsh.ps1")
        || sourced.ends_with("Phosphor\\pwsh.ps1")
        || (sourced.ends_with(".ps1")
            && sourced
                .rsplit(['/', '\\'])
                .next()
                .is_some_and(|name| {
                    name.strip_prefix(".tmp").is_some_and(|rest| {
                        rest.len() == "XXXXXX.ps1".len()
                            && rest.ends_with(".ps1")
                            && rest
                                .trim_end_matches(".ps1")
                                .chars()
                                .all(|c| c.is_ascii_alphanumeric())
                    })
                }))
}

/// We avoid using a regex here in favor of simple string manipulation for better performance in this
/// hot path.
fn strip_zsh_extended_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix(": ") else {
        return line;
    };

    let Some(semi_idx) = rest.find(';') else {
        return line;
    };

    let prefix = &rest[..semi_idx];
    let Some((timestamp, elapsed)) = prefix.split_once(':') else {
        return line;
    };

    if !timestamp.is_empty()
        && timestamp.bytes().all(|b| b.is_ascii_digit())
        && !elapsed.is_empty()
        && elapsed.bytes().all(|b| b.is_ascii_digit())
    {
        &rest[semi_idx + 1..]
    } else {
        line
    }
}

/// Represents a shell and its configuration.
#[derive(Clone, Debug)]
pub struct Shell {
    shell_type: ShellType,
    version: Option<String>,
    options: Option<HashSet<String>>,

    /// Shell plugins like Powerlevel10k that we autodetect while bootstrapping.
    /// This is not at all exhaustive, it's just for common plugins that need
    /// special handling (like warning the user that they're incompatible).
    plugins: HashSet<String>,

    /// The full path to the running shell binary on the host (e.g. "/usr/bin/zsh").
    /// Populated from the `Bootstrapped` DCS payload. For local sessions this is
    /// redundant with `ShellLaunchData::executable_path`; for SSH sessions this
    /// is the authoritative path on the remote host.
    shell_path: Option<String>,
}

impl Shell {
    pub fn new(
        shell_type: ShellType,
        version: Option<String>,
        options: Option<HashSet<String>>,
        plugins: HashSet<String>,
        shell_path: Option<String>,
    ) -> Self {
        Self {
            shell_type,
            version,
            options,
            plugins,
            shell_path,
        }
    }

    pub fn shell_path(&self) -> &Option<String> {
        &self.shell_path
    }

    pub fn shell_type(&self) -> ShellType {
        self.shell_type
    }

    pub fn version(&self) -> &Option<String> {
        &self.version
    }

    pub fn options(&self) -> &Option<HashSet<String>> {
        &self.options
    }

    pub fn plugins(&self) -> &HashSet<String> {
        &self.plugins
    }

    /// Returns true if the shell requires the use of an in-band command executor.
    /// This applies to both local and remote sessions.
    pub fn force_in_band_command_executor(&self) -> bool {
        self.shell_type.force_in_band_command_executor()
    }

    /// Returns whether the current shell supports native shell completions.
    pub fn supports_native_shell_completions(&self) -> bool {
        self.shell_type.supports_native_shell_completions()
    }

    /// Whether the shell supports "autocd" (`cd`ing into a directory without specifying
    /// `cd`).
    pub fn supports_autocd(&self) -> bool {
        match self.shell_type {
            ShellType::Zsh | ShellType::Bash => self
                .options
                .as_ref()
                .is_some_and(|map| map.contains("autocd")),
            // autocd is always enabled in Fish, see https://fishshell.com/docs/current/cmds/cd.html.
            ShellType::Fish => true,
            // Typing a directory path on its own `cd`s into it in nushell.
            ShellType::Nu => true,
            ShellType::PowerShell => false,
        }
    }

    /// If the particular version of this shell supports input reporting, return the byte sequence
    /// to trigger input reporting.
    ///
    /// These sequences are bound to Zap shell functions during session bootstrap that print the
    /// shell's input buffer, wrapped within the 'InputBuffer' DCS hook when triggered. PowerShell
    /// cannot use a binding that contains the letter "i"  because it does virtual key code
    /// translation based on the current layout, and not all layouts have the letter "i".
    ///
    /// https://github.com/microsoft/terminal/blob/20588130d8ef2ba40eb56bdae88e04cce7fc5b5d/src/terminal/parser/InputStateMachineEngine.cpp#L1074-L1100
    pub fn input_reporting_sequence(&self) -> Option<[u8; 2]> {
        match self.shell_type {
            ShellType::PowerShell => Some([escape_sequences::C0::ESC, b'1']),
            ShellType::Fish | ShellType::Zsh | ShellType::Nu => {
                Some([escape_sequences::C0::ESC, b'i'])
            }
            ShellType::Bash => self
                .version
                .as_ref()
                .and_then(|version| Version::from(version.as_str()))
                .and_then(|version| {
                    version
                        .compare_to(&*BASH_INPUT_REPORTING_MINIMUM_VERSION, Cmp::Ge)
                        .then_some([escape_sequences::C0::ESC, b'i'])
                }),
        }
    }

    /// Returns `true` if the given command should be written to history based on the shell's
    /// options.
    pub fn should_add_command_to_history(&self, command: &str) -> bool {
        if command.trim().is_empty() {
            return false;
        }
        match &self.options {
            Some(options) => {
                if !command.starts_with(' ') {
                    return true;
                }

                // If the command starts with a space, check the shell's options to determine if it
                // should be added to history.
                match self.shell_type {
                    ShellType::Zsh => !options.contains("histignorespace"),
                    ShellType::Bash => {
                        // Look for our fake option that contains the value of the HISTCONTROL
                        // environment variable.
                        if let Some(histcontrol) =
                            options.iter().find(|opt| opt.starts_with("!histcontrol"))
                        {
                            // HISTCONTROL can contain a single value or a list of values separated
                            // by colons.  In either case, we want to know whether the "ignorespace"
                            // or "ignoreboth" (ignorespace+ignoredups) values are present.
                            !histcontrol.contains("ignorespace")
                                && !histcontrol.contains("ignoreboth")
                        } else {
                            true
                        }
                    }
                    // nushell skips space-prefixed lines when `history.ignore_space_prefixed` is on
                    // (its default); `nu.nu` reports that as the `ignore_space_prefixed` option.
                    ShellType::Nu => !options.contains("ignore_space_prefixed"),
                    _ => true,
                }
            }
            None => true,
        }
    }
}

/// Brief, human-readable description of a shell session.
#[derive(Clone, Debug)]
pub enum ShellName {
    /// The description isn't more descriptive than the [`ShellType`], so the [`ShellType`] should
    /// take precedent if that is known.
    LessDescriptive(String),
    /// The description is more descriptive than the [`ShellType`], so it should take precendent
    /// over the [`ShellType`] even if it is known.
    MoreDescriptive(String),
}

impl Deref for ShellName {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::MoreDescriptive(name) | Self::LessDescriptive(name) => name,
        }
    }
}

impl ShellName {
    pub fn blank() -> Self {
        Self::MoreDescriptive(String::new())
    }
}

#[derive(Copy, Clone, Debug, Hash, PartialEq, Eq, Serialize, Deserialize, Sequence)]
pub enum ShellType {
    Zsh,
    Bash,
    Fish,
    PowerShell,
    /// nushell. Structured-data shell with its own syntax (no `&&`, no `$(...)`, no backslash
    /// escaping of bare words); see `app/assets/bundled/bootstrap/nu.nu`.
    Nu,
}

impl From<ShellType> for command_corrections::Shell {
    fn from(s: ShellType) -> command_corrections::Shell {
        match s {
            ShellType::Bash => command_corrections::Shell::Bash,
            ShellType::Zsh => command_corrections::Shell::Zsh,
            ShellType::Fish => command_corrections::Shell::Fish,
            ShellType::PowerShell => command_corrections::Shell::PowerShell,
            // `command_corrections` has no nushell. Bash is the closest spelling; its
            // suggestions are only ever shown, never run on the user's behalf.
            ShellType::Nu => command_corrections::Shell::Bash,
        }
    }
}

impl From<ShellType> for warp_util::path::ShellFamily {
    fn from(value: ShellType) -> Self {
        match value {
            // nushell's quoting is not POSIX, but the family only decides the escape character
            // and the executable-name syntax; the exact quoting comes from `ShellDialect`.
            ShellType::Zsh | ShellType::Bash | ShellType::Fish | ShellType::Nu => Self::Posix,
            ShellType::PowerShell => Self::PowerShell,
        }
    }
}

impl From<ShellType> for warp_util::path::ShellDialect {
    fn from(value: ShellType) -> Self {
        match value {
            ShellType::Zsh | ShellType::Bash => Self::Posix,
            ShellType::Fish => Self::Fish,
            ShellType::PowerShell => Self::PowerShell,
            ShellType::Nu => Self::Nu,
        }
    }
}

impl ShellType {
    // Returns a shell type from a shell executable name
    pub fn from_name(name: &str) -> Option<Self> {
        // Support (/usr/bin/zsh /bin/zsh -zsh or zsh)
        if name == "bash"
            || name == "-bash"
            || name.ends_with("/bash")
            || name.ends_with("bash.exe")
        {
            Some(ShellType::Bash)
        } else if name == "zsh" || name == "-zsh" || name.ends_with("/zsh") {
            Some(ShellType::Zsh)
        } else if name == "fish" || name == "-fish" || name.ends_with("/fish") {
            Some(ShellType::Fish)
        } else if name == "nu"
            || name == "-nu"
            || name == "nushell"
            || name.ends_with("/nu")
            || name.ends_with("/nushell")
            || name.ends_with("/nu.exe")
            || name == "nu.exe"
        {
            Some(ShellType::Nu)
        } else if name == "pwsh"
            || name.ends_with("/pwsh")
            || name.ends_with("pwsh.exe")
            || name == "powershell"
            || name.ends_with("/powershell")
            || name.ends_with("powershell.exe")
        {
            Some(ShellType::PowerShell)
        } else {
            None
        }
    }

    // Returns a shell type from a markdown code block language specifier
    pub fn from_markdown_language_spec(language: &str) -> Option<Self> {
        match language {
            "bash" | "shell" | "sh" => Some(ShellType::Bash),
            "zsh" => Some(ShellType::Zsh),
            "fish" => Some(ShellType::Fish),
            "powershell" | "pwsh" => Some(ShellType::PowerShell),
            "nu" | "nushell" => Some(ShellType::Nu),
            _ => None,
        }
    }

    /// Returns locations of history files in order of search precedence
    pub fn history_files(self) -> Vec<String> {
        match self {
            ShellType::Zsh => vec!["~/.zsh_history".to_string(), "~/.zhistory".to_string()],
            ShellType::Bash => vec!["~/.bash_history".to_string()],
            ShellType::Fish => vec!["~/.local/share/fish/fish_history".to_string()],
            // The plaintext history file. Nushell keeps it in the platform config directory,
            // which is `~/Library/Application Support/nushell` on macOS and the XDG config
            // directory elsewhere. A SQLite history (`history.sqlite3`) is not readable here.
            ShellType::Nu => vec![
                "~/.config/nushell/history.txt".to_string(),
                "~/Library/Application Support/nushell/history.txt".to_string(),
            ],
            #[cfg(not(windows))]
            ShellType::PowerShell => {
                vec!["~/.local/share/powershell/PSReadLine/ConsoleHost_history.txt".to_string()]
            }
            #[cfg(windows)]
            ShellType::PowerShell => {
                vec![base_config_dir()
                    .join("Microsoft/Windows/PowerShell/PSReadLine/ConsoleHost_history.txt")
                    .display()
                    .to_string()]
            }
        }
    }

    /// Returns the potential paths to the RC file relative to the `home` directory.
    ///
    /// The returned [`TypedPathBuf`]s are encoded for the target OS rather than the
    /// host OS, because the resulting path is rendered into a shell command executed
    /// on the target (e.g. via SSH or Auto-Warpify). A plain `PathBuf` would pick the
    /// host's separator and produce strings like `~\.zshrc` on a Windows host when
    /// targeting a Unix shell, which the remote shell cannot resolve. Encoding for
    /// the target OS lets `TypedPathBuf` enforce the correct separator.
    pub fn rc_file_paths(&self, os: TargetOS) -> Vec<TypedPathBuf> {
        let is_windows = matches!(os, TargetOS::Windows);
        let home_dir = if is_windows { "$HOME" } else { "~" };
        let relative_paths: Vec<&str> = match (self, os) {
            // On Windows either edition may be in use (PowerShell 7 reads
            // `Documents\PowerShell`, Windows PowerShell 5 reads
            // `Documents\WindowsPowerShell`), so write the file to both. On Unix
            // pwsh's `$PROFILE` is the XDG path. These were swapped (byte-identical
            // at the oracle pin) until #804.
            (ShellType::PowerShell, TargetOS::Windows) => vec![
                "Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
                "Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
            ],
            (ShellType::PowerShell, _) => {
                vec![".config/powershell/Microsoft.PowerShell_profile.ps1"]
            }
            (_, TargetOS::Windows) => vec![],
            // `config.nu` under the XDG config directory. macOS keeps it under `~/Library/
            // Application Support/nushell` instead; the Phosphorize snippet therefore writes to
            // `$nu.config-path` (see `nu_subshell_bootstrap_block_output.txt`) and this path is
            // only what the success block displays.
            (ShellType::Nu, _) => vec![".config/nushell/config.nu"],
            (ShellType::Bash, _) => vec![".bashrc"],
            (ShellType::Zsh, _) => vec![".zshrc"],
            (ShellType::Fish, _) => vec![".config/fish/config.fish"],
        };
        relative_paths
            .iter()
            .map(|relative_path| {
                let mut path = if is_windows {
                    TypedPathBuf::from_windows(home_dir)
                } else {
                    TypedPathBuf::from_unix(home_dir)
                };
                // `push` uses the separator of the encoding picked above, so the
                // result follows the target OS regardless of the host.
                path.push(relative_path);
                path
            })
            .collect()
    }

    /// Returns whether the current shell supports native shell completions.
    fn supports_native_shell_completions(&self) -> bool {
        matches!(self, ShellType::Zsh)
    }

    /// Given the output of the `alias` command, returns a map of alias keys to values.
    pub fn aliases(self, alias_output: &str) -> HashMap<SmolStr, String> {
        match self {
            ShellType::Zsh => alias_output
                .lines()
                .filter_map(|line| {
                    // ZSH outputs aliases in a key pair of `name=val`, with both the name and value
                    // optionally escaped. Aliases that span multiple lines are properly escaped.

                    line.split_once('=')
                        .and_then(|(key, value)| unescape_alias_key_value(key, value))
                })
                .collect(),
            ShellType::Bash => {
                // Bash outputs aliases in a key pair of `alias name=val`. Alias values that
                // span multiple lines are not escaped. For simplicity in parsing this, append a
                // newline on the front of the string and then split on "\nalias"
                let alias_output = format!("\n{alias_output}");
                alias_output
                    .split("\nalias ")
                    .filter_map(|line| {
                        line.split_once('=')
                            .and_then(|(key, value)| unescape_alias_key_value(key, value))
                    })
                    .collect()
            }
            ShellType::Fish => {
                let alias_output = format!("\n{alias_output}");
                alias_output
                    .split("\nalias ")
                    .filter_map(|line| {
                        // Fish outputs aliases in the form:
                        // alias name val
                        // For simplicity in parsing this, append a newline on the front of
                        // the string and then split on "\nalias"
                        // Note: Currently doesn't support alias values that span multiple
                        // lines due to fish not respecting their specified format for
                        // outputing alias values with multiple lines.
                        line.split_once(' ')
                            .and_then(|(key, value)| unescape_alias_key_value(key, value))
                    })
                    .collect()
            }
            ShellType::PowerShell => alias_output
                .lines()
                .filter_map(|line| {
                    // PowerShell outputs aliases in a key pair of `name -> val`, with both the name
                    // and value optionally escaped. Aliases that span multiple lines are properly
                    // escaped.
                    line.split_once(" -> ")
                        .filter(|(_, value)| !value.trim().is_empty())
                        .and_then(|(key, value)| unescape_alias_key_value(key, value))
                })
                .collect(),
            ShellType::Nu => alias_output
                .lines()
                .filter_map(|line| {
                    // `nu.nu` writes one alias per line as `name -> expansion`, with the
                    // expansion's newlines already folded to spaces. Nushell aliases are not
                    // shell-escaped, so the text is taken verbatim rather than unescaped (an
                    // expansion containing a quote or backslash must not be mangled).
                    line.split_once(" -> ")
                        .filter(|(key, value)| !key.is_empty() && !value.trim().is_empty())
                        .map(|(key, value)| (SmolStr::from(key), value.to_owned()))
                })
                .collect(),
        }
    }

    pub fn abbreviations(self, abbr_output: &str) -> HashMap<SmolStr, String> {
        match self {
            ShellType::Fish => {
                abbr_output
                    .lines()
                    .filter_map(|line| {
                        // Fish outputs abbreviations in the form:
                        // abbr -a -U -- name val # optional comment
                        // First, strip off the comment at the end if it exists. Then,
                        // strip off the preamble characters and separate the key and value
                        // Note: The key cannot have spaces in it (fish won't allow you to define an
                        // abbreviation with a space in the name)
                        line.split_once(" #")
                            .map_or(line, |split_line| split_line.0)
                            .split_once(" -- ")
                            .and_then(|(_, abbr)| abbr.split_once(' '))
                    })
                    .filter_map(|(key, value)| unescape_alias_key_value(key, value))
                    .collect()
            }
            // Abbreviations are currently only supported in fish
            _ => HashMap::new(),
        }
    }

    /// Parse the contents of the shell's history file
    pub fn parse_history(self, history_file_bytes: &[u8]) -> Vec<String> {
        let mut history_lines: Vec<String> = Vec::new();
        match self {
            ShellType::Zsh => {
                let mut current_line = String::new();

                let unmetafied_content = zsh_unmetafy(history_file_bytes);

                for line in unmetafied_content.lines() {
                    // Only strip the extended history prefix on the first line of each command.
                    // Continuation lines are raw command text and should not be stripped.
                    let command_part = if current_line.is_empty() {
                        strip_zsh_extended_prefix(line)
                    } else {
                        line
                    };
                    current_line.push_str(command_part);

                    // ZSH considers a command to be multi-line if it ends in a backslash, see
                    // https://github.com/johan/zsh/blob/master/Src/hist.c#L2192-L2220.
                    if line.ends_with('\\') {
                        // Replace the last backslash with a new line.
                        current_line.pop();
                        current_line.push('\n');
                    } else if !current_line.is_empty() {
                        history_lines.push(current_line.clone());
                        current_line.clear();
                    }
                }
            }
            ShellType::Bash => {
                let history_file_contents = String::from_utf8_lossy(history_file_bytes);
                // Bash format if HISTTIMEFORMAT not set
                // <command>
                // Bash format if HISTTIMEFORMAT set
                // #<timestamp>
                // <command>
                history_lines = history_file_contents
                    .lines()
                    .filter_map(|line| {
                        if line.starts_with('#') || line.is_empty() {
                            None
                        } else {
                            Some(line.to_owned())
                        }
                    })
                    .collect()
            }

            ShellType::Fish => {
                let history_file_contents = String::from_utf8_lossy(history_file_bytes);
                // fish has pseudo-yaml.
                // The commands start with "- cmd: ".
                history_lines = history_file_contents
                    .lines()
                    .filter_map(|line| line.strip_prefix("- cmd: "))
                    .map(fish_unescape_history_yaml)
                    .collect()
            }

            ShellType::PowerShell => {
                // PSReadLine's ConsoleHost_history.txt (pwsh 7 and Windows PowerShell 5.1): one
                // command per line; a multi-line command is saved with every line but its last
                // ending in a backtick (#1203). Lines the Phosphor bootstrap itself typed into the
                // shell are not the user's history.
                let history_file_contents = String::from_utf8_lossy(history_file_bytes);
                let mut current = String::new();
                for line in history_file_contents.lines() {
                    if let Some(continued) = line.strip_suffix('`') {
                        current.push_str(continued);
                        current.push('\n');
                        continue;
                    }
                    current.push_str(line);
                    if !current.is_empty() && !is_phosphor_powershell_bootstrap_line(&current) {
                        history_lines.push(current.clone());
                    }
                    current.clear();
                }
                if !current.is_empty() && !is_phosphor_powershell_bootstrap_line(&current) {
                    history_lines.push(current);
                }
            }

            ShellType::Nu => {
                let history_file_contents = String::from_utf8_lossy(history_file_bytes);
                // Nushell's plaintext history (reedline's file-backed history) is one entry per
                // line, with the newlines inside a multi-line entry written as the 4 characters
                // `<\n>`.
                history_lines = history_file_contents
                    .lines()
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| s.replace(NU_HISTORY_NEWLINE, "\n"))
                    .collect()
            }
        }

        history_lines
    }

    /// Bytes used to notify the shell to delete the current buffer.
    ///
    /// In zsh this is implemented by killing the ZLE buffer and must match the `bindkey` call in
    /// the boostrap `zsh.sh`. In bash this is implemented via a custom `bind` that calls
    /// `kill-whole-line` and must match the `bind` in `bash.sh`.  In fish this is a custom `bind`
    /// that clears the command line. PowerShell cannot use a binding that contains the letter "p"
    /// (DLE maps to ctrl-p) because it does virtual key code translation based on the current
    /// layout, and not all layouts have the letter "p".
    ///
    /// https://github.com/microsoft/terminal/blob/20588130d8ef2ba40eb56bdae88e04cce7fc5b5d/src/terminal/parser/InputStateMachineEngine.cpp#L1074-L1100
    pub fn kill_buffer_bytes(self) -> &'static [u8] {
        const POWERSHELL_BINDING: [u8; 2] = [escape_sequences::C0::ESC, b'2'];
        const OTHER_BINDING: [u8; 1] = [escape_sequences::C0::DLE];
        match self {
            ShellType::PowerShell => POWERSHELL_BINDING.as_slice(),
            // `nu.nu` binds Ctrl-P to `edit: clear` in every edit mode, replacing reedline's
            // history-up binding (input goes through Phosphor's editor, as with fish).
            ShellType::Zsh | ShellType::Bash | ShellType::Fish | ShellType::Nu => {
                OTHER_BINDING.as_slice()
            }
        }
    }

    /// Bytes used to execute a command, once the command text is sent
    pub fn execute_command_bytes(self) -> &'static [u8] {
        match self {
            ShellType::Bash | ShellType::Zsh => &b"\n"[..],
            ShellType::PowerShell => &b"\r"[..],
            // Reedline submits on CR, the byte the Enter key sends.
            ShellType::Nu => &b"\r"[..],
            // For Fish, we send an extra space, immediately followed by backspace, and then
            // the newline character. The backspace ensures that any autosuggestions are
            // suppressed, so we don't get erroneous ghosted autosuggestion text in the command
            // grid.
            ShellType::Fish => &b" \x7f\n"[..],
        }
    }

    /// The shell whose syntax commands that Phosphor runs for itself (generators, probes, chip
    /// commands) are written in. They are executed through `sh -c` / `bash -c` rather than typed
    /// into the user's line editor, so for nushell, whose own syntax shares almost nothing
    /// with them, that is bash. Commands that are typed into the user's prompt (a pre-filled
    /// `cd`, `git checkout`) use the shell's own type instead.
    pub fn executor_syntax(self) -> ShellType {
        match self {
            ShellType::Nu => ShellType::Bash,
            other => other,
        }
    }

    /// The name of the shell
    pub fn name(self) -> &'static str {
        match self {
            ShellType::Zsh => "zsh",
            ShellType::Bash => "bash",
            ShellType::Fish => "fish",
            ShellType::PowerShell => "pwsh",
            ShellType::Nu => "nu",
        }
    }

    /// If true, Zap will bootstrap the shell if it's the login shell on the remote host.
    pub fn is_fully_supported_remotely(&self) -> bool {
        match self {
            ShellType::Zsh | ShellType::Bash => true,
            ShellType::Fish | ShellType::PowerShell | ShellType::Nu => false,
        }
    }

    pub fn shell_command_to_get_executables(&self) -> &'static str {
        match self {
            ShellType::Bash => {
                // Since `compgen -c` returns more than just executables (and the output itself
                // doesn't include any info about what the type of the word is), we filter down to
                // executable "file"s. Note that we do this at the shell level since if we did this
                // as a post-process step, it would require N filesystem calls, which would not scale
                // well for remote sessions. Additionally, we invoke `type` once, passing it the full
                // list of commands, to avoid a lot of overhead invoking it thousands of times for
                // systems with a lot of installed commands.
                r#"COMMANDS=($(compgen -c)); TYPES=($(type -t ${COMMANDS[@]})); for i in "${!COMMANDS[@]}"; do if [[ ${TYPES[$i]} == "file" ]]; then echo ${COMMANDS[$i]}; fi; done"#
            }
            ShellType::Fish => {
                // Although `complete -C` returns more than just executables, we don't check the type here
                // since the output of `complete` already tells us what is an executable ('command') and what isn't
                // (whereas the output of `compgen` doesn't so we need to check the `type` there).
                // Instead we post-process the output below.  We try to use the `--escape` argument, but
                // if we're running a version of fish that doesn't support it, try again without it.
                "complete -C --escape '' || complete -C ''"
            }
            ShellType::Zsh => {
                // zsh is cool and has an API for just fetching executables
                "builtin print -l -- ${(ok)commands}"
            }
            // Every in-band and local command Phosphor runs is POSIX `sh` syntax (the nushell
            // executors run it through `sh -c`), so this lists the executable files on `$PATH`
            // with shell builtins only: no per-file process, and nothing nushell-specific to
            // parse. Nushell's own commands are reported by `nu.nu` at bootstrap.
            ShellType::Nu => {
                r#"IFS=:; for d in $PATH; do [ -d "$d" ] || continue; for f in "$d"/*; do if [ -f "$f" ] && [ -x "$f" ]; then printf '%s\n' "${f##*/}"; fi; done; done"#
            }
            ShellType::PowerShell => {
                // PowerShell does not deal in strings, but in Objects and Object lists
                // Get-Command and Select-Object each return a list of Objects. In the shell,
                // this will print one item per line. However, when it is converted to a string,
                // it will join the entries together with a space. So to make sure we get one item
                // per line, we explicitly join the results with a newline.
                //
                // We then write that joined text to stdout as explicit UTF-8 bytes rather than
                // letting PowerShell encode it with the machine's active code page. On a
                // localized Windows install (say GBK with Chinese executable names) the code-page
                // bytes are not valid UTF-8, `CommandOutput::to_string` fails, and
                // `executables_from_shell_command_output` discards the *entire* list — after
                // which every valid command keeps rendering as unknown for the life of the
                // session. Matches `shell_command_to_get_all_functions` /
                // `shell_command_to_get_all_builtins` below, which write stdout the same way.
                "$names = Get-Command -CommandType Application | \
                Select-Object -ExpandProperty Name; \
                $text = [string]::Join([Environment]::NewLine, $names); \
                $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($text); \
                [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)"
            }
        }
    }

    /// Returns a Vec containing executable commands parsed from the given `output`.
    ///
    /// If `output` is `Err(..)`, returns an empty Vec.
    pub fn executables_from_shell_command_output(
        &self,
        output: Result<CommandOutput>,
        is_msys2: bool,
    ) -> Vec<SmolStr> {
        match output {
            Ok(command_output) if command_output.status == CommandExitStatus::Success => {
                let Ok(output_string) = command_output.to_string() else {
                    return Vec::new();
                };
                match self {
                    ShellType::Bash | ShellType::Zsh => {
                        // For bash and zsh, we wrote the command such that the output is just
                        // a list of executable files.
                        if !is_msys2 {
                            return output_string.lines().map(Into::into).collect();
                        }

                        // TODO add this to fish
                        output_string
                            .lines()
                            // Remove all `.dll` files.
                            .filter(|line| !line.to_lowercase().ends_with("dll"))
                            .flat_map(|line| {
                                // Those suffixes are contained in `PATHEXT`.
                                for ext in PATHEXT {
                                    // If the command ends with one of those suffixes, tell
                                    // Zap about this command as-is and also sans-suffix, e.g.
                                    // "git" and "git.exe".
                                    if line.to_lowercase().ends_with(&ext.to_lowercase()) {
                                        let trimmed = &line[..line.len() - ext.len()];
                                        return Box::<[&str]>::from([trimmed, line]);
                                    }
                                }

                                // Otherwise, pass it through unaltered.
                                Box::<[&str]>::from([line])
                            })
                            .map_into()
                            .collect()
                    }
                    ShellType::Fish => {
                        // This is the post-processing for Fish explained above.
                        output_string
                            .lines()
                            .filter_map(|line| {
                                line.split_once(char::is_whitespace).and_then(
                                    |(command, command_type)| {
                                        let is_executable = command_type == "command"
                                            || command_type == "command link"
                                            || command_type.starts_with("Executable");
                                        is_executable.then_some(command.into())
                                    },
                                )
                            })
                            .collect()
                    }
                    ShellType::Nu => output_string.lines().map_into().collect(),
                    ShellType::PowerShell => {
                        // Windows allows certain suffixes, e.g. "exe", to be elided.
                        if cfg!(windows) {
                            output_string
                                .lines()
                                .flat_map(|line| {
                                    // Those suffixes are contained in `PATHEXT`.
                                    for ext in PATHEXT {
                                        // If the command ends with one of those suffixes, tell
                                        // Zap about this command as-is and also sans-suffix, e.g.
                                        // "git" and "git.exe".
                                        if line.to_lowercase().ends_with(&ext.to_lowercase()) {
                                            let trimmed = &line[..line.len() - ext.len()];
                                            return Box::<[&str]>::from([trimmed, line]);
                                        }
                                    }
                                    // Otherwise, pass it through unaltered.
                                    Box::<[&str]>::from([line])
                                })
                                .map_into()
                                .collect()
                        } else {
                            output_string.lines().map_into().collect()
                        }
                    }
                }
            }
            Ok(output) => {
                log::warn!("Generator for executable names failed");
                if let Ok(output_string) = output.to_string() {
                    log::warn!("{output_string}");
                };
                Vec::new()
            }
            Err(e) => {
                log::warn!("Generator for executable names failed: {e:#}");
                Vec::new()
            }
        }
    }

    /// The in-band shell command that enumerates *every* function name visible
    /// in a bootstrapped session, or `None` when the shell has no need of one.
    ///
    /// Only PowerShell needs one. Bash, Zsh and Fish each report their complete
    /// function list during bootstrap itself — `compgen -A function`,
    /// `builtin print -l -- ${(ok)functions}` and `functions -an` respectively
    /// (see `app/assets/bundled/bootstrap/{bash_body.sh,zsh_body.sh,fish.sh}`)
    /// — so a second enumeration would return names the session already has.
    /// Inventing a command for them would cost an extra in-band round trip per
    /// session and buy nothing, which is why this is a per-shell method rather
    /// than one constant.
    ///
    /// The `Warp` prefix filter is deliberate and must not be rebranded: it
    /// excludes the shell integration's *own* helper functions, which are still
    /// declared as `Warp-*` in `pwsh.ps1`. Matching on any other prefix would
    /// leak `Warp-Precmd` and friends into the user's completions.
    pub fn shell_command_to_get_all_functions(&self) -> Option<&'static str> {
        match self {
            ShellType::PowerShell => Some(
                // The `Warp` prefix here names this fork's own `Warp-*` helper
                // functions in `pwsh.ps1`, not the product; see the doc comment
                // above. Renaming it would leak `Warp-Precmd` and friends into
                // the user's completions.
                // brand-guard: allow matches this fork's own `Warp-*` pwsh helpers, not the product
                "$names = Get-Command -CommandType Function | Where-Object { \
                -not $_.Name.StartsWith('Warp') } | Select-Object -ExpandProperty Name; \
                $text = [string]::Join([Environment]::NewLine, $names); \
                $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($text); \
                [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)",
            ),
            // Nushell reports its commands in the Bootstrapped hook (`scope commands`).
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Nu => None,
        }
    }

    /// The in-band shell command that enumerates *every* builtin (cmdlet) name
    /// visible in a bootstrapped session, or `None` when the shell has no need
    /// of one.
    ///
    /// As with [`Self::shell_command_to_get_all_functions`], only PowerShell
    /// needs one: `compgen -b`, `builtin print -l -- ${(ok)builtins}` and
    /// `builtin -n` already deliver the full set at bootstrap for Bash, Zsh and
    /// Fish.
    pub fn shell_command_to_get_all_builtins(&self) -> Option<&'static str> {
        match self {
            ShellType::PowerShell => Some(
                "$names = Get-Command -CommandType Cmdlet | Select-Object -ExpandProperty Name; \
                $text = [string]::Join([Environment]::NewLine, $names); \
                $bytes = [System.Text.UTF8Encoding]::new($false).GetBytes($text); \
                [Console]::OpenStandardOutput().Write($bytes, 0, $bytes.Length)",
            ),
            // Nushell reports its commands in the Bootstrapped hook (`scope commands`).
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Nu => None,
        }
    }

    pub fn force_in_band_command_executor(&self) -> bool {
        // TODO: Remove this function once we have confidence in using a local executor in
        // powershell.
        false
    }
}

/// Provides the necessary info to be able to launch and bootstrap the selected AvailableShell. For
/// executables, this is the path to the executable and the shell type. For WSL, this is the distro
/// name. For Docker sandboxes, this is the `sbx` CLI path plus the base Docker
/// image; the shell inside the container is whatever the image provides.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ShellLaunchData {
    Executable {
        executable_path: PathBuf,
        shell_type: ShellType,
    },
    /// Windows Subsystem for Linux.
    #[cfg_attr(not(windows), allow(dead_code))]
    WSL { distro: String },
    MSYS2 {
        executable_path: PathBuf,
        shell_type: ShellType,
    },
    /// A shell running inside a `sbx`-managed Docker sandbox container.
    ///
    /// A dedicated variant ensures callers can't accidentally execute the
    /// sandbox as if it were a regular local shell: the `sbx` binary at
    /// `sbx_path` is not a shell, it's the CLI we use to enter the container.
    DockerSandbox {
        sbx_path: PathBuf,
        /// Base Docker image to use when creating the sandbox (passed as
        /// `sbx run --template <image>`). `None` means "use sbx's default
        /// image".
        base_image: Option<String>,
    },
}

impl ShellLaunchData {
    /// Converts the given path string to a OS-native PathBuf, performing any necessary shell-informed conversions.
    pub fn maybe_convert_absolute_path(&self, path_str: &str) -> Option<PathBuf> {
        match self {
            ShellLaunchData::Executable { .. } => Some(PathBuf::from(path_str)),
            ShellLaunchData::WSL { distro } => {
                let unix_path = TypedPath::unix(path_str);
                convert_wsl_to_windows_host_path(&unix_path, distro).ok()
            }
            ShellLaunchData::MSYS2 {
                executable_path, ..
            } => {
                let unix_path = TypedPath::unix(path_str);
                convert_msys2_to_windows_native_path(
                    &unix_path,
                    &msys2_exe_to_root(WindowsPath::new(
                        executable_path.as_os_str().as_encoded_bytes(),
                    )),
                )
                .ok()
            }
            // Paths inside the sandbox container are plain Unix paths.
            ShellLaunchData::DockerSandbox { .. } => Some(PathBuf::from(path_str)),
        }
    }

    /// Converts a shell-encoded [`typed_path::TypedPathBuf`] into an OS-native path.
    fn maybe_convert_shell_encoded_path(
        &self,
        shell_encoded_path: TypedPathBuf,
    ) -> Option<PathBuf> {
        match self {
            ShellLaunchData::Executable { .. } | ShellLaunchData::DockerSandbox { .. } => {
                PathBuf::try_from(shell_encoded_path).ok()
            }
            ShellLaunchData::WSL { distro } => {
                convert_wsl_to_windows_host_path(&shell_encoded_path.to_path(), distro).ok()
            }
            ShellLaunchData::MSYS2 {
                executable_path, ..
            } => convert_msys2_to_windows_native_path(
                &shell_encoded_path.to_path(),
                &msys2_exe_to_root(WindowsPath::new(
                    executable_path.as_os_str().as_encoded_bytes(),
                )),
            )
            .ok(),
        }
    }

    /// Naively changes the path string to an OS-native encoding, without performing shell-informed conversions.
    fn to_native_path_encoding(&self, path_str: &str) -> Option<PathBuf> {
        match self {
            ShellLaunchData::Executable { .. } => Some(PathBuf::from(path_str)),
            ShellLaunchData::WSL { .. } | ShellLaunchData::MSYS2 { .. } => {
                let windows_encoding = TypedPath::unix(path_str).with_windows_encoding();
                PathBuf::try_from(windows_encoding).ok()
            }
            // The container is Unix; Zap runs on the host, so paths are
            // already in the host's native encoding. Pass through unchanged.
            ShellLaunchData::DockerSandbox { .. } => Some(PathBuf::from(path_str)),
        }
    }

    /// Converts a path string to a shell's encoding.
    fn to_shell_encoding<'a>(&self, path_str: &'a str) -> TypedPath<'a> {
        match self {
            ShellLaunchData::Executable { .. } => {
                if cfg!(unix) {
                    TypedPath::unix(path_str)
                } else {
                    TypedPath::windows(path_str)
                }
            }
            ShellLaunchData::WSL { .. }
            | ShellLaunchData::MSYS2 { .. }
            | ShellLaunchData::DockerSandbox { .. } => TypedPath::unix(path_str),
        }
    }

    /// Attempts to append the relative path to the base path and convert it into a OS-native PathBuf,
    /// performing any necessary shell-informed conversions.
    pub fn maybe_convert_relative_path(
        &self,
        base_path_str: &str,
        relative_path_str: &str,
    ) -> Option<PathBuf> {
        let base_path = self.to_shell_encoding(base_path_str);
        let rest_of_path = self.to_shell_encoding(relative_path_str);
        let absolute_typed_path = base_path.join(rest_of_path);
        self.maybe_convert_shell_encoded_path(absolute_typed_path)
    }

    /// Joins the given path string to the base path, ensuring the given path string is encoded correctly.
    pub fn join_to_native_path(&self, base_path: &Path, path_str: &str) -> Option<PathBuf> {
        self.to_native_path_encoding(path_str)
            .map(|rest_of_path| base_path.join(rest_of_path))
    }

    /// How to present this data to the user for error messages.
    pub fn shell_detail(&self) -> String {
        match self {
            Self::Executable {
                executable_path, ..
            }
            | Self::MSYS2 {
                executable_path, ..
            } => executable_path.to_string_lossy().into_owned(),
            Self::WSL { distro } => distro.to_owned(),
            Self::DockerSandbox { base_image, .. } => match base_image {
                Some(image) => format!("Docker sandbox ({image})"),
                None => "Docker sandbox".to_owned(),
            },
        }
    }
}

impl From<ShellLaunchData> for SessionPlatform {
    fn from(data: ShellLaunchData) -> Self {
        match data {
            ShellLaunchData::Executable { .. } => SessionPlatform::Native,
            ShellLaunchData::WSL { .. } => SessionPlatform::WSL,
            ShellLaunchData::MSYS2 { .. } => SessionPlatform::MSYS2,
            ShellLaunchData::DockerSandbox { .. } => SessionPlatform::DockerSandbox,
        }
    }
}

/// Unescape the key and value for an alias, returning None if either fails
fn unescape_alias_key_value(key: &str, value: &str) -> Option<(SmolStr, String)> {
    let key = unescape_quotes(key)
        .map_err(|e| {
            log::error!("Unable to unescape key for alias: {e}");
            e
        })
        .ok()?;
    let value = unescape_quotes(value)
        .map_err(|e| {
            log::error!("Unable to unescape value for alias: {e}");
            e
        })
        .ok()?;
    Some((key.into(), value))
}

// fish history replaces newlines with \n, and \ with \\.
// This does the reverse.
fn fish_unescape_history_yaml(line: &str) -> String {
    let mut result = String::new();
    result.reserve(line.len());
    let mut escaped = false;
    for ch in line.chars() {
        match ch {
            '\\' => {
                if escaped {
                    result.push('\\');
                }
                escaped = !escaped;
            }
            'n' if escaped => {
                result.push('\n');
                escaped = false;
            }
            _ => {
                result.push(ch);
                escaped = false;
            }
        }
    }
    result
}

// Basically a translation of this function into rust:
// http://mika.l3ib.org/code/unmetafy.c
// To unmetafy a string from zsh's internal format, we need to skip each meta symbol
// and XOR the next symbol with 32.
fn zsh_unmetafy(content: &[u8]) -> String {
    let mut unmetafied = Vec::new();

    match content.last() {
        None => "".into(),
        Some(byte) => {
            let mut following_byte = *byte;

            content.iter().rev().skip(1).for_each(|current_byte| {
                if *current_byte == ZSH_META {
                    following_byte ^= 32;
                } else {
                    unmetafied.push(following_byte);
                    following_byte = *current_byte;
                }
            });

            unmetafied.push(following_byte);
            unmetafied.reverse();
            String::from_utf8_lossy(&unmetafied).into()
        }
    }
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
