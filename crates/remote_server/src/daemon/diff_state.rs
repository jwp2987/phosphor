//! Diff-state subscriptions (Wave H parity, family package P6).
//! Reserved by P0: until the family lands every request
//! for it is answered with `ErrorCode::UNSUPPORTED` and the client uses its shell backend. The
//! package that implements it replaces [`handle`] and adds `crate::capability::DIFF_STATE` to
//! `HELPER_CAPABILITIES` (plus the pin in `server_tests.rs`).

use super::jobs::Reply;

/// The capability this module will advertise once implemented.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::DIFF_STATE;

/// Answers `request` (a proto variant name) with UNSUPPORTED.
pub(super) fn handle(reply: &Reply, request: &'static str) {
    super::server::unsupported(reply, request);
}
