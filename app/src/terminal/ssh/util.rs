use lazy_static::lazy_static;
use regex::Regex;

/// Converts a multiline bash or zsh script to one line by turning newlines into semicolons or
/// deleting them, as appropriate.
///
/// Extra semicolons are a syntax error in bash, so this is careful to avoid adding them except
/// where necessary.
///
/// This function exists because there's a strange macOS ssh server bug where sending a lot of data
/// containing newlines to a shell results in data corruption.
pub fn convert_script_to_one_line(script: &str) -> String {
    lazy_static! {
        static ref EXTRA_SPACES_REGEX: Regex = Regex::new(r"\n+\s*").expect("invalid regex");
        static ref NO_SEMICOLON_REGEX: Regex =
            Regex::new("(; ?|\\{|do|then|else|in)\n").expect("invalid regex");
        static ref REMOVE_COMMENTS_REGEX: Regex = Regex::new(r"(?m)^ *#.*").expect("invalid regex");
        static ref REMOVE_LEADING_NEWLINES: Regex = Regex::new(r"^\n*").expect("invalid regex");
    };
    let script = REMOVE_COMMENTS_REGEX.replace_all(script, "");
    let script = REMOVE_LEADING_NEWLINES.replace_all(&script, "");
    let script = EXTRA_SPACES_REGEX.replace_all(&script, "\n");
    let script = NO_SEMICOLON_REGEX.replace_all(&script, "$1 ");
    let mut script = script.replace('\n', ";");
    script.push('\n');
    script
}

#[derive(Debug, PartialEq, Eq)]
pub enum SshLoginState {
    LastLogin,
    NonSshOutput,
    Authenticating,
    PromptDetected,
}

/// Reads the contents of the output grid to determine SSH login state. Returns [SshLoginState::LastLogin] if
/// "Last login:" is detected in the output. Returns [SshLoginState::NonSshOutput] if certain keywords
/// known to be a part of ssh login prompts are found in the current last line of command output. The
/// "password" and "Password" are for password authentication. "passphrase" is intended to cover authentication
/// by public key. And "yes/no" relates to trust-on-first-use prompts for host-based authentication.
pub fn check_ssh_login_state(block_output: &str) -> SshLoginState {
    lazy_static! {
        // Common final prompt characters followed by a space.
        static ref PROMPT_REGEX: Regex = Regex::new(r"[$#%>❯│⟫»▶λ→] $").expect("invalid regex");
    };

    let mut last_line = None;

    for line in block_output.lines() {
        if line.starts_with("Last login:") {
            return SshLoginState::LastLogin;
        }
        // With an iterator, there's no way to know if it's the last element so
        // we overwrite last_line at each iteration.
        last_line = Some(line);
    }

    last_line.map_or(SshLoginState::Authenticating, |line| {
        if line.contains("password")
            || line.contains("Password")
            || line.contains("passphrase")
            || line.contains("yes/no")
            || line.contains("Please type")
            || line.contains("'yes'")
            || line.contains("Confirm user presence")
            || line.starts_with("Enter ")
            || line.starts_with("Allow ")
            // #1374: ssh prints this and then re-prompts (possibly many seconds later, while the
            // server delays its reply). It is a mid-authentication line, never "login done".
            || line.contains("Permission denied, please try again")
        {
            SshLoginState::Authenticating
        } else if PROMPT_REGEX.is_match(line) {
            SshLoginState::PromptDetected
        } else {
            SshLoginState::NonSshOutput
        }
    })
}

/// Represents the parsed components of an interactive SSH command.
/// For some [`SshWarpifyCommand`]s, we do not support parsing
/// a host or port In these cases, we can still parse to a valid
/// empty `InteractiveSshCommand` to indicate that we did
/// successfully detect an interactive SSH command.
#[derive(Clone, Debug, Default)]
pub struct InteractiveSshCommand {
    pub host: Option<String>,
    pub port: Option<String>,
}

impl InteractiveSshCommand {
    /// Parses ssh commands of the form `ssh ...`.
    /// Only returns an `InteractiveSshCommand` if we determine the command is interactive.
    fn parse_ssh_command(command: &str) -> Option<InteractiveSshCommand> {
        let command = if let Some(suffix) = command.strip_prefix("command ") {
            suffix
        } else {
            command
        };
        // A glued or separate shell operator (`ssh host&`, `ssh host;`, a pipe) is not an
        // interactive session on the pty.
        // Only an UNQUOTED operator counts: `-o ProxyCommand='sleep 3; false'` is one ssh command
        // and must arm the connect gate (#1395).
        if has_unquoted_shell_operator(command) {
            return None;
        }
        let tokens = parse_ssh_command_tokens(command)?;
        let mut host: Option<String> = None;
        let mut port: Option<String> = None;

        let mut i = 1;
        while i < tokens.len() {
            match tokens[i].as_str() {
                // -T or -W imply a non-interactive session.
                "-T" | "-W" => return None,

                // #1035: -w (tun device), -M (master) and -S (control socket) cannot ride the
                // wrapper's multiplexed session; the shell wrappers hand them to plain ssh. Only
                // the standalone forms are recognised here: a clustered `-4vw host` has always
                // parsed as interactive (see ssh_interactive_shell_parsing).
                "-w" | "-M" | "-S" => return None,

                // -O (control command), -Q (query): never an interactive session.
                "-O" | "-Q" => return None,

                "-p" => {
                    i += 1;
                    if i < tokens.len() {
                        port = Some(tokens[i].clone());
                    } else {
                        return None;
                    }
                }

                // SSH option that doesn't change interactivity and require an argument: Skip the next item.
                "-B" | "-b" | "-c" | "-D" | "-E" | "-e" | "-F" | "-I" | "-i" | "-J" | "-L"
                | "-l" | "-m" | "-o" | "-P" | "-R" => {
                    i += 1;
                }

                // SSH option(s) that don't change interactivity.
                // A cluster of flags is scanned up to the first option that takes an argument.
                // -N (no command), -f (background), -G (print config), -V (version), -T, -W, -O
                // and -Q never open an interactive session.
                arg if arg.starts_with('-') && !arg.starts_with("--") => {
                    for flag in arg.chars().skip(1) {
                        match flag {
                            'N' | 'f' | 'G' | 'V' | 'T' | 'W' | 'O' | 'Q' | 'M' => return None,
                            'B' | 'b' | 'c' | 'D' | 'E' | 'e' | 'F' | 'I' | 'i' | 'J' | 'L'
                            | 'l' | 'm' | 'o' | 'P' | 'p' | 'R' | 'S' | 'w' => break,
                            _ => {}
                        }
                    }
                }

                // Long-form or otherwise unknown options don't change interactivity.
                arg if arg.starts_with('-') => {}

                // Otherwise, it's a positional argument (e.g., hostname, command to run)
                pos_arg => {
                    // If we detect multiple positional args, there's some type of unknown command formulation.
                    if host.is_some() {
                        return None;
                    }
                    host = Some(pos_arg.to_string());
                }
            }
            i += 1;
        }

        Some(InteractiveSshCommand { host, port })
    }
}

pub enum SshLikeCommand {
    Gcloud,
    ElasticBeanstalk,
    DigitalOceanDroplet,
}

/// TMUX SSH Warpification can be triggered by any command that
/// we determine to be an interactive SSH command. This enum
/// represents the different types of SSH commands we support
/// for TMUX Warpification. `Ssh` means a literal `ssh` command,
/// where all other commands are categorized as SSH-like commands.
pub enum SshWarpifyCommand {
    Ssh,
    SshLike(SshLikeCommand),
}

impl SshWarpifyCommand {
    /// Not a literal `ssh` command, but another command that starts an interactive SSH
    /// session that we can Warpify with TMUX.
    pub fn is_ssh_like_command(&self) -> bool {
        matches!(self, SshWarpifyCommand::SshLike(_))
    }
}

lazy_static! {
    static ref INTERACTIVE_SSH: Regex = Regex::new(r"^ssh\s+").expect("interactive SSH regex invalid");

    /// Matches "gcloud compute ssh" for connecting to GCP VMs.
    static ref GCLOUD_REGEX: Regex = Regex::new(r"^gcloud\s+compute\s+ssh\s.+").expect("gcloud SSH regex invalid");

    /// Matches "eb ssh" for connecting to AWS Elastic Beanstalk VMs.
    static ref ELASTIC_BEANSTALK_REGEX: Regex = Regex::new(r"^eb\s+ssh\s.+").expect("elastic beanstalk SSH regex invalid");

    /// Matches "doctl compute ssh" for connecting to a digital ocean droplet.
    static ref DIGITAL_OCEAN_DROPLET_REGEX: Regex = Regex::new(r"^doctl\s+compute\s+ssh\s.+").expect("digital ocean SSH regex invalid");
}

impl SshWarpifyCommand {
    pub fn matches(command: &str) -> Option<SshWarpifyCommand> {
        let command = if let Some(suffix) = command.strip_prefix("command ") {
            suffix
        } else {
            command
        };
        if INTERACTIVE_SSH.is_match(command) {
            Some(SshWarpifyCommand::Ssh)
        } else if GCLOUD_REGEX.is_match(command) {
            Some(SshWarpifyCommand::SshLike(SshLikeCommand::Gcloud))
        } else if ELASTIC_BEANSTALK_REGEX.is_match(command) {
            Some(SshWarpifyCommand::SshLike(SshLikeCommand::ElasticBeanstalk))
        } else if DIGITAL_OCEAN_DROPLET_REGEX.is_match(command) {
            Some(SshWarpifyCommand::SshLike(
                SshLikeCommand::DigitalOceanDroplet,
            ))
        } else {
            None
        }
    }
}

pub fn parse_interactive_ssh_command(command: &str) -> Option<InteractiveSshCommand> {
    match SshWarpifyCommand::matches(command) {
        Some(SshWarpifyCommand::Ssh) => InteractiveSshCommand::parse_ssh_command(command),
        Some(SshWarpifyCommand::SshLike(SshLikeCommand::Gcloud)) => {
            Some(InteractiveSshCommand::default())
        }
        Some(SshWarpifyCommand::SshLike(SshLikeCommand::ElasticBeanstalk)) => {
            Some(InteractiveSshCommand::default())
        }
        Some(SshWarpifyCommand::SshLike(SshLikeCommand::DigitalOceanDroplet)) => {
            Some(InteractiveSshCommand::default())
        }
        None => None,
    }
}

/// Whether `command` has a shell control operator (`&`, `;`, `|`) outside single quotes, double
/// quotes and backslash escapes.
fn has_unquoted_shell_operator(command: &str) -> bool {
    let mut quote: Option<char> = None;
    let mut chars = command.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (None, '\\') | (Some('"'), '\\') => {
                chars.next();
            }
            (None, '\'' | '"') => quote = Some(c),
            (Some(q), _) if c == q => quote = None,
            (None, '&' | ';' | '|') => return true,
            _ => {}
        }
    }
    false
}

fn parse_ssh_command_tokens(command: &str) -> Option<Vec<String>> {
    let Ok(tokens) = shell_words::split(command) else {
        return None;
    };

    // Cases: "", "ls", "ssh-add-key"
    if tokens.is_empty() || tokens[0] != "ssh" {
        return None;
    }
    Some(tokens)
}

/// Creates an sftp command that copies a given local file into the pwd in the warpified ssh session.
/// `is_dir` (whether the local path is a directory, which adds `-r`) is probed by the caller off
/// the UI thread; this function never touches the filesystem.
pub fn transfer_file_sftp_command(
    local_file_path: String,
    is_dir: bool,
    ssh_host: String,
    ssh_port: Option<String>,
    pwd: Option<String>,
) -> Option<String> {
    // "sftp "
    let mut command = String::from("sftp ");

    // "sftp -P 2222"
    if let Some(port) = ssh_port {
        command += &format!("-P {port} ");
    }

    // "sftp -P 2222 sshuser@127.0.0.1 <<< "put "
    command += &ssh_host;
    command += " <<< \"put ";

    // "sftp -P 2222 sshuser@127.0.0.1 <<< "put -r"
    if is_dir {
        command += "-r "
    }

    // "sftp -P 2222 sshuser@127.0.0.1 <<< "put -r \"path/to/local/file\""
    command += &format!("\\\"{}\\\"", &local_file_path);

    // "sftp -P 2222 sshuser@127.0.0.1 <<< "put -r path/to/local/file pwd/on/remote"
    if let Some(pwd) = pwd {
        command += " ";
        command += &format!("\\\"{}\\\"", &pwd);
    }

    // "sftp -P 2222 sshuser@127.0.0.1 <<< "put -r path/to/local/file pwd/on/remote""
    command += "\"";

    Some(command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quoted_operators_do_not_make_ssh_non_interactive() {
        for cmd in [
            "ssh -o ProxyCommand='sleep 3; false' host",
            "ssh -o 'ProxyCommand=ssh -W %h:%p j | cat' host",
            "ssh -o \"ProxyCommand=a & b\" host",
        ] {
            assert!(parse_interactive_ssh_command(cmd).is_some(), "{cmd}");
        }
        assert!(parse_interactive_ssh_command("ssh -o X='a' host; ls").is_none());
    }

    #[test]
    fn ssh_non_interactive_forms_are_not_interactive() {
        for cmd in [
            "ssh -N host",
            "ssh -f host",
            "ssh -fN host",
            "ssh -Nf host",
            "ssh -vN host",
            "ssh -O check host",
            "ssh -q -O exit host",
            "ssh -Q cipher",
            "ssh -G host",
            "ssh -V",
            "ssh -w any host",
            "ssh -M host",
            "ssh -S /tmp/ctl host",
            "ssh host&",
            "ssh host;",
            "ssh host | cat",
        ] {
            assert!(parse_interactive_ssh_command(cmd).is_none(), "{cmd}");
        }
        for cmd in [
            "ssh host",
            "ssh -v host",
            "ssh -p 22 host",
            "ssh -p22 host",
            "ssh -o Foo=Nf host",
            "ssh -i key_Nf host",
            "ssh -lNick host",
        ] {
            assert!(parse_interactive_ssh_command(cmd).is_some(), "{cmd}");
        }
    }

    #[test]
    fn ssh_gcloud_ssh_parsing() {
        assert!(parse_interactive_ssh_command("gcloud").is_none());
        assert!(parse_interactive_ssh_command("gcloud compute").is_none());
        assert!(parse_interactive_ssh_command("gcloud compute ss").is_none());
        assert!(parse_interactive_ssh_command("gcloud compute ssh").is_none());
        assert!(parse_interactive_ssh_command("command gcloud compute ssh").is_none());

        assert!(
            parse_interactive_ssh_command("command gcloud compute ssh --zone us-west1-a").is_some()
        );
        assert!(parse_interactive_ssh_command("gcloud compute ssh --zone us-west1-a").is_some());
        assert!(
            parse_interactive_ssh_command("gcloud compute ssh --zone us-west1-a my-instance")
                .is_some()
        );
        assert!(
            parse_interactive_ssh_command(
                "gcloud compute ssh --zone us-west1-a my-instance --project my-project"
            )
            .is_some()
        );
    }

    #[test]
    fn ssh_elastic_beanstalk_parsing() {
        assert!(parse_interactive_ssh_command("eb").is_none());
        assert!(parse_interactive_ssh_command("eb ss").is_none());
        assert!(parse_interactive_ssh_command("eb ssh").is_none());
        assert!(parse_interactive_ssh_command("command eb ssh").is_none());

        assert!(parse_interactive_ssh_command("command eb ssh --profile my-profile").is_some());
        assert!(parse_interactive_ssh_command("eb ssh --profile my-profile").is_some());
        assert!(parse_interactive_ssh_command("eb ssh --profile my-profile my-env").is_some());
    }

    #[test]
    fn ssh_digital_ocean_droplet_parsing() {
        assert!(parse_interactive_ssh_command("doctl").is_none());
        assert!(parse_interactive_ssh_command("doctl compute").is_none());
        assert!(parse_interactive_ssh_command("doctl compute ss").is_none());
        assert!(parse_interactive_ssh_command("doctl compute ssh").is_none());
        assert!(parse_interactive_ssh_command("command doctl compute ssh").is_none());

        assert!(parse_interactive_ssh_command("command doctl compute ssh --region nyc1").is_some());
        assert!(parse_interactive_ssh_command("doctl compute ssh --region nyc1").is_some());
        assert!(
            parse_interactive_ssh_command("doctl compute ssh --region nyc1 my-droplet").is_some()
        );
    }

    /// Verifies that commands resulting from shell alias expansion are correctly
    /// detected as interactive SSH commands. When a user types an alias (e.g.
    /// `myssh`), the terminal view expands it to the alias value before passing
    /// it to `parse_interactive_ssh_command`. These tests cover representative
    /// expanded forms.
    #[test]
    fn ssh_alias_expanded_commands() {
        // Simple alias: alias myssh='ssh user@host'
        assert_eq!(
            parse_interactive_ssh_command("ssh user@host").unwrap().host,
            Some("user@host".to_string())
        );

        // Alias with key and user: alias company1='ssh -i /path/to/key user@server'
        assert_eq!(
            parse_interactive_ssh_command("ssh -i /path/to/key user@server")
                .unwrap()
                .host,
            Some("user@server".to_string())
        );

        // Alias with extra args appended by the user: alias myssh='ssh -i key'
        // then the user types `myssh user@host` which expands to `ssh -i key user@host`
        assert_eq!(
            parse_interactive_ssh_command("ssh -i key user@host")
                .unwrap()
                .host,
            Some("user@host".to_string())
        );

        // Alias that isn't SSH should not match
        assert!(parse_interactive_ssh_command("ls -la").is_none());
    }

    #[test]
    fn ssh_interactive_shell_parsing() {
        assert!(parse_interactive_ssh_command("").is_none());
        assert!(parse_interactive_ssh_command("ls").is_none());
        assert!(parse_interactive_ssh_command("ssh-add-key").is_none());

        // Basic interactive command
        assert!(
            parse_interactive_ssh_command("ssh localhost").unwrap().host
                == Some("localhost".to_string())
        );
        assert!(
            parse_interactive_ssh_command("command ssh localhost")
                .unwrap()
                .host
                == Some("localhost".to_string())
        );
        assert!(
            parse_interactive_ssh_command("ssh root@127.14.80.1 -p 2222")
                .unwrap()
                .host
                == Some("root@127.14.80.1".to_string())
        );
        assert!(
            parse_interactive_ssh_command("ssh -4vw root@127.14.80.1 -p 2222")
                .unwrap()
                .host
                == Some("root@127.14.80.1".to_string())
        );

        // Commands with -T or -W, which are non-interactive
        assert!(parse_interactive_ssh_command("ssh -T user@host").is_none());
        assert!(parse_interactive_ssh_command("ssh -v user@host -W localhost:22").is_none());
        assert!(
            parse_interactive_ssh_command("ssh -o IdentityFile=/etc/file -T user@host").is_none()
        );

        // Commands with multiple positional arguments, implying non-interactive
        assert!(parse_interactive_ssh_command("ssh user@host ls").is_none());
        assert!(parse_interactive_ssh_command("ssh user@host echo 'Hello, World!'").is_none());

        // Weird spacing and shell characters shouldn't matter
        assert!(
            parse_interactive_ssh_command("ssh     user@host")
                .unwrap()
                .host
                == Some("user@host".to_string())
        );
        assert!(
            parse_interactive_ssh_command("ssh -4 -- localhost")
                .unwrap()
                .host
                == Some("localhost".to_string())
        );
    }
}
