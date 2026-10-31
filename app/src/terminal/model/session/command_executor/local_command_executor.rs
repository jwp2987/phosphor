use super::{CommandExecutor, CommandOutput, ExecuteCommandOptions};
use crate::safe_warn;
use crate::terminal::shell::{Shell, ShellType};
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use command::r#async::Command;
use parking_lot::Mutex;
use std::any::Any;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

#[cfg(unix)]
fn kill_all_processes_in_process_group(pid: u32) -> Result<(), nix::Error> {
    use nix::sys::signal::{kill, Signal};
    use nix::unistd::Pid;
    // Killing a negative PID kills all processes in this process group
    kill(Pid::from_raw(-(pid as i32)), Signal::SIGKILL)
}
#[cfg(unix)]
fn terminate_process_group(process_group: &ActiveProcessGroup) {
    let process_group_id = process_group.id;
    // A pgid of 0 targets the caller's own process group, and 1 negates to
    // -1, which SIGKILLs every process this user is allowed to signal.
    // Neither is ever a legitimate target, so refuse them rather than let a
    // bad pgid reach `kill`.
    if process_group_id < 2 {
        log::warn!("Refusing to signal process group {process_group_id}: pid is below 2");
        return;
    }

    match kill_all_processes_in_process_group(process_group_id) {
        Ok(()) => log::info!("Sent SIGKILL to process group {process_group_id}"),
        Err(error @ nix::errno::Errno::ESRCH) => {
            log::info!("Process group {process_group_id} had already exited: {error}");
        }
        Err(error @ nix::errno::Errno::EPERM) => {
            log::warn!("Not permitted to kill process group {process_group_id}: {error}");
        }
        Err(error) => {
            log::warn!("Failed to kill process group {process_group_id}: {error}");
        }
    }
}

/// Windows has no process groups, so the spawned child is placed in a kill-on-close Job
/// Object instead; closing the job terminates the child and everything it spawned.
#[cfg(windows)]
fn terminate_process_group(process_group: &ActiveProcessGroup) {
    match process_group.job.lock().take() {
        Some(job) => {
            drop(job);
            log::info!("Closed job object for process {}", process_group.id);
        }
        None => log::info!(
            "No job object for process {}; relying on kill_on_drop",
            process_group.id
        ),
    }
}

/// The process groups this executor has spawned and not yet accounted for.
///
/// Registration is keyed by pgid, but each entry holds an `Arc` identity as well,
/// so a stale handle whose pgid has since been reused by the OS can never cancel
/// the new occupant: `remove` only matches on `Arc::ptr_eq`.
#[derive(Debug, Default)]
struct ActiveProcessGroups {
    process_groups: Mutex<HashMap<u32, Arc<ActiveProcessGroup>>>,
}

#[derive(Debug)]
struct ActiveProcessGroup {
    id: u32,
    /// Windows only: the job holding the child, closed to kill it and its descendants.
    #[cfg(windows)]
    job: Mutex<Option<command::windows::KillOnCloseJob>>,
}

impl ActiveProcessGroups {
    fn register(&self, process_group_id: u32) -> Arc<ActiveProcessGroup> {
        let process_group = Arc::new(ActiveProcessGroup {
            id: process_group_id,
            #[cfg(windows)]
            job: Mutex::new(None),
        });
        self.process_groups
            .lock()
            .insert(process_group_id, process_group.clone());
        process_group
    }

    fn remove(&self, process_group: &Arc<ActiveProcessGroup>) -> bool {
        let mut process_groups = self.process_groups.lock();
        if !process_groups
            .get(&process_group.id)
            .is_some_and(|active| Arc::ptr_eq(active, process_group))
        {
            return false;
        }
        process_groups.remove(&process_group.id);
        true
    }

    fn complete(&self, process_group: &Arc<ActiveProcessGroup>) {
        self.remove(process_group);
        // The command finished on its own: close the job without killing deliberately
        // detached descendants, matching the unix behaviour of leaving the group alone.
        #[cfg(windows)]
        if let Some(job) = process_group.job.lock().take() {
            job.disarm();
        }
    }

    fn cancel(&self, process_group: &Arc<ActiveProcessGroup>) {
        if self.remove(process_group) {
            terminate_process_group(process_group);
        }
    }

    fn cancel_all(&self) {
        let process_groups = self
            .process_groups
            .lock()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for process_group in process_groups {
            self.cancel(&process_group);
        }
    }
}

/// Ties a spawned child's process group to the lifetime of the command future.
/// Dropping it without calling [`SpawnedChildCleanup::complete`] — which is what
/// happens when the future is cancelled — SIGKILLs the whole group, so a
/// generator's descendants cannot outlive the command that spawned them.
struct SpawnedChildCleanup {
    process_group: Option<Arc<ActiveProcessGroup>>,
    active_process_groups: Arc<ActiveProcessGroups>,
}

impl SpawnedChildCleanup {
    fn new(process_group_id: u32, active_process_groups: Arc<ActiveProcessGroups>) -> Self {
        let process_group = active_process_groups.register(process_group_id);
        Self {
            process_group: Some(process_group),
            active_process_groups,
        }
    }

    /// Windows only: puts the freshly spawned child in a kill-on-close job so that
    /// cancelling the command can terminate it and its descendants. Best effort.
    #[cfg(windows)]
    fn attach_job(&self, process_handle: isize) {
        let Some(process_group) = &self.process_group else {
            return;
        };
        match command::windows::KillOnCloseJob::with_process(process_handle) {
            Ok(job) => *process_group.job.lock() = Some(job),
            Err(e) => log::warn!(
                "Could not place process {} in a job object: {e:#}",
                process_group.id
            ),
        }
    }

    fn complete(mut self) {
        if let Some(process_group) = self.process_group.take() {
            self.active_process_groups.complete(&process_group);
        }
    }
}

impl Drop for SpawnedChildCleanup {
    fn drop(&mut self) {
        if let Some(process_group) = self.process_group.take() {
            self.active_process_groups.cancel(&process_group);
        }
    }
}

/// The `shell_config_flag` that selects "no user configuration" for nushell. For nushell it is
/// also the marker that the command is POSIX syntax meant for `bash -c`, see
/// [`CommandBuilder::build`].
const NU_NO_CONFIG_FLAG: &str = "--no-config-file";

enum CommandBuilder<'a> {
    #[cfg(windows)]
    CmdExe,
    ShellType {
        shell_type: ShellType,
        local_shell_path: Option<&'a Path>,
    },
}

impl CommandBuilder<'_> {
    fn build(self, command_string: &str, shell_config_flag: &str) -> Command {
        match self {
            #[cfg(windows)]
            CommandBuilder::CmdExe => {
                use command::windows::CommandExt as _;
                let mut command = Command::new_with_process_group("cmd.exe");
                command.args(["/Q", "/C"]);
                command.raw_arg(command_string);
                command
            }
            // Commands Phosphor runs for itself are POSIX syntax, which nushell cannot parse, so
            // they go through `bash` (or `sh`). A login-shell command (`-l`) is the shell's own
            // syntax and is run by nushell itself, below.
            CommandBuilder::ShellType {
                shell_type: ShellType::Nu,
                ..
            } if shell_config_flag == NU_NO_CONFIG_FLAG => {
                let bash = warp_util::path::resolve_executable("bash");
                let program = bash
                    .as_deref()
                    .and_then(|path| path.to_str())
                    .unwrap_or("sh");
                let mut command = Command::new_with_process_group(program);
                if bash.is_some() {
                    command.arg("--norc");
                }
                command.arg("-c");
                command.arg(command_string);
                command
            }
            CommandBuilder::ShellType {
                local_shell_path,
                shell_type,
            } => {
                let program_to_execute = local_shell_path
                    .as_ref()
                    .and_then(|p| p.to_str())
                    .unwrap_or_else(|| {
                        log::warn!("local_shell_path was None for a local session");
                        shell_type.name()
                    });
                let mut command = Command::new_with_process_group(program_to_execute);
                command.arg(shell_config_flag);
                if shell_type == ShellType::PowerShell {
                    // -Command/-c re-parses the joined Windows command-line string with
                    // PowerShell's own tokenizer, which chokes on the backslash-escaped
                    // quotes that Windows argv quoting produces for any command_string
                    // containing a `"` (e.g. a quoted path) on PS 7.6. -EncodedCommand
                    // sidesteps both re-parsing layers; see
                    // `util::windows::encode_pwsh_command`.
                    command.arg("-EncodedCommand");
                    command.arg(crate::util::encode_pwsh_command(command_string));
                } else {
                    command.arg("-c");
                    command.arg(command_string);
                }
                command
            }
        }
    }
}

#[cfg(test)]
#[path = "local_command_executor_tests.rs"]
mod tests;

/// `CommandExecutor` implementation that executes the given `command` in a forked subshell process
/// where the current working directory is set to `current_dir_path` and $PATH is set
/// according to environment_variables. This is typically used to run generator commands for local sessions.
#[derive(Debug)]
pub struct LocalCommandExecutor {
    local_shell_path: Option<PathBuf>,
    shell_type: ShellType,

    active_process_groups: Arc<ActiveProcessGroups>,
}

impl LocalCommandExecutor {
    pub fn new(local_shell_path: Option<PathBuf>, shell_type: ShellType) -> Self {
        Self {
            local_shell_path,
            shell_type,
            active_process_groups: Arc::default(),
        }
    }

    pub async fn execute_local_command(
        &self,
        command: &str,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        let shell_config_flag = match self.shell_type {
            ShellType::Zsh => "-f",
            ShellType::Bash => "--norc",
            ShellType::Fish => "--no-config",
            ShellType::PowerShell => "-NoProfile",
            ShellType::Nu => NU_NO_CONFIG_FLAG,
        };

        self.execute_local_command_internal(
            command,
            current_directory_path,
            environment_variables,
            shell_config_flag,
            execute_command_options,
        )
        .await
    }

    pub async fn execute_local_command_in_login_shell(
        &self,
        command: &str,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
    ) -> Result<CommandOutput> {
        let shell_config_flag = match self.shell_type {
            ShellType::Bash | ShellType::Zsh | ShellType::Fish | ShellType::Nu => "-l",
            ShellType::PowerShell => "-Login",
        };

        self.execute_local_command_internal(
            command,
            current_directory_path,
            environment_variables,
            shell_config_flag,
            ExecuteCommandOptions {
                // We have to run the command in the same shell as the session
                // because we want to run it in a login shell.
                run_command_in_same_shell_as_session: true,
            },
        )
        .await
    }

    #[cfg(unix)]
    fn command_builder(
        &self,
        _execute_command_options: ExecuteCommandOptions,
    ) -> CommandBuilder<'_> {
        CommandBuilder::ShellType {
            shell_type: self.shell_type,
            local_shell_path: self.local_shell_path.as_deref(),
        }
    }

    #[cfg(windows)]
    fn command_builder(
        &self,
        execute_command_options: ExecuteCommandOptions,
    ) -> CommandBuilder<'_> {
        let use_cmd_exe = !execute_command_options.run_command_in_same_shell_as_session
            && self.shell_type == ShellType::PowerShell;
        if use_cmd_exe {
            CommandBuilder::CmdExe
        } else {
            CommandBuilder::ShellType {
                shell_type: self.shell_type,
                local_shell_path: self.local_shell_path.as_deref(),
            }
        }
    }

    async fn execute_local_command_internal(
        &self,
        command: &str,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        // The value of shell_config_flag is appended as an argument
        // indicating the supplied command should be run under some configuration,
        // i.e. in a login shell or without sourcing .rc files
        shell_config_flag: &str,
        execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        let command_builder = self.command_builder(execute_command_options);

        let mut command_process = command_builder.build(command, shell_config_flag);

        // This sets then environment variables, including the PATH var.
        // We need to run the command with the PATH var set because if the
        // user opened Zap through a parent process that didn't have the PATH var set
        // (i.e. outside of a shell, for example opening the app via Finder),
        // the subshell won't inherit the PATH var, but we need the PATH var
        // to reference executables we might run as part of generators.
        // Note: we don't need to quote/escape the PATH and pwd because
        // they're treated as single words.
        if let Some(environment_variables) = environment_variables {
            command_process.envs(&environment_variables);
        }

        // Set the current dir, if any.
        if let Some(current_directory_path) = current_directory_path {
            command_process.current_dir(current_directory_path);
        }

        // The purpose of the executor is to produce output. If the child
        // has been dropped, there's no way to get the output anymore,
        // so there's no need for the process itself to stick around.
        let child = command_process
            .kill_on_drop(true)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let child_cleanup =
            SpawnedChildCleanup::new(child.id(), self.active_process_groups.clone());
        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle as _;
            child_cleanup.attach_job(child.as_raw_handle() as isize);
        }

        let output = child
            .output()
            .await
            .map(|output| output.into())
            .map_err(|e| {
                safe_warn!(
                    safe: ("error executing local command"),
                    full: ("error executing command {:?} with error {:?}", command, e)
                );
                anyhow!(e)
            });

        if output.is_ok() {
            child_cleanup.complete();
        }
        output
    }
}

#[async_trait]
impl CommandExecutor for LocalCommandExecutor {
    async fn execute_command(
        &self,
        command: &str,
        _shell: &Shell,
        current_directory_path: Option<&str>,
        environment_variables: Option<HashMap<String, String>>,
        execute_command_options: ExecuteCommandOptions,
    ) -> Result<CommandOutput> {
        self.execute_local_command(
            command,
            current_directory_path,
            environment_variables,
            execute_command_options,
        )
        .await
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn supports_parallel_command_execution(&self) -> bool {
        true
    }

    fn cancel_active_commands(&self) {
        self.active_process_groups.cancel_all();
    }
}
