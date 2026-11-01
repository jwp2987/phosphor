use anyhow::Result;
use diesel::{sqlite::SqliteConnection, ExpressionMethods, QueryDsl, RunQueryDsl};
use warp_core::command::ExitCode;

use crate::terminal::ShellHost;

/// Bumped by the writer thread each time it has committed a change to the `commands` table
/// (a started or finished command). Readers that cache a query over that table record it before
/// querying and treat the result as stale once it moves: the write event itself, not a time
/// window, says when a cached answer may be missing a row.
static COMMANDS_WRITE_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn commands_write_epoch() -> u64 {
    COMMANDS_WRITE_EPOCH.load(std::sync::atomic::Ordering::SeqCst)
}

pub fn note_commands_written() {
    COMMANDS_WRITE_EPOCH.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
}

/// Returns the command that was run right after `command`
/// in the same session, if any.
pub fn get_next_command(
    conn: &mut SqliteConnection,
    command: &super::model::Command,
) -> Result<super::model::Command> {
    let next_command = super::schema::commands::dsl::commands
        .filter(super::schema::commands::columns::id.gt(command.id))
        .filter(super::schema::commands::columns::session_id.eq(&command.session_id))
        // Skip any empty blocks
        .filter(super::schema::commands::columns::command.ne(""))
        .order(super::schema::commands::columns::id.asc())
        .limit(1)
        .first::<super::model::Command>(conn)?;
    Ok(next_command)
}

/// Returns the commands that were run right before `command`
/// in the same session, if any. They are ordered from oldest to newest.
pub fn get_previous_commands(
    conn: &mut SqliteConnection,
    command: &super::model::Command,
    num_commands: usize,
) -> Result<Vec<super::model::Command>> {
    let previous_commands = super::schema::commands::dsl::commands
        .filter(super::schema::commands::columns::id.lt(command.id))
        .filter(super::schema::commands::columns::session_id.eq(&command.session_id))
        // Skip any empty blocks
        .filter(super::schema::commands::columns::command.ne(""))
        .order(super::schema::commands::columns::id.desc())
        .limit(num_commands as i64)
        .load::<super::model::Command>(conn)?;
    Ok(previous_commands.into_iter().rev().collect())
}

/// Gets the last num_commands times the same command was run in a similar context
/// (same pwd, exit code, shell, hostname), from newest to oldest.
pub fn get_same_commands_from_history(
    conn: &mut SqliteConnection,
    command: &str,
    pwd: &Option<String>,
    exit_code: ExitCode,
    shell_host: Option<&ShellHost>,
    num_commands: usize,
) -> Result<Vec<super::model::Command>> {
    let commands = super::schema::commands::dsl::commands
        .filter(super::schema::commands::columns::command.eq(command))
        .filter(super::schema::commands::columns::pwd.eq(pwd))
        .filter(super::schema::commands::columns::exit_code.eq(exit_code.value()))
        .filter(
            super::schema::commands::columns::shell
                .eq(shell_host.map(|host| host.shell_type.name())),
        )
        .filter(
            super::schema::commands::columns::hostname.eq(shell_host.map(|host| &host.hostname)),
        )
        // Get newest to oldest commands.
        .order(super::schema::commands::columns::id.desc())
        .limit(num_commands as i64)
        .load::<super::model::Command>(conn)?;

    Ok(commands)
}
