//! The ordering trace of TODAY's behaviour (design phase 0, risk 1).
//!
//! The legacy buffers push and release raw bytes without ids, so the trace records, per buffer, how
//! many bytes were held and released and which key (`seq=` from `input_order`) caused it. Later
//! phases emit the same hops from the queue, so a recorded old trace can be compared with a new
//! one. Lines go to the `warp::input_order` target (debug), prefixed `input_order`, as
//! `hop=kh_iq_*`. Counts only: typed bytes are never logged (they can be a password).

/// Names of the legacy buffers, as in the design's inventory table.
pub mod legacy {
    pub const PASTE_PTY: &str = "Q4_paste_sequencer";
    pub const PASTE_EDITOR: &str = "Q4b_paste_hold";
    pub const SUBSHELL_BOOTSTRAP: &str = "Q6b_bootstrap_in_flight";
    pub const SUBSHELL_SETUP: &str = "Q6a_setup_held";
    pub const HANDOFF: &str = "Q3_handoff_hold";
    pub const SSH: &str = "Q7_ssh_typeahead";
    pub const AFTER_ENTER: &str = "Q2_after_enter_replay";
}

/// A key's bytes were held by `queue`.
pub fn hold(queue: &'static str, bytes: usize) {
    warpui::input_order::log_key_hop("kh_iq_hold", || format!("queue={queue} bytes={bytes}"));
}

/// `queue` released `chunks` held items (`bytes` in total) towards `dest`, in order.
pub fn release(queue: &'static str, chunks: usize, bytes: usize, dest: &'static str) {
    warpui::input_order::log_key_hop("kh_iq_release", || {
        format!("queue={queue} chunks={chunks} bytes={bytes} dest={dest}")
    });
}

/// User bytes went out to `dest` without being held.
pub fn deliver(bytes: usize, dest: &'static str) {
    warpui::input_order::log_key_hop("kh_iq_deliver", || format!("bytes={bytes} dest={dest}"));
}
