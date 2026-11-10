//! Kill switches of the round 14 queue reconciliation (`docs/design/r14-queueres.md`).
//!
//! Each fix of that round changes what happens to a key on a path that was wrong before; each can
//! be turned off alone, by an environment variable read once per process. Unset (or anything but
//! `0` / `off` / `false` / `no`) means the fix is ON.
//!
//! | variable | off restores |
//! |---|---|
//! | `PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN` | #1900: synced input queued behind an Enter's gate, held for an ssh connect, or re-offered by a follow-up, is an ordinary keyboard entry again (it can take over an agent-run command at delivery) |
//! | `PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES` | #1998: an Enter, pty bytes, a control key, a sequence or an editor key behind a dropped ssh hold is skipped without a log line on the hand-back and local-shell paths |
//! | `PHOSPHOR_YOUNG_HOLD_READER_LOOK` | #2047: a held young Enter is never given to a reader ahead of the line editor; its 10 s net writes it as the shell's line |

use std::sync::OnceLock;

/// #1900.
pub const KEEP_SYNCED_ORIGIN_ENV: &str = "PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN";
/// #1998.
pub const SSH_DROP_ACCOUNTS_ENTRIES_ENV: &str = "PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES";
/// #2047.
pub const YOUNG_HOLD_READER_LOOK_ENV: &str = "PHOSPHOR_YOUNG_HOLD_READER_LOOK";

/// Whether a kill switch with this value leaves its fix on: everything but `0`, `off`, `false`
/// and `no` (any case, surrounding blanks ignored) does, an unset variable included.
pub fn switch_is_on(value: Option<&str>) -> bool {
    !value.is_some_and(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "0" | "off" | "false" | "no"
        )
    })
}

fn read_switch(name: &'static str, cell: &'static OnceLock<bool>) -> bool {
    *cell.get_or_init(|| {
        let on = switch_is_on(std::env::var(name).ok().as_deref());
        if !on {
            log::info!("{name} is off: the round 14 queue fix it guards is disabled");
        }
        on
    })
}

/// #1900: synced input keeps `Origin::Synced` wherever the pane queue holds it.
pub fn keep_synced_origin() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(KEEP_SYNCED_ORIGIN_ENV, &ON)
}

/// #1998: every entry behind a dropped ssh hold is accounted for.
pub fn ssh_drop_accounts_entries() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(SSH_DROP_ACCOUNTS_ENTRIES_ENV, &ON)
}

/// #2047: a held young Enter looks for a reader ahead of the line editor.
pub fn young_hold_reader_look() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(YOUNG_HOLD_READER_LOOK_ENV, &ON)
}

#[cfg(test)]
#[path = "switches_tests.rs"]
mod tests;
