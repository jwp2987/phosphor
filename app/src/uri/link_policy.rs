//! Whether a URL that arrived as *content* may be handed to the operating system's URL handler.
//!
//! `AppContext::open_url` / `ViewContext::open_url` pass a string straight to the platform
//! opener. With no scheme check that reaches `file:`, `javascript:`, `data:`, `vscode:`,
//! `ms-msdt:` and the app's own scheme -- i.e. whichever program registered the scheme receives
//! an attacker-chosen argument. `set_before_open_url` cannot close this: its callback is
//! `Fn(&str, &AppContext) -> String` (`crates/warpui_core/src/core/app.rs`), so it can rewrite a
//! URL but it **cannot veto** one. Every call site therefore has to guard itself, and this module
//! is where the AI-block, AI-assistant, banner and context-chip call sites get their answer.
//!
//! The scheme policy itself is **not restated here**. It is
//! [`crate::notebooks::link::is_openable_url_scheme`], the single definition already used by
//! `NotebookLinks::resolve`/`open`, by `set_before_open_url` in `lib.rs` and by the terminal's
//! `openable_terminal_url`; the two functions below narrow it. Copying the allow-list instead
//! would mean a future tightening reaches some sinks and not others.
//!
//! Architecturally `is_openable_url_scheme` wants to live *here*, next to `UriHost`, rather
//! than under `notebooks/`: four subsystems now depend on it and none of them is a notebook
//! concern. Moving it would collide with concurrent work in `notebooks/link.rs`, so the
//! recommendation is recorded and the definition is imported rather than duplicated.
//!
//! **This module is ahead of the oracle, not a parity port.** Pinned Warp `42effe840` passes
//! every one of these URLs to the OS unchecked (`block/view_impl/common.rs:2650`,
//! `block.rs:5284`/`6993`, `block/status_bar.rs:1069`, `banner/view.rs:288`,
//! `ai_assistant/transcript.rs:163`, `ai_assistant/panel.rs:1051`,
//! `context_chips/display_chip.rs:2243` there). Do not "restore" any of it during a re-pin.

use url::Url;
use warpui::{Entity, SingletonEntity as _, ViewContext};

use crate::{
    notebooks::link::is_openable_url_scheme, view_components::DismissibleToast, ChannelState,
    ToastStack,
};

/// Why a URL carried by content was not handed to the OS URL handler.
///
/// The two cases are kept apart deliberately. "This is not a URL at all" and "this is a URL whose
/// scheme we refuse" are different facts, and collapsing them is exactly how the original bug
/// read elsewhere in the tree: `Url::parse(uri).is_err()` treated *every* parseable URI as safe
/// to open, fusing "unparseable" with "allowed".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BlockedContentLink {
    /// The string did not parse as a URL.
    NotAUrl,
    /// It parsed, but its scheme is not one we hand to an OS handler. Carries the scheme so the
    /// user is told *which* one was refused rather than just that something failed.
    DisallowedScheme(String),
}

impl BlockedContentLink {
    /// User-visible explanation, for surfaces that have an error affordance. A click that
    /// silently does nothing is its own bug, so every sink that can show something shows this.
    pub fn toast_text(&self) -> String {
        match self {
            BlockedContentLink::NotAUrl => crate::t!("common-toast-link-invalid"),
            BlockedContentLink::DisallowedScheme(scheme) => {
                crate::t!("common-toast-link-blocked-scheme", scheme = scheme.as_str())
            }
        }
    }

    /// Log line for surfaces with no error affordance (a click handler holding only
    /// `&AppContext` cannot update the toast model). Kept next to `toast_text` so the two
    /// wordings stay in step.
    pub fn log_text(&self) -> String {
        match self {
            BlockedContentLink::NotAUrl => "not a valid URL".to_owned(),
            BlockedContentLink::DisallowedScheme(scheme) => {
                format!("refused scheme {scheme:?}")
            }
        }
    }
}

/// Parse a URL that arrived as **content the app did not author** and decide whether it may be
/// opened.
///
/// This is the predicate for model-authored markdown (AI block rich content and tables, the AI
/// assistant transcript) and for remote- or repository-derived strings (an imported code-review
/// comment's URL, a pull-request URL discovered from the checkout). None of it is written by us,
/// all of it is one plain click from the OS, and it is at least as untrusted as the terminal
/// output already guarded by `openable_terminal_url` -- so it must never be *more* permissive
/// than that.
///
/// The narrowing on top of [`is_openable_url_scheme`] is the app's own channel scheme (`warp`,
/// `warppreview`, `phosphor`, ...), which notebooks allow and this content does not. Our own
/// scheme is not inert: [`crate::uri::UriHost::Launch`] loads a launch configuration and starts
/// every tab and command it defines, and [`crate::uri::UriHost::Action`] covers
/// `NewTab`/`OpenFileEditor` with a URL-supplied path. A model that can emit one link must not
/// be able to start a process. Nor does this content need the own scheme: the
/// `maybe_rewrite_web_url_to_intent` rewrite in `set_before_open_url` that *produces* own-scheme
/// URLs runs downstream of this check, on an already-allowed `https` URL.
///
/// Note this is stricter than `is_openable_notebook_link`, which allows the own scheme for
/// navigation-only hosts. That distinction is deliberate: a notebook is a document the user
/// opened, whereas these surfaces render whatever the model just emitted.
pub fn openable_untrusted_content_url(uri: &str) -> Result<Url, BlockedContentLink> {
    let url = parse_openable(uri)?;

    // `Url::parse` lower-cases the scheme, so this comparison needs no normalisation.
    if url.scheme() == ChannelState::url_scheme() {
        return Err(BlockedContentLink::DisallowedScheme(
            url.scheme().to_owned(),
        ));
    }

    Ok(url)
}

/// Parse a URL that the **app itself composed** and decide whether it may be opened.
///
/// This is the weaker predicate, and it is weaker for a reason rather than by default: the only
/// callers are surfaces whose link targets are compile-time constants -- `Banner`'s formatted
/// text (every construction site passes a `const` URL or a `crate::t!` string) and the static
/// `AGENT_TIPS` list rendered by the AI status bar. For those the own scheme is not an
/// escalation, because whoever authored the URL already runs as the app.
///
/// It still refuses `file:`, `javascript:`, `data:` and every unregistered scheme, which is the
/// half of the hole that matters here: a bad constant is a bug we want reported, not a program
/// launch.
///
/// **Do not reach for this to make an inconvenient refusal go away.** If a surface's link text
/// can come from a model, a server or a repository, it is untrusted no matter which module
/// renders it, and it belongs in [`openable_untrusted_content_url`].
pub fn openable_app_content_url(uri: &str) -> Result<Url, BlockedContentLink> {
    parse_openable(uri)
}

/// Tell the user a link was refused, through the workspace's ordinary error toast.
///
/// A click that silently does nothing is its own bug -- "nothing happened" is indistinguishable
/// from a broken link, and a user who cannot see the refusal will retry it. Every sink that has
/// a `ViewContext` routes its refusals here so the wording stays identical across surfaces; the
/// two that hold only `&AppContext` (the AI-block table-cell handler and the agent-tip handler)
/// cannot reach the toast model and log instead.
pub fn report_blocked_link<V: Entity>(blocked: &BlockedContentLink, ctx: &mut ViewContext<V>) {
    let window_id = ctx.window_id();
    let text = blocked.toast_text();
    ToastStack::handle(ctx).update(ctx, |toast_stack, ctx| {
        toast_stack.add_ephemeral_toast(DismissibleToast::error(text), window_id, ctx);
    });
}

/// The half both predicates share: parse, then apply the one scheme allow-list.
fn parse_openable(uri: &str) -> Result<Url, BlockedContentLink> {
    let Ok(url) = Url::parse(uri) else {
        return Err(BlockedContentLink::NotAUrl);
    };

    if !is_openable_url_scheme(&url) {
        return Err(BlockedContentLink::DisallowedScheme(
            url.scheme().to_owned(),
        ));
    }

    Ok(url)
}

#[cfg(test)]
mod tests {
    use super::{openable_app_content_url, openable_untrusted_content_url, BlockedContentLink};
    use crate::ChannelState;

    fn own(path: &str) -> String {
        format!("{}://{path}", ChannelState::url_scheme())
    }

    /// "Not a URL" and "refused scheme" have to stay distinct: fusing them is the original
    /// defect, and a caller that cannot tell them apart cannot tell the user which happened.
    #[test]
    fn unparseable_input_is_not_reported_as_a_refused_scheme() {
        assert_eq!(
            openable_untrusted_content_url("not a url"),
            Err(BlockedContentLink::NotAUrl)
        );
        assert_eq!(
            openable_untrusted_content_url(""),
            Err(BlockedContentLink::NotAUrl)
        );
        assert_eq!(
            openable_app_content_url("not a url"),
            Err(BlockedContentLink::NotAUrl)
        );
    }

    #[test]
    fn script_and_local_handler_schemes_are_refused_by_both_predicates() {
        for uri in [
            "javascript:alert(1)",
            // `Url::parse` lower-cases the scheme, so a mixed-case spelling is the same scheme.
            "JaVaScRiPt:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "ms-msdt:/id%20PCWDiagnostic",
            "vscode://file/tmp/payload",
            "smb://attacker.example/share",
            // A scheme nobody has heard of is the interesting case: whichever program registered
            // it would receive an attacker-chosen argument.
            "zapzap://whatever",
        ] {
            assert!(
                matches!(
                    openable_untrusted_content_url(uri),
                    Err(BlockedContentLink::DisallowedScheme(_))
                ),
                "{uri} must not reach an OS handler from untrusted content"
            );
            assert!(
                matches!(
                    openable_app_content_url(uri),
                    Err(BlockedContentLink::DisallowedScheme(_))
                ),
                "{uri} must not reach an OS handler from app content either"
            );
        }
    }

    #[test]
    fn web_and_mail_schemes_still_open() {
        for uri in [
            "https://example.com/path?q=1",
            "http://example.com/path",
            "HTTPS://example.com",
            "mailto:support@example.com",
        ] {
            assert!(
                openable_untrusted_content_url(uri).is_ok(),
                "{uri} should still open from untrusted content"
            );
            assert!(
                openable_app_content_url(uri).is_ok(),
                "{uri} should still open from app content"
            );
        }
    }

    /// The own scheme is where the two predicates part company, and the reason is
    /// `UriHost::Launch`: it starts every tab and command a launch configuration defines.
    #[test]
    fn own_scheme_is_refused_for_untrusted_content_and_allowed_for_app_content() {
        for path in [
            "launch/my-config",
            "action/new_tab?path=/etc",
            "action/open_file_editor?path=/etc/passwd",
            "tab_config/anything",
            "home",
        ] {
            let uri = own(path);
            assert_eq!(
                openable_untrusted_content_url(&uri),
                Err(BlockedContentLink::DisallowedScheme(
                    ChannelState::url_scheme().to_owned()
                )),
                "{uri} must not be reachable from model- or remote-authored content"
            );
            assert!(
                openable_app_content_url(&uri).is_ok(),
                "{uri} is composed by the app and should still open"
            );
        }
    }

    /// The verdict is the parsed URL, and callers open *that* rather than the original string,
    /// so the two must agree on what was approved.
    #[test]
    fn approved_url_round_trips() {
        let url = openable_untrusted_content_url("https://example.com/a%20b?q=1#frag")
            .expect("an https URL should be openable");
        assert_eq!(url.as_str(), "https://example.com/a%20b?q=1#frag");
    }
}
