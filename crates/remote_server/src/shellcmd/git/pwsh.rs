//! PowerShell source that runs one native command (`git`, `gh`) the way [`GitRunner`] needs it:
//! raw stdout and stderr bytes, the real exit code, stdin fed byte for byte, a fixed environment.
//! For Windows hosts reached over ssh with no helper (#1874).
//!
//! PowerShell's own pipeline re-encodes text and adds line ends, which would corrupt a patch or a
//! diff, so the command runs through `System.Diagnostics.Process` with every stream a raw byte
//! stream. The script's own stdin (what the ssh channel carries) is copied to the child's stdin,
//! so a patch never has to fit the command line (Windows allows 32 767 characters).
//!
//! [`GitRunner`]: super::GitRunner

/// What the script sets up around the child.
#[derive(Clone, Debug, Default)]
pub struct PwshCommand<'a> {
    /// `git` or `gh`.
    pub program: &'a str,
    pub args: &'a [&'a str],
    pub cwd: &'a str,
    /// The script's stdin is the child's stdin (otherwise the child gets an empty one).
    pub forward_stdin: bool,
    /// Variables removed from the child's environment.
    pub unset: &'a [&'a str],
    /// Variables set in the child's environment.
    pub set: &'a [(&'a str, &'a str)],
    /// Set `GIT_SSH_COMMAND` to the host's own plus `-o BatchMode=yes` (see
    /// [`super::git_ssh_command`]).
    pub ssh_batch: bool,
    /// Directory list to use as the child's `PATH`.
    pub path: Option<&'a str>,
}

fn q(s: &str) -> String {
    format!("'{}'", s.replace('\'', "''"))
}

/// The PowerShell source for `command`.
pub fn run_script(command: &PwshCommand<'_>) -> String {
    let args: Vec<String> = command.args.iter().map(|a| q(a)).collect();
    let mut env = String::new();
    for name in command.unset {
        env.push_str(&format!("[void]$psi.Environment.Remove({})\n", q(name)));
    }
    for (name, value) in command.set {
        env.push_str(&format!("$psi.Environment[{}] = {}\n", q(name), q(value)));
    }
    if command.ssh_batch {
        env.push_str(
            "$ssh = $env:GIT_SSH_COMMAND\n\
             if (-not $ssh) { $ssh = 'ssh' }\n\
             $psi.Environment['GIT_SSH_COMMAND'] = $ssh + ' -o BatchMode=yes'\n",
        );
    }
    if let Some(path) = command.path {
        env.push_str(&format!("$psi.Environment['PATH'] = {}\n", q(path)));
    }
    let feed = if command.forward_stdin {
        "[Console]::OpenStandardInput().CopyTo($p.StandardInput.BaseStream)\n"
    } else {
        ""
    };
    format!(
        "$ErrorActionPreference = 'Stop'\n\
         try {{ Set-Location -LiteralPath {cwd} }} catch {{ [Console]::Error.WriteLine('cd: ' + {cwd} + ': No such file or directory'); exit 1 }}\n\
         $psi = New-Object System.Diagnostics.ProcessStartInfo\n\
         $psi.FileName = {program}\n\
         foreach ($a in @({args})) {{ [void]$psi.ArgumentList.Add($a) }}\n\
         $psi.WorkingDirectory = (Get-Location).ProviderPath\n\
         $psi.UseShellExecute = $false\n\
         $psi.RedirectStandardInput = $true\n\
         $psi.RedirectStandardOutput = $true\n\
         $psi.RedirectStandardError = $true\n\
         {env}\
         try {{ $p = [System.Diagnostics.Process]::Start($psi) }} catch {{ [Console]::Error.WriteLine({program} + ': command not found'); exit 127 }}\n\
         $out = New-Object System.IO.MemoryStream\n\
         $err = New-Object System.IO.MemoryStream\n\
         $outTask = $p.StandardOutput.BaseStream.CopyToAsync($out)\n\
         $errTask = $p.StandardError.BaseStream.CopyToAsync($err)\n\
         try {{ {feed}}} catch {{ }}\n\
         try {{ $p.StandardInput.Close() }} catch {{ }}\n\
         $p.WaitForExit()\n\
         [void]$outTask.Wait()\n\
         [void]$errTask.Wait()\n\
         $so = [Console]::OpenStandardOutput(); $b = $out.ToArray(); $so.Write($b, 0, $b.Length); $so.Flush()\n\
         $se = [Console]::OpenStandardError(); $b = $err.ToArray(); $se.Write($b, 0, $b.Length); $se.Flush()\n\
         exit $p.ExitCode",
        cwd = q(command.cwd),
        program = q(command.program),
        args = args.join(", "),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_script_quotes_every_piece_and_forwards_stdin_only_when_asked() {
        let args = ["log", "--format=it's"];
        let script = run_script(&PwshCommand {
            program: "git",
            args: &args,
            cwd: "C:\\my repo",
            forward_stdin: true,
            unset: &["GIT_ASKPASS"],
            set: &[("GIT_TERMINAL_PROMPT", "0")],
            ssh_batch: true,
            path: None,
        });
        assert!(
            script.contains("Set-Location -LiteralPath 'C:\\my repo'"),
            "{script}"
        );
        assert!(script.contains("@('log', '--format=it''s')"), "{script}");
        // The child's own working directory is set (Set-Location does not move the process).
        assert!(
            script.contains("$psi.WorkingDirectory = (Get-Location).ProviderPath"),
            "{script}"
        );
        assert!(
            script.contains("Environment.Remove('GIT_ASKPASS')"),
            "{script}"
        );
        assert!(
            script.contains("Environment['GIT_TERMINAL_PROMPT'] = '0'"),
            "{script}"
        );
        assert!(script.contains("-o BatchMode=yes"), "{script}");
        assert!(script.contains("OpenStandardInput().CopyTo"), "{script}");
        let quiet = run_script(&PwshCommand {
            program: "gh",
            args: &["pr", "view"],
            cwd: "/r",
            ..Default::default()
        });
        assert!(!quiet.contains("OpenStandardInput"), "{quiet}");
        assert!(quiet.contains("exit $p.ExitCode"), "{quiet}");
    }
}
