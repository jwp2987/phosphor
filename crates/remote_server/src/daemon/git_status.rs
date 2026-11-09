//! Git status subscription (UpdateGitStatus) (Wave H parity, family package P6).
//! Reserved by P0: the request is a notification, so
//! until the family lands it is dropped with a debug log (the client does not wait on a reply;
//! the capability list tells it to use its shell backend). The implementing package replaces
//! [`ignore`] and adds `crate::capability::GIT_STATUS` to `HELPER_CAPABILITIES`.

/// The capability this module will advertise once implemented.
#[allow(dead_code)]
pub(super) const CAPABILITY: &str = crate::capability::GIT_STATUS;

/// Drops the notification `name`.
pub(super) fn ignore(name: &'static str) {
    log::debug!("Daemon: ignoring unsupported notification {name}");
}
