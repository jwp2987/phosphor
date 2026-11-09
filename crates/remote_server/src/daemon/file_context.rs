//! Batch ReadFileContext (Wave H parity, family package P1).
//! Reserved by P0: until the family lands every request
//! for it is answered with `ErrorCode::UNSUPPORTED` and the client uses its shell backend. The
//! package that implements it replaces [`handle`] and adds `crate::capability::FILE_CONTEXT` to
//! `HELPER_CAPABILITIES` (plus the pin in `server_tests.rs`).

use super::jobs::Reply;

/// The capability this module will advertise once implemented.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::FILE_CONTEXT;

/// Answers `request` (a proto variant name) with UNSUPPORTED.
pub(super) fn handle(reply: &Reply, request: &'static str) {
    super::server::unsupported(reply, request);
}
