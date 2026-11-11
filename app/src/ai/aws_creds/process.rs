//! `credential_process`: run the configured command and read its JSON credentials from stdout
//! (`{"Version": 1, "AccessKeyId": ..., "SecretAccessKey": ..., "SessionToken": ..., "Expiration": ...}`).
//!
//! The command's stdout and stderr can hold secrets, so neither is ever logged or kept in an
//! error: failures report the exit code only.

use std::time::Duration;

use futures::future::{Either, select};
use serde_json::Value;

use super::{Context, Creds, CredsError, imds::parse_json_credentials};

const PROCESS_TIMEOUT: Duration = Duration::from_secs(30);

/// Splits a command line the way the AWS CLI does: `shlex` (POSIX rules) on unix, the Windows C
/// runtime's rules on Windows ([`split_windows_command`]).
pub(crate) fn split_command(line: &str) -> Option<Vec<String>> {
    #[cfg(windows)]
    {
        split_windows_command(line)
    }
    #[cfg(not(windows))]
    {
        shlex::split(line)
    }
}

/// The AWS CLI's Windows split (botocore `_windows_shell_split`, the C runtime's argument
/// rules). Only double quotes group, and `""` is an empty word. A single quote is an ordinary
/// character. A backslash is an ordinary character (a path separator) unless a run of them
/// stands before a double quote: 2n backslashes and a quote are n backslashes and a grouping
/// quote, 2n+1 are n backslashes and a literal quote. `None` when a quote is left open.
#[cfg(any(windows, test))]
pub(crate) fn split_windows_command(line: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut started = false;
    let mut backslashes = 0usize;
    for c in line.chars() {
        match c {
            '\\' => backslashes += 1,
            '"' => {
                current.extend(std::iter::repeat_n('\\', backslashes / 2));
                let literal = backslashes % 2 == 1;
                backslashes = 0;
                started = true;
                if literal {
                    current.push('"');
                } else {
                    in_quotes = !in_quotes;
                }
            }
            c => {
                if backslashes > 0 {
                    current.extend(std::iter::repeat_n('\\', backslashes));
                    backslashes = 0;
                    started = true;
                }
                if c.is_whitespace() && !in_quotes {
                    if started {
                        parts.push(std::mem::take(&mut current));
                        started = false;
                    }
                } else {
                    current.push(c);
                    started = true;
                }
            }
        }
    }
    if in_quotes {
        return None;
    }
    if backslashes > 0 {
        current.extend(std::iter::repeat_n('\\', backslashes));
        started = true;
    }
    if started {
        parts.push(current);
    }
    Some(parts)
}

pub(crate) async fn credentials(ctx: &Context, command_line: &str) -> Result<Creds, CredsError> {
    let parts = split_command(command_line)
        .filter(|p| !p.is_empty())
        .ok_or_else(|| {
            CredsError::InvalidConfig("credential_process is not a valid command".into())
        })?;

    let mut command = command::r#async::Command::new(&parts[0]);
    command
        .args(&parts[1..])
        .stdin(command::Stdio::null())
        .kill_on_drop(true);
    let output = command.output();
    let timer = warpui::r#async::Timer::after(PROCESS_TIMEOUT);
    futures::pin_mut!(output);
    futures::pin_mut!(timer);
    let output = match select(output, timer).await {
        Either::Left((result, _)) => result
            .map_err(|_| CredsError::Provider("credential_process could not be started".into()))?,
        // Dropping the pending output future kills the child (`kill_on_drop`).
        Either::Right(_) => {
            return Err(CredsError::Timeout("credential_process timed out".into()));
        }
    };
    if !output.status.success() {
        let code = output
            .status
            .code()
            .map(|c| c.to_string())
            .unwrap_or_else(|| "a signal".to_string());
        return Err(CredsError::Provider(format!(
            "credential_process failed with exit status {code}"
        )));
    }

    let stdout = String::from_utf8(output.stdout)
        .map_err(|_| CredsError::Provider("credential_process output is not UTF-8".into()))?;
    let value: Value = serde_json::from_str(&stdout)
        .map_err(|_| CredsError::Provider("credential_process output is not valid JSON".into()))?;
    if value.get("Version").and_then(Value::as_i64) != Some(1) {
        return Err(CredsError::InvalidConfig(
            "credential_process must report Version 1".into(),
        ));
    }
    let creds = parse_json_credentials(&stdout)?;
    if creds.expires_at.is_some_and(|exp| exp <= ctx.now()) {
        return Err(CredsError::Expired(
            "credential_process returned expired credentials".into(),
        ));
    }
    Ok(creds)
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
