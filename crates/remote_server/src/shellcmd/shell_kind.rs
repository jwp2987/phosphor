//! Which family of shell the remote login shell is, and how to hand it a POSIX script.
//!
//! ssh runs the remote command through the user's LOGIN shell. A POSIX-family login shell
//! (bash, zsh, dash, ash, ksh) runs a POSIX script as is. fish and nushell do not parse POSIX
//! syntax, so the script is passed to `sh -c` with the quoting that shell needs. PowerShell
//! (Windows hosts) gets PowerShell source instead, sent as `-EncodedCommand` so no quoting layer
//! can touch it.

use super::base64;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ShellKind {
    /// bash, zsh, dash, ash, ksh, sh: runs a POSIX script directly.
    Posix,
    Fish,
    Nu,
    /// PowerShell (`pwsh` / `powershell`), on Windows hosts.
    Pwsh,
}

impl ShellKind {
    /// Classifies a shell by name or path (`/usr/bin/fish`, `zsh`, `pwsh.exe`). `None` for a
    /// shell this does not know (csh, tcsh, ...): the caller then reports the shell backend
    /// unavailable for the session rather than guessing.
    pub fn from_shell_name(name: &str) -> Option<Self> {
        let base = name
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(name)
            .trim_start_matches('-')
            .to_ascii_lowercase();
        let base = base.strip_suffix(".exe").unwrap_or(&base);
        match base {
            "sh" | "bash" | "zsh" | "dash" | "ash" | "ksh" | "mksh" | "busybox" => {
                Some(Self::Posix)
            }
            "fish" => Some(Self::Fish),
            "nu" | "nushell" => Some(Self::Nu),
            "pwsh" | "powershell" => Some(Self::Pwsh),
            _ => None,
        }
    }

    /// Whether builders must emit PowerShell rather than POSIX source for this shell.
    pub fn is_powershell(self) -> bool {
        matches!(self, Self::Pwsh)
    }

    /// The remote command string that runs `script` under this kind of login shell. `script` is
    /// POSIX source for `Posix`/`Fish`/`Nu` and PowerShell source for `Pwsh`.
    pub fn wrap(self, script: &str) -> String {
        match self {
            Self::Posix => script.to_owned(),
            Self::Fish => format!("sh -c '{}'", fish_single_quote_body(script)),
            Self::Nu => {
                let hashes = "#".repeat(raw_string_hashes(script));
                format!("^sh -c r{hashes}'{script}'{hashes}")
            }
            Self::Pwsh => {
                let utf16: Vec<u8> = script
                    .encode_utf16()
                    .flat_map(|unit| unit.to_le_bytes())
                    .collect();
                format!(
                    "pwsh -NoProfile -NonInteractive -EncodedCommand {}",
                    base64::encode(&utf16)
                )
            }
        }
    }
}

/// The inside of a fish single-quoted string: only `\` and `'` are special there.
fn fish_single_quote_body(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\'', "\\'")
}

/// How many `#` a nushell raw string needs so that `'` followed by that many `#` does not occur
/// in `s` (and so cannot close the string early).
fn raw_string_hashes(s: &str) -> usize {
    let mut needed = 1;
    let bytes = s.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'\'' {
            let run = bytes[i + 1..].iter().take_while(|&&c| c == b'#').count();
            needed = needed.max(run + 1);
        }
    }
    needed
}

#[cfg(test)]
#[path = "shell_kind_tests.rs"]
mod tests;
