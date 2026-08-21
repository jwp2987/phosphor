//! Link-opening behavior for notebooks.
use std::{
    borrow::Cow,
    fmt,
    future::{self, Future},
    net::IpAddr,
    path::{Path, PathBuf},
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
    uri::parse_url_paths::{get_item_data_from_warp_link, WarpWebLink},
    workspace::ActiveSession,
};

use super::file::is_markdown_file;

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;

/// Coverage for the URL-scheme policy. These tests are inline rather than in `link_tests.rs`
/// so the policy, its enforcement and its coverage all land in one file.
#[cfg(test)]
mod scheme_policy_tests {
    use url::Url;
    use warpui::{App, ModelHandle, WindowId};

    use super::{LinkTarget, NotebookLinks, ResolveError, SessionSource, is_openable_url_scheme};
    use crate::{ChannelState, workspace::ActiveSession};

    fn parse(url: &str) -> Url {
        Url::parse(url).expect("test URL should parse")
    }

    fn init(app: &mut App) -> ModelHandle<NotebookLinks> {
        let window_id = WindowId::new();
        // No session is registered for this window, so nothing can resolve as a file path and
        // every assertion below is about the scheme decision alone.
        app.add_singleton_model(|_ctx| ActiveSession::default());
        app.add_model(|ctx| NotebookLinks::new(SessionSource::Active(window_id), ctx))
    }

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
                !is_openable_url_scheme(&parse(link)),
                "{link} should not be handed to an OS handler"
            );
        }
    }

    #[test]
    fn web_mail_and_own_schemes_are_openable() {
        for link in [
            "https://example.com/path?q=1",
            "http://example.com/path",
            "HTTPS://example.com",
            "mailto:support@example.com",
        ] {
            assert!(
                is_openable_url_scheme(&parse(link)),
                "{link} should still open"
            );
        }

        let own_scheme_link = format!("{}://action/open-repo", ChannelState::url_scheme());
        assert!(
            is_openable_url_scheme(&parse(&own_scheme_link)),
            "{own_scheme_link} should still open"
        );
    }

    /// The predicate above is only worth anything if `resolve` actually consults it, so assert
    /// on the resolved result rather than on the predicate a second time.
    #[test]
    fn resolve_blocks_dangerous_schemes_and_keeps_https() {
        App::test((), |mut app| async move {
            let links = init(&mut app);

            for link in [
                "javascript:alert(1)",
                "file:///etc/passwd",
                "vscode://file/tmp/payload",
            ] {
                assert_eq!(
                    links
                        .read(&app, |links, ctx| links.resolve(link, ctx))
                        .await,
                    Err(ResolveError::BlockedScheme),
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
}

/// Whether a URL may be handed to the operating system's URL handler.
///
/// Notebook content is not trusted. It is authored by the user, but it is also authored by the
/// model -- AI blocks, comment chips and generated documents all render through this editor --
/// and a link's *scheme* decides which OS-registered program receives it. The allowed set is:
///
/// * `http` / `https` -- the browser. The whole point of a web link.
/// * `mailto` -- the mail composer. It opens a draft; it does not run anything.
/// * the app's own channel scheme (`warp`, `warppreview`, `phosphor`, ...) -- these come back to
///   us, and what they are allowed to mean is separately allow-listed in
///   `uri::web_intent_parser::WebIntent::try_from_url`. It has to stay openable because
///   `set_before_open_url` in `lib.rs` deliberately rewrites recognised web URLs *into* this
///   scheme.
///
/// Everything else is refused, because everything else can reach a program we know nothing
/// about: `javascript:` and `data:` execute in whichever handler claims them, `file:` hands a
/// local path to the system opener (see `open_file` for the matching hardening on the file
/// path), and a custom scheme (`vscode:`, `smb:`, `ms-msdt:`, anything a third-party installer
/// registered) resolves to an arbitrary local binary with an attacker-chosen argument.
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
            // The `url` crate only provides `to_file_path` on certain platforms.
            #[cfg(feature = "local_fs")]
            if url.scheme() == "file" {
                // Unlike below, if there's missing information, we can still fall back to the
                // system for file:// URL handling.
                if let Some(session) = self.session_source.session(ctx) {
                    if let Ok(file) = url.to_file_path() {
                        // TODO(ben): Support line and column in file:// URLs.
                        return Either::Left(Self::resolve_file(file, session, None));
                    }
                }
            }

            // A scheme we will not hand to an OS handler is reported as a broken link rather
            // than resolved, so the tooltip shows why nothing happened instead of the click
            // being silently swallowed.
            if !is_openable_url_scheme(&url) {
                return Either::Right(future::ready(Err(ResolveError::BlockedScheme)));
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
    /// * URLs are opened in the web browser or system-default application, but only if their
    ///   scheme passes `is_openable_url_scheme`; anything else is refused.
    /// * Markdown files are opened in Zap (if the `FileNotebooks` feature flag is enabled).
    /// * Other files are opened in the configured editor or system-default application.
    pub fn open(&self, link: LinkTarget, ctx: &mut ModelContext<Self>) {
        match link {
            LinkTarget::Url(url) => {
                // Defence in depth: `LinkTarget::Url` is a public variant, so a caller can build
                // one without going through `resolve`. Re-check before the URL reaches an OS
                // handler rather than trusting that it was validated on the way in.
                if !is_openable_url_scheme(&url) {
                    log::warn!(
                        "Refusing to open notebook link with disallowed scheme {:?}",
                        url.scheme()
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
        // Images are safe to open with the system default viewer.
        if is_supported_image_file(&path) {
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
    /// The link's URL scheme is not one we hand to an OS handler.
    /// See `is_openable_url_scheme`.
    BlockedScheme,
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
            ResolveError::BlockedScheme => f.write_str("Blocked link: unsupported URL scheme"),
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
