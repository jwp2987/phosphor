//! Link-opening behavior for notebooks.
use std::{
    borrow::Cow,
    fmt,
    future::{self, Future},
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::Arc,
};

use futures_util::future::Either;
use url::Url;
use warp_util::path::{CleanPathResult, LineAndColumnArg};
use warpui::{
    r#async::SpawnedFutureHandle, AppContext, Entity, ModelContext, ModelHandle, SingletonEntity,
    WindowId,
};

#[cfg(feature = "local_fs")]
use crate::util::file::external_editor::EditorSettings;
#[cfg(feature = "local_fs")]
use crate::util::openable_file_type::{is_supported_image_file, resolve_file_target, FileTarget};
use crate::{
    ChannelState,
    drive::ZapDriveObjectArgs,
    terminal::model::session::Session,
    uri::UriHost,
    uri::parse_url_paths::{get_item_data_from_warp_link, WarpWebLink},
    workspace::ActiveSession,
};

use super::file::is_markdown_file;

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;

/// Coverage for the link policy. These tests are inline rather than in `link_tests.rs` so the
/// policy, its enforcement and its coverage all land in one file.
#[cfg(test)]
mod link_policy_tests {
    use std::{path::Path, sync::Arc};

    use parking_lot::Mutex;
    use tempfile::tempdir;
    use url::Url;
    use warpui::{App, ModelHandle};

    use super::{
        LinkEvent, LinkTarget, NotebookLinks, ResolveError, SessionSource, file_url_is_local,
        is_openable_notebook_link,
    };
    use crate::{
        ChannelState, terminal::model::session::Session, util::openable_file_type::FileTarget,
        workspace::ActiveSession,
    };

    fn parse(url: &str) -> Url {
        Url::parse(url).expect("test URL should parse")
    }

    fn own(path: &str) -> Url {
        parse(&format!("{}://{path}", ChannelState::url_scheme()))
    }

    /// A resolver that **has a session and a working directory**, which is what a real notebook
    /// has.
    ///
    /// The fixture this replaces registered no session, and said so in its own comment ("No
    /// session is registered for this window, so nothing can resolve as a file path"). That made
    /// the `file:///etc/passwd -> Err(Blocked)` assertion it carried vacuous: `resolve`
    /// handles `file:` *before* the allow-list and only falls through to it when there is no
    /// session, so the test proved a refusal production never performs. `SessionSource::Target`
    /// is used here precisely because it cannot be session-less -- it holds a strong `Arc`, so
    /// the session also outlives the weak reference `ActiveSession` keeps.
    fn init(app: &mut App, base_directory: &Path) -> ModelHandle<NotebookLinks> {
        let session = Arc::new(Session::test());
        // `NotebookLinks::new` observes the `ActiveSession` singleton, so it has to exist even
        // though `Target` never reads it.
        app.add_singleton_model(|_ctx| ActiveSession::default());
        // `open` reads `EditorSettings` through production code, so the settings stack has to
        // exist or the read panics before any assertion runs.
        crate::test_util::settings::initialize_settings_for_tests(app);
        let base_directory = base_directory.to_owned();
        app.add_model(|ctx| {
            NotebookLinks::new(
                SessionSource::Target {
                    session,
                    base_directory,
                },
                ctx,
            )
        })
    }

    /// Ensure a file exists, creating its parents if necessary.
    async fn touch(path: &Path) {
        if let Some(parent) = path.parent() {
            async_fs::create_dir_all(parent)
                .await
                .expect("creating parent directory failed");
        }
        async_fs::File::create(path)
            .await
            .expect("creating test file failed")
            .sync_all()
            .await
            .expect("syncing test file failed");
    }

    fn capture_events(
        app: &mut App,
        links: &ModelHandle<NotebookLinks>,
    ) -> Arc<Mutex<Vec<LinkEvent>>> {
        let events = Arc::new(Mutex::new(vec![]));
        let sink = events.clone();
        app.update(|ctx| {
            ctx.subscribe_to_model(links, move |_, event, _| {
                sink.lock().push(event.clone());
            })
        });
        events
    }

    /// Note what this does and does not prove: it is about the predicate only. `file:` in
    /// particular is listed here because the predicate rejects it, but `resolve` never asks the
    /// predicate about a `file:` URL -- see `resolve_keeps_local_file_urls_working` below for
    /// what actually happens to one.
    #[test]
    fn script_and_local_handler_schemes_are_not_openable() {
        for link in [
            "javascript:alert(1)",
            // `Url::parse` lower-cases the scheme, so a mixed-case spelling is the same scheme.
            "JaVaScRiPt:alert(1)",
            "data:text/html,<script>alert(1)</script>",
            "file:///etc/passwd",
            "vbscript:msgbox(1)",
            "ms-msdt:/id%20PCWDiagnostic",
            "vscode://file/tmp/payload",
            "smb://attacker.example/share",
            // A scheme nobody has heard of is exactly the interesting case: whichever program
            // registered it would receive an attacker-chosen argument.
            "zapzap://whatever",
        ] {
            assert!(
                !is_openable_notebook_link(&parse(link)),
                "{link} should not be handed to an OS handler"
            );
        }
    }

    #[test]
    fn web_and_mail_schemes_are_openable() {
        for link in [
            "https://example.com/path?q=1",
            "http://example.com/path",
            "HTTPS://example.com",
            "mailto:support@example.com",
        ] {
            assert!(
                is_openable_notebook_link(&parse(link)),
                "{link} should still open"
            );
        }
    }

    /// The own scheme is not a blanket allow. This is the regression test for the hole the
    /// first version of this guard opened: it allowed the whole scheme, so
    /// `phosphor://launch/<name>` -- run whatever that launch config defines -- was one plain
    /// click away in a `Selectable` (model-authored) view.
    #[test]
    fn own_scheme_is_allowed_only_for_navigation_intents() {
        for link in [
            "launch/my-config",
            "launch/../../etc/passwd",
            "action/new_tab?path=/etc",
            "action/new_window?path=/etc",
            "action/open_file_editor?path=/etc/passwd",
            "action/docker/open_subshell",
            "action/new_agent_conversation",
            "tab_config/anything",
            "mcp/oauth_callback?code=stolen",
            "linear/work_on_issue?issue=1",
            "codex/anything",
            "session/00000000-0000-0000-0000-000000000000",
            "auth/desktop_redirect",
        ] {
            let url = own(link);
            assert!(
                !is_openable_notebook_link(&url),
                "{url} must not be reachable from notebook content"
            );
        }

        for link in [
            "action/open-repo",
            "conversation/abc123",
            "drive/folder/name-id",
            "settings/appearance?theme=dark",
            "home",
        ] {
            let url = own(link);
            assert!(
                is_openable_notebook_link(&url),
                "{url} is navigation-only and should still open"
            );
        }
    }

    /// The predicate is only worth anything if `resolve` consults it, so assert on the resolved
    /// result rather than on the predicate a second time. Unlike the fixture this replaces,
    /// a session is present, so nothing here passes for want of one.
    #[test]
    fn resolve_blocks_dangerous_links_and_keeps_https() {
        App::test((), |mut app| async move {
            let base = tempdir().unwrap();
            let links = init(&mut app, base.path());

            for link in [
                "javascript:alert(1)".to_owned(),
                "vscode://file/tmp/payload".to_owned(),
                own("launch/my-config").to_string(),
                own("action/open_file_editor?path=/etc/passwd").to_string(),
            ] {
                assert_eq!(
                    links
                        .read(&app, |links, ctx| links.resolve(&link, ctx))
                        .await,
                    Err(ResolveError::Blocked),
                    "{link} should not resolve to an openable target"
                );
            }

            assert_eq!(
                links
                    .read(&app, |links, ctx| links
                        .resolve("https://example.com/docs", ctx))
                    .await,
                Ok(LinkTarget::Url(parse("https://example.com/docs"))),
                "a normal web link must still resolve and open"
            );
        });
    }

    /// A `file:` URL with a remote authority must be refused *before* anything touches the
    /// filesystem, because on Windows `to_file_path` turns it into a UNC path and the
    /// `metadata` call in `resolve_file` would open an SMB connection to the attacker's host
    /// during resolution -- on hover, with no user decision.
    ///
    /// This one is a genuine unit test on every platform: the resolve-level assertion further
    /// down is only load-bearing on Windows (elsewhere `to_file_path` rejects a non-empty
    /// authority anyway), which is exactly why the check is also tested directly here.
    #[test]
    fn file_url_authority_decides_local_versus_remote() {
        for local in [
            "file:///etc/passwd",
            "file://localhost/etc/passwd",
            "file://LOCALHOST/x",
        ] {
            assert!(file_url_is_local(&parse(local)), "{local} is a local path");
        }
        for remote in [
            "file://attacker.example/share/payload.txt",
            "file://192.0.2.1/share/x",
        ] {
            assert!(
                !file_url_is_local(&parse(remote)),
                "{remote} names another machine and must never be stat-ed"
            );
        }
    }

    #[test]
    fn resolve_refuses_a_remote_file_url() {
        App::test((), |mut app| async move {
            let base = tempdir().unwrap();
            let links = init(&mut app, base.path());

            assert_eq!(
                links
                    .read(&app, |links, ctx| links
                        .resolve("file://attacker.example/share/payload.txt", ctx))
                    .await,
                Err(ResolveError::Blocked),
                "a file: URL naming another host must not resolve"
            );
        });
    }

    /// The honest statement of what `file:` does in production, replacing the assertion that
    /// only held because the old fixture had no session: with a session, a local `file:` URL
    /// **does** resolve, to a `LocalFile`. What keeps it safe is `open_file`, not the scheme
    /// allow-list.
    #[test]
    fn resolve_keeps_local_file_urls_working() {
        App::test((), |mut app| async move {
            let base = tempdir().unwrap();
            let file = base.path().join("notes.txt");
            touch(&file).await;
            let links = init(&mut app, base.path());

            let url = Url::from_file_path(&file).expect("temp path should convert to a file URL");
            let resolved = links
                .read(&app, |links, ctx| links.resolve(url.as_str(), ctx))
                .await;

            match resolved {
                Ok(LinkTarget::LocalFile { path, .. }) => assert_eq!(path, file),
                other => panic!("expected a local file target for {url}, got {other:?}"),
            }
        });
    }

    /// `.svg` must not take `open_file`'s image early return, which forced
    /// `FileTarget::SystemGeneric` -- the OS default handler, normally a browser, on a document
    /// that can script.
    #[test]
    fn svg_is_not_handed_to_the_system_handler() {
        App::test((), |mut app| async move {
            let base = tempdir().unwrap();
            let svg = base.path().join("payload.svg");
            touch(&svg).await;
            let links = init(&mut app, base.path());
            let events = capture_events(&mut app, &links);

            let url = Url::from_file_path(&svg).expect("temp path should convert to a file URL");
            links
                .update(&mut app, |links, ctx| {
                    let future = links.resolve_and_open(url.as_str(), ctx);
                    ctx.await_spawned_future(future.future_id())
                })
                .await;

            let events = events.lock();
            match events.first() {
                // Whatever it resolved to, it must not be a target the OS default handler owns.
                Some(LinkEvent::OpenFileWithTarget { target, .. }) => assert!(
                    !matches!(
                        target,
                        FileTarget::SystemGeneric | FileTarget::SystemDefault
                    ),
                    "an .svg must not be handed to the OS default handler, got {target:?}"
                ),
                // No event means `open_file` took the dangerous-target arm and revealed the file
                // in Finder / Explorer instead. Also not an OS handler, so also acceptable.
                None => (),
                other => panic!("unexpected LinkEvent for an .svg link: {other:?}"),
            }
        });
    }

    /// The companion to the test above: the early return is narrowed, not deleted. Raster
    /// images still open in the system viewer, which is the pin's behaviour and what
    /// `link_tests.rs::test_open_local_image_uses_system_generic_target` also asserts. If that
    /// trade is ever revisited, both tests should move together.
    #[test]
    fn raster_images_still_use_the_system_viewer() {
        App::test((), |mut app| async move {
            let base = tempdir().unwrap();
            let png = base.path().join("photo.png");
            touch(&png).await;
            let links = init(&mut app, base.path());
            let events = capture_events(&mut app, &links);

            let url = Url::from_file_path(&png).expect("temp path should convert to a file URL");
            links
                .update(&mut app, |links, ctx| {
                    let future = links.resolve_and_open(url.as_str(), ctx);
                    ctx.await_spawned_future(future.future_id())
                })
                .await;

            let events = events.lock();
            match events.first() {
                Some(LinkEvent::OpenFileWithTarget { target, .. }) => {
                    assert_eq!(target, &FileTarget::SystemGeneric)
                }
                other => panic!("expected OpenFileWithTarget for a .png link, got {other:?}"),
            }
        });
    }
}

/// Whether a URL's *scheme* is one we are willing to hand outside the process at all.
///
/// This is the scheme-level half of the policy and deliberately says nothing about what a URL
/// is allowed to *mean*. The allowed set is:
///
/// * `http` / `https` -- the browser. The whole point of a web link.
/// * `mailto` -- the mail composer. It opens a draft; it does not run anything.
/// * the app's own channel scheme (`warp`, `warppreview`, `phosphor`, ...) -- it comes back to
///   us through `uri::handle_incoming_uri`. It has to pass at this level because
///   `set_before_open_url` in `lib.rs` deliberately rewrites recognised web URLs *into* this
///   scheme, and that rewrite must not be discarded as an escalation.
///
/// Everything else is refused, because everything else can reach a program we know nothing
/// about: `javascript:` and `data:` execute in whichever handler claims them, `file:` hands a
/// path to the system opener, and a custom scheme (`vscode:`, `smb:`, `ms-msdt:`, anything a
/// third-party installer registered) resolves to an arbitrary local binary with an
/// attacker-chosen argument.
///
/// **This predicate is NOT sufficient for untrusted content, and it is not what guards `file:`.**
/// Passing it only means the URL may leave the process. For a link that came out of a notebook
/// use [`is_openable_notebook_link`], which additionally constrains what an own-scheme URL may
/// mean; and note that `resolve` handles `file:` on its own path *before* consulting either
/// predicate, so "`file` is absent from the list above" is not what stops a `file:` link.
///
/// The browser build already applies exactly this policy in `warpui::browser::safe_browser_open_url`
/// before calling `window.open`. The desktop build had none: `ctx.open_url` goes straight to
/// `open::that_detached` / `NSWorkspace.openURL`.
///
/// **This is ahead of the oracle, not a parity port.** Pinned Warp `42effe840` returns
/// `LinkTarget::Url` for any scheme `Url::parse` accepts (`link.rs:147`) and calls
/// `ctx.open_url` on it unconditionally (`link.rs:266`), so a plain click on a model-authored
/// `[click me](file:///...)` reached the OS handler. Do not "restore" the pin's behaviour during
/// a re-pin.
pub fn is_openable_url_scheme(url: &Url) -> bool {
    // `Url::parse` lower-cases the scheme, so this comparison needs no normalisation.
    matches!(url.scheme(), "http" | "https" | "mailto")
        || url.scheme() == ChannelState::url_scheme()
}

/// Whether a URL that came out of *notebook content* may be opened.
///
/// Notebook content is not trusted. It is authored by the user, but it is also authored by the
/// model -- AI blocks, comment chips and generated documents all render through this editor --
/// and in `InteractionState::Selectable` views a single plain click opens the link with no
/// modifier (`editor/view.rs`). So this predicate is what decides what one click on
/// model-authored text is allowed to do.
///
/// `http` / `https` / `mailto` pass unchanged: they leave for a browser or a mail composer,
/// neither of which can be handed a local program with an attacker-chosen argument.
///
/// The app's own scheme does **not** pass unchanged, and the first version of this guard was
/// wrong about exactly that. It allowed the whole scheme on the stated grounds that
/// `uri::web_intent_parser::WebIntent::try_from_url`'s `ALLOWED_ACTIONS` was a second gate.
/// **That gate is not on this path.** `try_from_url` is reached only from
/// `maybe_rewrite_web_url_to_intent`, which converts *web* URLs into intent URLs. An own-scheme
/// URL handed to `ctx.open_url` goes to the OS and returns through `lib.rs`'s `on_open_urls`
/// -> `uri::handle_incoming_uri` -> `validate_custom_uri`, which routes on [`UriHost`] and never
/// consults `WebIntent`. That left `phosphor://launch/<name>` -- "load a launch configuration
/// from the user's config directory and dispatch `root_view:open_launch_config`", i.e. start the
/// tabs and run the commands that config defines -- one plain click away from a model.
///
/// So the *meaning* is allow-listed here, against the enum that actually routes the URL. The
/// `match` below is exhaustive on purpose: adding a `UriHost` variant will fail to compile until
/// somebody decides whether untrusted content may reach it. Allow-listing by pointing at a
/// policy in another module is what produced the hole this replaces.
pub fn is_openable_notebook_link(url: &Url) -> bool {
    if !is_openable_url_scheme(url) {
        return false;
    }

    if url.scheme() != ChannelState::url_scheme() {
        // http / https / mailto: there is nothing further to constrain.
        return true;
    }

    // `validate_custom_uri` routes on the host, so route on the host here too rather than
    // pattern-matching the string form.
    let Some(host) = url.host_str() else {
        return false;
    };
    let Ok(host) = UriHost::from_str(host) else {
        return false;
    };

    match host {
        // Navigation only: each of these opens an in-app view. None of them takes a filesystem
        // path, a command, a launch configuration or a credential from the URL.
        UriHost::Conversation | UriHost::Drive | UriHost::Settings | UriHost::Home => true,
        // `action` is mixed, so the host cannot be allowed wholesale: `new_tab` / `new_window`
        // and `open_file_editor` take a `path` from the URL and `docker/open_subshell` starts a
        // shell. `/open-repo` opens the repository picker and takes no argument from the URL
        // (`uri/mod.rs` calls `WorkspaceAction::OpenRepository { path: None }`), and it is also
        // the one action `WebIntent`'s `ALLOWED_ACTIONS` reaches -- mirrored here rather than
        // deferred to, since deferring to it was the bug.
        UriHost::Action => url.path() == "/open-repo",
        // `launch` loads a launch config and runs what it defines; `tab_config` is the same
        // problem in a different file. Neither may ever be reachable from a link a model wrote.
        UriHost::Launch
        | UriHost::TabConfig
        // `mcp` feeds the URL straight into an OAuth callback handler.
        | UriHost::Mcp
        // `linear` builds an agent task out of the URL's parameters.
        | UriHost::Linear
        // `codex` starts an AI session, `session` focuses a terminal pane by UUID, and `auth` is
        // inert in this fork. Refused for want of a reason to allow them rather than for a
        // demonstrated exploit -- an allow-list only works if that is the default answer.
        | UriHost::Codex
        | UriHost::Session
        | UriHost::Auth => false,
    }
}

/// Whether a `file:` URL names a path on *this* machine.
///
/// A `file:` URL may carry an authority component, and `Url::to_file_path` honours it: on
/// Windows `file://host/share/x` becomes the UNC path `\\host\share\x`. `resolve` then calls
/// `async_fs::metadata` on it, which opens an SMB connection to an attacker-named host -- during
/// *resolution*, i.e. on hover or on the first click, before the user has agreed to open
/// anything. So the host is checked before any filesystem call sees the path.
///
/// An empty authority and `localhost` are the two spellings of "this machine" (RFC 8089); the
/// `url` crate lower-cases the host, so `LOCALHOST` needs no separate case.
fn file_url_is_local(url: &Url) -> bool {
    matches!(url.host_str(), None | Some("") | Some("localhost"))
}

/// The target of a notebook link.
#[derive(Debug, Clone)]
pub enum LinkTarget {
    Url(Url),
    LocalFile {
        path: PathBuf,
        line_and_column: Option<LineAndColumnArg>,
        /// The base session when the link was resolved. It's stored here in case it changes
        /// between resolving and opening the link.
        session: Arc<Session>,
        /// Whether or not this file is a Markdown file viewable in Zap.
        is_markdown: bool,
    },
    LocalDirectory {
        path: PathBuf,
    },
}

impl LinkTarget {
    /// A secondary action to show in the tooltip for this link.
    pub fn secondary_action(&self) -> Option<SecondaryAction> {
        match self {
            LinkTarget::LocalDirectory { .. } => Some(SecondaryAction {
                label: crate::t!("notebook-link-new-session").into(),
                tooltip: Some(crate::t!("notebook-link-new-session-tooltip").into()),
                accessibility_content: crate::t!("notebook-link-open-terminal-session").into(),
            }),
            LinkTarget::LocalFile {
                is_markdown: true, ..
            } => Some(SecondaryAction {
                label: crate::t!("notebook-link-open-in-editor").into(),
                tooltip: None,
                accessibility_content: crate::t!("notebook-link-edit-markdown-file").into(),
            }),
            LinkTarget::Url(_) | LinkTarget::LocalFile { .. } => None,
        }
    }
}

impl PartialEq for LinkTarget {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Url(my_url), Self::Url(other_url)) => my_url == other_url,
            (
                Self::LocalFile {
                    path: my_path,
                    line_and_column: my_location,
                    session: my_session,
                    ..
                },
                Self::LocalFile {
                    path: other_path,
                    line_and_column: other_location,
                    session: other_session,
                    ..
                },
            ) => {
                my_path == other_path
                    && my_location == other_location
                    && Arc::ptr_eq(my_session, other_session)
            }
            (Self::LocalDirectory { path: my_path }, Self::LocalDirectory { path: other_path }) => {
                my_path == other_path
            }
            _ => false,
        }
    }
}

impl fmt::Display for LinkTarget {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LinkTarget::Url(url) => url.fmt(f),
            LinkTarget::LocalFile { path, .. } => path.display().fmt(f),
            LinkTarget::LocalDirectory { path, .. } => path.display().fmt(f),
        }
    }
}

/// Model for resolving and opening links in a notebook, taking into account their context (for
/// example, resolving relative file paths).
pub struct NotebookLinks {
    session_source: SessionSource,
}

impl NotebookLinks {
    pub fn new(session_source: SessionSource, ctx: &mut ModelContext<Self>) -> Self {
        ctx.observe(
            &ActiveSession::handle(ctx),
            Self::handle_active_session_change,
        );

        Self { session_source }
    }

    /// Resolve a link target. If the link is a valid URL or starts with a potential domain name,
    /// it's treated as an URL. Otherwise, it's treated as a local file path, possibly with a line
    /// and column number. This returns `None` if the link is known to be invalid (for example, it
    /// resolves to a nonexistent file path).
    pub fn resolve(
        &self,
        link: &str,
        ctx: &AppContext,
    ) -> impl Future<Output = Result<LinkTarget, ResolveError>> + use<> {
        if let Ok(url) = Url::parse(link) {
            // `file:` is handled here and NOT by the allow-list below, which is the trap this
            // branch used to set. A `file:` link that resolves is turned into a
            // `LinkTarget::LocalFile`, so it never reaches `is_openable_notebook_link` at all;
            // what keeps it safe is `open_file`, which refuses to hand a path to the OS default
            // handler. Do not read "`file` is not in the allow-list" as "`file:` links are
            // refused" -- with a session present, which is what a real notebook has, they are
            // not, and a test that asserted otherwise was passing only because its fixture had
            // no session.
            //
            // The `url` crate only provides `to_file_path` on certain platforms.
            #[cfg(feature = "local_fs")]
            if url.scheme() == "file" {
                // Refuse a remote authority before anything touches the filesystem: see
                // `file_url_is_local`. This must stay ahead of `to_file_path`/`metadata`,
                // because the connection attempt *is* the leak -- it happens during resolution,
                // which runs on hover as well as on click.
                if !file_url_is_local(&url) {
                    return Either::Right(future::ready(Err(ResolveError::Blocked)));
                }

                // Unlike below, if there's missing information, we can still fall back to the
                // system for file:// URL handling.
                if let Some(session) = self.session_source.session(ctx) {
                    if let Ok(file) = url.to_file_path() {
                        // TODO(ben): Support line and column in file:// URLs.
                        return Either::Left(Self::resolve_file(file, session, None));
                    }
                }
            }

            // A link we will not act on is reported as broken rather than resolved, so the
            // tooltip shows why nothing happened instead of the click being silently swallowed.
            if !is_openable_notebook_link(&url) {
                return Either::Right(future::ready(Err(ResolveError::Blocked)));
            }

            return Either::Right(future::ready(Ok(LinkTarget::Url(url))));
        }

        // If parsing failed, see if this is a web URL without a scheme.
        // The heuristic we use is to take the substring up to the first slash (if present), and
        // check for a valid public domain name or IP address.
        let maybe_domain = link.split_once('/').map_or(link, |(start, _)| start);
        if addr::parse_domain_name(maybe_domain)
            .is_ok_and(|domain| domain.has_known_suffix() && domain.root().is_some())
            || maybe_domain.parse::<IpAddr>().is_ok()
        {
            if let Ok(url) = Url::parse(&format!("http://{link}")) {
                return Either::Right(future::ready(Ok(LinkTarget::Url(url))));
            }
        }

        // At this point, we can only resolve file targets, which require a session.
        match self.session_source.session(ctx) {
            Some(session) if session.launch_data().is_some() => {
                let launch_data = session
                    .launch_data()
                    .expect("Session launch data should exist");
                let clean_path = CleanPathResult::with_line_and_column_number(link);
                let path = match self.session_source.base_directory(ctx) {
                    Some(base_directory) => {
                        cfg_if::cfg_if! {
                            if #[cfg(feature = "local_fs")] {
                                let Some(path) = crate::util::file::absolute_path_if_valid(
                                    &clean_path,
                                    crate::util::file::ShellPathType::PlatformNative(base_directory.to_path_buf()),
                                    Some(launch_data),
                                    &crate::util::file::LinkValidationContext::Local,
                                ) else {
                                    return Either::Right(future::ready(Err(ResolveError::FileNotFound)));
                                };
                                path
                            } else {
                                // If we don't have a local filesystem, we append the path naively.
                                base_directory.join(clean_path.path)
                            }
                        }
                    }
                    None => {
                        let Some(path) = launch_data.maybe_convert_absolute_path(&clean_path.path)
                        else {
                            return Either::Right(future::ready(Err(ResolveError::MissingContext)));
                        };
                        // To open a relative path, we must have a base directory. Otherwise, we don't know for
                        // sure how the path will be resolved.
                        if path.is_relative() {
                            return Either::Right(future::ready(Err(ResolveError::MissingContext)));
                        }
                        path
                    }
                };

                Either::Left(Self::resolve_file(
                    path,
                    session,
                    clean_path.line_and_column_num,
                ))
            }
            Some(session) => {
                let clean_path_result = CleanPathResult::with_line_and_column_number(link);
                let clean_path = Path::new(&clean_path_result.path);
                let path = if clean_path.is_relative() {
                    // To open a relative path, we must have a base directory. Otherwise, we don't know for
                    // sure how the path will be resolved.
                    match self.session_source.base_directory(ctx) {
                        Some(directory) => directory.join(clean_path),
                        None => {
                            return Either::Right(future::ready(Err(ResolveError::MissingContext)))
                        }
                    }
                } else {
                    clean_path.to_path_buf()
                };

                Either::Left(Self::resolve_file(
                    path,
                    session,
                    clean_path_result.line_and_column_num,
                ))
            }
            None => Either::Right(future::ready(Err(ResolveError::MissingContext))),
        }
    }

    /// Resolve a file path into a [`LinkTarget`], checking if it exists.
    async fn resolve_file(
        path: PathBuf,
        session: Arc<Session>,
        line_and_column: Option<LineAndColumnArg>,
    ) -> Result<LinkTarget, ResolveError> {
        // Every notebook link that reaches an `async_fs::metadata` call funnels through here, so
        // this is the one place a remote path can be stopped before the stat opens a network
        // connection to a host the link named. `file:` URLs are already screened by
        // `file_url_is_local`, but a link can also spell a UNC path *directly*
        // (`\\attacker.example\share\x`), which fails `Url::parse` and so arrives through
        // `resolve`'s plain-path branches instead -- and two of those three branches never pass
        // through `util::file::is_path_valid`, which makes exactly this check for exactly this
        // reason (it notes the stat takes ~15s and hangs the UI, which is the same fact from the
        // performance side). `is_network_resource` deliberately treats WSL's UNC hosts as local.
        #[cfg(windows)]
        if warp_util::path::is_network_resource(&path) {
            return Err(ResolveError::Blocked);
        }

        let metadata = async_fs::metadata(&path).await?;
        Ok(if metadata.is_dir() {
            // Discard line/column information, which doesn't make sense for a directory.
            LinkTarget::LocalDirectory { path }
        } else {
            LinkTarget::LocalFile {
                is_markdown: is_markdown_file(&path),
                path,
                line_and_column,
                session,
            }
        })
    }

    /// Open a resolved link:
    /// * URLs are opened in the web browser or system-default application, but only if they
    ///   pass `is_openable_notebook_link`; anything else is refused.
    /// * Markdown files are opened in Zap (if the `FileNotebooks` feature flag is enabled).
    /// * Other files are opened in the configured editor or system-default application.
    pub fn open(&self, link: LinkTarget, ctx: &mut ModelContext<Self>) {
        match link {
            LinkTarget::Url(url) => {
                // Defence in depth: `LinkTarget::Url` is a public variant, so a caller can build
                // one without going through `resolve`. Re-check before the URL reaches an OS
                // handler rather than trusting that it was validated on the way in.
                //
                // This re-check is safe to apply twice because it is a pure predicate over the
                // *original* URL -- nothing here canonicalises the URL between `resolve` and
                // `open`. That is deliberate: `WebIntent`'s canonical form is not idempotent
                // (a `drive` intent is rebuilt with three path segments collapsed into two plus
                // a query, which no longer parses as a `drive` intent), so validating a rewritten
                // form here would reject links `resolve` had just accepted.
                if !is_openable_notebook_link(&url) {
                    log::warn!(
                        "Refusing to open notebook link: scheme {:?} host {:?} is not reachable \
                         from notebook content",
                        url.scheme(),
                        url.host_str()
                    );
                    return;
                }

                if let Some(WarpWebLink::DriveObject(args)) = get_item_data_from_warp_link(&url) {
                    return ctx.emit(LinkEvent::ZapDriveLink {
                        open_warp_drive_args: *args,
                    });
                }

                ctx.open_url(url.as_str())
            }
            LinkTarget::LocalFile {
                path,
                line_and_column,
                session,
                is_markdown: true,
            } => {
                // Honour the viewer preference. With it disabled the file opens
                // like any other file rather than in the built-in viewer; the
                // fork emitted OpenFileNotebook unconditionally, so the setting
                // had no effect on links at all.
                //
                // EditorSettings only exists with a local filesystem, so without
                // that feature the built-in viewer stays the only option.
                #[cfg(not(feature = "local_fs"))]
                let _ = line_and_column;

                #[cfg(feature = "local_fs")]
                {
                    if *EditorSettings::as_ref(ctx).prefer_markdown_viewer {
                        ctx.emit(LinkEvent::OpenFileNotebook { path, session });
                    } else {
                        open_file(path, line_and_column, ctx);
                    }
                }

                #[cfg(not(feature = "local_fs"))]
                ctx.emit(LinkEvent::OpenFileNotebook { path, session });
            }
            LinkTarget::LocalFile {
                path,
                line_and_column,
                ..
            } => open_file(path, line_and_column, ctx),
            LinkTarget::LocalDirectory { path, .. } => ctx.open_file_path(&path),
        }
    }

    /// Perform the secondary action for this link.
    pub fn secondary_action(&self, link: &LinkTarget, ctx: &mut ModelContext<Self>) {
        match link {
            LinkTarget::LocalDirectory { path } => {
                ctx.emit(LinkEvent::StartLocalSession { path: path.clone() })
            }
            LinkTarget::LocalFile {
                path,
                line_and_column,
                is_markdown: true,
                ..
            } => {
                // The default action for Markdown file links is to open them in Zap. As a
                // secondary action, open them in an external app.
                open_file(path.clone(), *line_and_column, ctx)
            }
            _ => (),
        }
    }

    /// Asynchronously resolve and open a link.
    pub fn resolve_and_open(
        &self,
        link: &str,
        ctx: &mut ModelContext<Self>,
    ) -> SpawnedFutureHandle {
        ctx.spawn(
            self.resolve(link, ctx),
            |me, resolved, ctx| match resolved {
                Ok(link) => me.open(link, ctx),
                // Callers that want to show the failure use `resolve` directly (the editor view
                // renders a broken-link tooltip); log it here so a silently-dropped click at
                // least leaves a trace.
                Err(err) => log::warn!("Not opening link: {err}"),
            },
        )
    }

    pub fn set_session_source(&mut self, source: SessionSource, ctx: &mut ModelContext<Self>) {
        self.session_source = source;
        ctx.emit(LinkEvent::RefreshLinks);
    }

    /// Listen for session changes that might invalidate resolved links.
    fn handle_active_session_change(
        &mut self,
        _handle: ModelHandle<ActiveSession>,
        ctx: &mut ModelContext<Self>,
    ) {
        // Re-resolve links against the new session info, especially if the working directory
        // changed.
        if matches!(self.session_source, SessionSource::Active(_)) {
            ctx.emit(LinkEvent::RefreshLinks);
        }
    }
}

/// Whether `path` is an image format that can carry executable content, and so must never be
/// handed to the OS default handler.
///
/// SVG is the only such format in `is_supported_image_file`'s list: it is XML, it can embed
/// `<script>` and external references, and its registered handler on a normal desktop is a
/// browser. `jpg`/`jpeg`/`png`/`gif`/`webp` are raster formats that the handler decodes.
#[cfg(feature = "local_fs")]
fn is_scripting_image_file(path: &Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// Open a file respecting user's editor settings.
///
/// For targets that would be handed to the OS default handler (`SystemGeneric` /
/// `SystemDefault`), we reveal the file in Finder / Explorer instead of opening it.
/// This prevents a malicious markdown link from triggering arbitrary code execution
/// via an executable disguised as a local file (e.g. an extensionless shell script).
// The `line_and_column` argument is unused when there is no local filesystem.
#[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
fn open_file(
    path: PathBuf,
    line_and_column: Option<LineAndColumnArg>,
    ctx: &mut ModelContext<NotebookLinks>,
) {
    #[cfg(feature = "local_fs")]
    {
        // Raster images are safe to open with the system default viewer. SVG is not, even though
        // `is_supported_image_file` accepts it: an SVG is a scripting document whose default
        // handler is normally a browser, so this early return handed `[x](file:///tmp/x.svg)` to
        // a handler that executes the file's contents -- and it returns *before* the
        // dangerous-target arm below that exists to stop exactly that. Letting SVG fall through
        // routes it to `FileTarget::ImageViewer` (the in-app viewer, which decodes rather than
        // executes) or to the code editor, both of which are already treated as safe targets.
        //
        // The exclusion lives here rather than in `is_supported_image_file` because that
        // predicate has four other callers that mean "can we display this as an image", which is
        // still true of SVG. The right shape is a separate `is_supported_raster_image_file` in
        // `util::openable_file_type`; that file is outside this change and the split is recorded
        // as a follow-up instead.
        if is_supported_image_file(&path) && !is_scripting_image_file(&path) {
            ctx.emit(LinkEvent::OpenFileWithTarget {
                path,
                target: FileTarget::SystemGeneric,
                line_col: line_and_column,
            });
            return;
        }

        let settings = EditorSettings::as_ref(ctx);
        let target = resolve_file_target(&path, settings, None);
        match target {
            // Safe targets: open in a viewer/editor that won't execute the file.
            FileTarget::MarkdownViewer(_)
            | FileTarget::CodeEditor(_)
            | FileTarget::ExternalEditor(_)
            | FileTarget::EnvEditor => {
                ctx.emit(LinkEvent::OpenFileWithTarget {
                    path,
                    target,
                    line_col: line_and_column,
                });
            }
            // Dangerous targets: the OS default handler could execute the file.
            // Reveal in Finder / Explorer instead.
            FileTarget::SystemGeneric | FileTarget::SystemDefault => {
                ctx.open_file_path_in_explorer(&path);
            }
            FileTarget::ImageViewer(_) => {
                ctx.emit(LinkEvent::OpenFileWithTarget {
                    path,
                    target,
                    line_col: line_and_column,
                });
            }
        }
    }
    #[cfg(not(feature = "local_fs"))]
    ctx.open_file_path(&path);
}

impl Entity for NotebookLinks {
    type Event = LinkEvent;
}

/// An error resolving a file link.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolveError {
    /// The target file does not exist.
    FileNotFound,
    /// The context needed to resolve a file is missing.
    MissingContext,
    /// The link is one we refuse to act on from notebook content: a scheme we will not hand to
    /// an OS handler, an own-scheme intent that is not navigation-only, or a `file:` URL naming
    /// another host. See `is_openable_notebook_link` and `file_url_is_local`.
    ///
    /// Named for the decision, not for the scheme. The first spelling was `BlockedScheme`, which
    /// stopped being true as soon as the guard started refusing links on something other than
    /// their scheme.
    Blocked,
    Unknown,
}

impl From<std::io::Error> for ResolveError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            ResolveError::FileNotFound
        } else {
            ResolveError::Unknown
        }
    }
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ResolveError::FileNotFound => f.write_str("File not found"),
            ResolveError::MissingContext => f.write_str("No base directory"),
            ResolveError::Blocked => {
                f.write_str("Blocked link: this link cannot be opened from a document")
            }
            ResolveError::Unknown => f.write_str("Broken file link"),
        }
    }
}

#[derive(Debug, Clone)]
pub enum LinkEvent {
    /// Emitted when the view should open a Markdown file as a notebook.
    OpenFileNotebook {
        path: PathBuf,
        session: Arc<Session>,
    },
    ZapDriveLink {
        open_warp_drive_args: ZapDriveObjectArgs,
    },
    /// This event tells the parent pane group to open a new terminal session in the given
    /// directory.
    StartLocalSession { path: PathBuf },
    /// Signal to views that they should re-resolve links because the backing context for
    /// resolution has changed.
    RefreshLinks,
    #[cfg(feature = "local_fs")]
    /// Emitted when a file should be opened in Zap (code editor or markdown viewer).
    OpenFileWithTarget {
        path: PathBuf,
        target: FileTarget,
        line_col: Option<LineAndColumnArg>,
    },
}

/// A secondary action for a link, besides opening it.
#[derive(Debug, Clone)]
pub struct SecondaryAction {
    pub label: Cow<'static, str>,
    pub tooltip: Option<Cow<'static, str>>,
    pub accessibility_content: Cow<'static, str>,
}

/// Source for the [`Session`] and working directory to use when opening Markdown files as notebooks.
pub enum SessionSource {
    /// Use the specific target session and directory.
    Target {
        session: Arc<Session>,
        base_directory: PathBuf,
    },
    /// Use the window's active session and working directory.
    Active(WindowId),
}

impl SessionSource {
    fn session(&self, ctx: &AppContext) -> Option<Arc<Session>> {
        match self {
            SessionSource::Target { session, .. } => Some(session.clone()),
            SessionSource::Active(window_id) => ActiveSession::as_ref(ctx).session(*window_id),
        }
    }

    fn base_directory<'a>(&'a self, ctx: &'a AppContext) -> Option<&'a Path> {
        match self {
            SessionSource::Target { base_directory, .. } => Some(base_directory.as_path()),
            SessionSource::Active(window_id) => {
                ActiveSession::as_ref(ctx).path_if_local(*window_id)
            }
        }
    }
}
