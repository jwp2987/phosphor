//! Per-host consent for installing the `phosphor-remote` helper (Wave H, #1804).
//!
//! The helper is optional. The no-helper path is the primary design and is always available, so
//! declining, or a host that cannot take the helper, loses nothing. A host's policy is one of
//! `ask` (the default), `allow` or `never`; the app stores the allowed and the denied hosts as
//! two lists and this module is the pure logic over them. The app-wide install mode setting
//! (`always ask` / `always install` / `never install`) stays as the default for a host whose
//! policy is `ask`.

/// What the user decided for one host.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HelperPolicy {
    /// Not decided: follow the app-wide mode (which asks by default).
    #[default]
    Ask,
    /// Install without asking.
    Allow,
    /// Never install, never update, never upload.
    Never,
}

/// The app-wide install mode, mirrored here so the decision is testable without the app.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GlobalInstallMode {
    AlwaysAsk,
    AlwaysInstall,
    NeverInstall,
}

/// What to do when the helper is not installed on a host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperDecision {
    /// Show the install/continue choice.
    Ask,
    /// Upload the helper.
    Install,
    /// Stay on the no-helper path.
    Skip,
}

/// A host\'s own decision wins over the app-wide mode; `Ask` defers to it.
pub fn decide(host: HelperPolicy, global: GlobalInstallMode) -> HelperDecision {
    match host {
        HelperPolicy::Allow => HelperDecision::Install,
        HelperPolicy::Never => HelperDecision::Skip,
        HelperPolicy::Ask => match global {
            GlobalInstallMode::AlwaysAsk => HelperDecision::Ask,
            GlobalInstallMode::AlwaysInstall => HelperDecision::Install,
            GlobalInstallMode::NeverInstall => HelperDecision::Skip,
        },
    }
}

/// Whether an already-installed (older) helper may be replaced by this client\'s version without
/// asking: yes unless the user said `never` for the host.
pub fn may_update_existing(host: HelperPolicy) -> bool {
    host != HelperPolicy::Never
}

/// The host\'s policy from the two stored lists. `never` beats `allow` if a host is in both
/// (hand-edited settings): the safer reading wins.
pub fn policy_from_lists(host: &str, allowed: &[String], denied: &[String]) -> HelperPolicy {
    if denied.iter().any(|h| h == host) {
        HelperPolicy::Never
    } else if allowed.iter().any(|h| h == host) {
        HelperPolicy::Allow
    } else {
        HelperPolicy::Ask
    }
}

/// Records `policy` for `host` in the two lists: removes it from both, then adds it to the list
/// that matches. `Ask` leaves it in neither.
pub fn set_policy(
    host: &str,
    policy: HelperPolicy,
    allowed: &mut Vec<String>,
    denied: &mut Vec<String>,
) {
    allowed.retain(|h| h != host);
    denied.retain(|h| h != host);
    match policy {
        HelperPolicy::Ask => {}
        HelperPolicy::Allow => allowed.push(host.to_string()),
        HelperPolicy::Never => denied.push(host.to_string()),
    }
}

#[cfg(test)]
#[path = "policy_tests.rs"]
mod tests;
