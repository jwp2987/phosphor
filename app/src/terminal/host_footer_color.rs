//! Resolves which host a terminal session is "on", to color the window footer bar
//! (`view::window_footer_bar`) so a session on a host matching a user rule -- a
//! production box, say -- is visually unmistakable.
//!
//! # Two host sources, one precedence
//!
//! [`resolve_host`] is the single function that combines both sources Phosphor can
//! learn a session's remote host from, so rule matching has exactly one input to
//! test rather than two code paths that could disagree:
//!
//! 1. **Shell-integration hostname** (`Session::hostname()`) -- authoritative when
//!    present. `SessionInfo::determine_session_type` (`terminal::model::session`)
//!    already compares this against the local machine's hostname at `InitShell` time
//!    to classify the session `Local` vs `WarpifiedRemote`; a `WarpifiedRemote`
//!    session's hostname is therefore the *remote* host, reported by shell
//!    integration running on that machine.
//! 2. **A typed `ssh`-shaped command's parsed target**
//!    (`warpify::trigger_state::WarpifyState::pending_ssh_target`, backed by
//!    `ssh::util::parse_interactive_ssh_command`) -- works without warpification,
//!    e.g. while the SSH session is still logging in, or the remote shell never
//!    gets shell integration installed.
//!
//! Source 1 wins whenever the session is genuinely remote; source 2 is the fallback
//! for a `Local` session (nothing has been warpified, so `hostname()` is just the
//! local machine) that is currently running an ssh-shaped command.
//!
//! # The unknown case
//!
//! `gcloud`/Elastic-Beanstalk/DigitalOcean-style commands, an `ssh` invocation via an
//! unresolvable alias, a second hop from inside a remote shell, and `tmux attach` can
//! all leave source 2 unable to name a host even though a remote session is plausibly
//! in progress. [`ResolvedHost::Unknown`] exists so that case is never silently
//! folded into [`ResolvedHost::Local`].
//!
//! A rule can never match `Unknown` -- there is no string to match against, and
//! [`matching_color`] correctly returns `None` for it, same as it would for `Local`.
//! That is *not* the end of the story, though: [`resolve_footer_bar_color`] layers a
//! second decision on top of `matching_color`'s result, specifically for `Unknown`,
//! so that "we don't know what host this is" renders as a distinct, configurable
//! caution color (`TabSettings::unknown_host_color`, default
//! [`AnsiColorIdentifier::Yellow`]) instead of silently falling back to the same
//! default a genuinely `Local` session gets. Do not "fix" `matching_color` to close
//! this gap by matching `Unknown` against a placeholder string (such as `""`) --
//! that would make the unknown-host color depend on incidental rule patterns (e.g. a
//! rule matching `^$`), instead of being the single, always-applicable setting it is
//! today.
//!
//! # Performance
//!
//! Every function here is pure and cheap except the regex matching in
//! [`matching_color`], which is why callers must never call it during render or
//! layout: resolve it once when the session's host or the rule list changes, cache
//! the resulting color, and have rendering read the cached value. See
//! `view::window_footer_bar` and `TerminalView`'s `window_footer_bar_color` field.

use warp_core::ui::theme::AnsiColorIdentifier;

use crate::terminal::model::session::SessionType;
use crate::workspace::tab_settings::HostFooterColorRule;

/// The host string to match window-footer-bar color rules against, resolved with
/// [`resolve_host`]'s precedence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedHost {
    /// The session is not connected to any host other than the local machine.
    Local,
    /// A host string to match rules against: a remote hostname reported by shell
    /// integration, or a typed `ssh` command's parsed target.
    Named(String),
    /// A remote connection is plausibly in progress, but which host it is could not
    /// be determined (see the module doc's "unknown case").
    ///
    /// Deliberately distinct from [`Self::Local`]: collapsing this into `Local` (or
    /// into a bare `Option<String>`'s `None`) would make "we don't know what host
    /// this is" indistinguishable from "this is definitely the local machine" --
    /// [`resolve_footer_bar_color`] gives this case its own configurable color
    /// precisely so it renders differently from `Local`'s default, rather than the
    /// false sense of safety of the two looking the same.
    Unknown,
}

/// Resolves the host a terminal session should be matched against, per the module
/// doc's precedence: `session_hostname` (source 1) wins whenever `session_type` is
/// [`SessionType::WarpifiedRemote`]; otherwise `pending_ssh_target` (source 2) is
/// used.
///
/// `pending_ssh_target` is `WarpifyState::pending_ssh_target()`'s tri-state: `None`
/// when no interactive-SSH-shaped command is currently in flight, `Some(None)` when
/// one is but its host could not be parsed, and `Some(Some(host))` when it was.
pub fn resolve_host(
    session_type: &SessionType,
    session_hostname: &str,
    pending_ssh_target: Option<Option<String>>,
) -> ResolvedHost {
    if matches!(session_type, SessionType::WarpifiedRemote { .. }) {
        return if session_hostname.is_empty() {
            ResolvedHost::Unknown
        } else {
            ResolvedHost::Named(session_hostname.to_string())
        };
    }

    match pending_ssh_target {
        None => ResolvedHost::Local,
        Some(None) => ResolvedHost::Unknown,
        Some(Some(host)) => ResolvedHost::Named(host),
    }
}

/// Returns the color of the first rule (in list order) whose pattern matches
/// `host`, or `None` if `host` is not [`ResolvedHost::Named`] or no rule matches.
///
/// First match wins: rules are tried in the order they appear in `rules`, and a
/// later rule matching the same host is never consulted.
pub fn matching_color(
    host: &ResolvedHost,
    rules: &[HostFooterColorRule],
) -> Option<AnsiColorIdentifier> {
    let ResolvedHost::Named(host) = host else {
        return None;
    };

    rules
        .iter()
        .find(|rule| rule.pattern.is_match(host))
        .map(|rule| rule.color)
}

/// Resolves the window-footer-bar color for a session in one call: [`resolve_host`]
/// followed by [`matching_color`], with one addition layered on top --
/// [`ResolvedHost::Unknown`] always yields `unknown_host_color` rather than `None`.
/// That addition lives here, not in `matching_color`, deliberately: see the module
/// doc's "unknown case" for why `matching_color` must stay untouched.
///
/// - [`ResolvedHost::Local`] yields `None` (the default color): an ordinary local
///   shell must not be painted as a caution case.
/// - [`ResolvedHost::Unknown`] yields `Some(unknown_host_color)` unconditionally --
///   independent of `rules`, so a rule that happens to match some unrelated string
///   can never substitute for the dedicated unknown-host color.
/// - [`ResolvedHost::Named`] goes through [`matching_color`] as before.
///
/// Callers cache the result and recompute it only when the session's host, the rule
/// list, or `unknown_host_color` changes -- never during render/layout, see the
/// module doc.
pub fn resolve_footer_bar_color(
    session_type: &SessionType,
    session_hostname: &str,
    pending_ssh_target: Option<Option<String>>,
    rules: &[HostFooterColorRule],
    unknown_host_color: AnsiColorIdentifier,
) -> Option<AnsiColorIdentifier> {
    let host = resolve_host(session_type, session_hostname, pending_ssh_target);
    match host {
        ResolvedHost::Local => None,
        ResolvedHost::Unknown => Some(unknown_host_color),
        ResolvedHost::Named(_) => matching_color(&host, rules),
    }
}

#[cfg(test)]
#[path = "host_footer_color_tests.rs"]
mod tests;
