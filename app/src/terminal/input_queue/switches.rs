//! Kill switches of the round 14 queue reconciliation (`docs/design/r14-queueres.md`) and of the
//! owner decisions that followed it in round 17 (`docs/design/r10-r17-queue3.md`).
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
//! | `PHOSPHOR_BARE_ENTER_HOLDS_LINES` | #1999: lines typed behind a bare deferred Enter held for an unready shell are written to the pty at once, and the bare Enter later |
//! | `PHOSPHOR_KEEP_DEFERRED_ENTER_KEYS_WITH_LINE` | #2000: a deferred Enter kept unrun on an event path releases the keys typed behind it (complete lines go to the pty while its own line stays in the input) |
//! | `PHOSPHOR_SUBSHELL_LINE_EDITOR_PROOF` | #1991: a subshell setup line written to a line editor is judged by the echo filter again (the R9 flush on "nothing echoed", R7 blind, the filter armed) |
//! | `PHOSPHOR_HOLD_KEYS_BEHIND_RECALL` | #2107: no `HistoryRecall` gate opens when the Up arrow opens the inline history menu on a long history; keys typed before the list lands are held only by the menu's own selection-key gate (Up, Down, Accept) and edit the empty buffer ahead of the recalled line, as before |
//! | `PHOSPHOR_REMOTE_TAB_ENTER_WAIT` | #2188: no `RemoteTabCompletion` gate opens when a Tab in a remote (managed ssh) session starts a completion the remote side answers; an Enter typed before that answer runs the line as typed (uncompleted) and cancels the completion, and text typed before it drops the answer, as before |
//! | `PHOSPHOR_HOLD_EDIT_KEYS_BEHIND_REPORT` | #2115: no `ReportEdit` gate opens when a cursor or deletion key is typed in the editor while the shell's report of the line is still on its way; the key acts at once on the partial buffer (a move past its start is clamped and lost) |
//! | `PHOSPHOR_SSH_UNBOOTSTRAPPED_PLAIN` | #2120 (`docs/design/r19-shssh.md`): the `SSH` hook of a remote login shell the remote command does not bootstrap (sh, dash, ash, ksh, ...) announces a managed session again: the keys typed in that session are held for a first prompt that never comes |

use std::sync::OnceLock;

/// #1900.
pub const KEEP_SYNCED_ORIGIN_ENV: &str = "PHOSPHOR_QUEUE_KEEP_SYNCED_ORIGIN";
/// #1998.
pub const SSH_DROP_ACCOUNTS_ENTRIES_ENV: &str = "PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES";
/// #2047.
pub const YOUNG_HOLD_READER_LOOK_ENV: &str = "PHOSPHOR_YOUNG_HOLD_READER_LOOK";

/// #1999.
pub const BARE_ENTER_HOLDS_LINES_ENV: &str = "PHOSPHOR_BARE_ENTER_HOLDS_LINES";
/// #2000.
pub const KEEP_DEFERRED_ENTER_KEYS_WITH_LINE_ENV: &str =
    "PHOSPHOR_KEEP_DEFERRED_ENTER_KEYS_WITH_LINE";
/// #1991.
pub const SUBSHELL_LINE_EDITOR_PROOF_ENV: &str = "PHOSPHOR_SUBSHELL_LINE_EDITOR_PROOF";
/// #2120.
pub const SSH_UNBOOTSTRAPPED_PLAIN_ENV: &str = "PHOSPHOR_SSH_UNBOOTSTRAPPED_PLAIN";

/// #2107.
pub const HOLD_KEYS_BEHIND_RECALL_ENV: &str = "PHOSPHOR_HOLD_KEYS_BEHIND_RECALL";

/// #2188.
pub const REMOTE_TAB_ENTER_WAIT_ENV: &str = "PHOSPHOR_REMOTE_TAB_ENTER_WAIT";

/// #2115.
pub const HOLD_EDIT_KEYS_BEHIND_REPORT_ENV: &str = "PHOSPHOR_HOLD_EDIT_KEYS_BEHIND_REPORT";

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
            log::info!("{name} is off: the input queue fix it guards is disabled");
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

/// #1999: an Enter held for an unready shell holds the lines typed behind it whatever its own
/// line is, a bare Enter included.
pub fn bare_enter_holds_lines() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(BARE_ENTER_HOLDS_LINES_ENV, &ON)
}

/// #2000: a deferred Enter kept unrun on an event path keeps the keys typed behind it with its
/// line.
pub fn keep_deferred_enter_keys_with_line() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(KEEP_DEFERRED_ENTER_KEYS_WITH_LINE_ENV, &ON)
}

/// #1991: a subshell setup line written to a line editor is judged without the echo filter.
pub fn subshell_line_editor_proof() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(SUBSHELL_LINE_EDITOR_PROOF_ENV, &ON)
}

/// #2120: the `SSH` hook of a login shell the remote command does not bootstrap is the plain
/// verdict of the connect window, and announces no session.
pub fn ssh_unbootstrapped_plain() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(SSH_UNBOOTSTRAPPED_PLAIN_ENV, &ON)
}

/// #2107: the keys typed while the inline history menu's list is being built wait behind the
/// `HistoryRecall` gate and are applied to the recalled line.
pub fn hold_keys_behind_recall() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(HOLD_KEYS_BEHIND_RECALL_ENV, &ON)
}

/// #2188: the keys typed while a remote Tab completion is being answered wait behind the
/// `RemoteTabCompletion` gate and are applied to the completed line.
pub fn remote_tab_enter_wait() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(REMOTE_TAB_ENTER_WAIT_ENV, &ON)
}

/// #2115: a cursor or deletion key typed in the editor while the shell's report of the line is
/// still on its way waits behind the `ReportEdit` gate, with every key after it, and is applied
/// to the joined line. `0`: such keys act at once on the partial buffer, as before.
pub fn hold_edit_keys_behind_report() -> bool {
    static ON: OnceLock<bool> = OnceLock::new();
    read_switch(HOLD_EDIT_KEYS_BEHIND_REPORT_ENV, &ON)
}

#[cfg(test)]
#[path = "switches_tests.rs"]
mod tests;
