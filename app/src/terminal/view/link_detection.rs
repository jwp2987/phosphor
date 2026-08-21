use std::ops::Deref;

use serde::{Serialize, Serializer};
use url::Url;
use warp_core::channel::ChannelState;

use warpui::{platform::Cursor, ViewContext};

use crate::{
    notebooks::link::is_openable_url_scheme,
    send_telemetry_from_ctx,
    server::telemetry::{LinkOpenMethod, TelemetryEvent},
    terminal::{
        model::{
            grid::grid_handler::Link,
            index::Point,
            terminal_model::{WithinBlock, WithinModel},
            RespectObfuscatedSecrets,
        },
        TerminalModel,
    },
};

cfg_if::cfg_if! {
    if #[cfg(feature = "local_fs")] {
        use crate::{
            terminal::model::grid::grid_handler,
            terminal::ShellLaunchData,
            util::file::{FileLink, absolute_path_if_valid, ShellPathType},
            util::openable_file_type::FileTarget,
        };
        use std::path::PathBuf;
        use unicode_general_category::{get_general_category, GeneralCategory};
        use unicode_width::UnicodeWidthChar;
        use warp_util::path::CleanPathResult;
        use warp_util::path::LineAndColumnArg;
    }
}

use super::{FindLinkArg, TerminalEditor};

// "a/" and "b/" are prefixes specific to Git Diff
#[cfg(feature = "local_fs")]
const PREFIXES_TO_REMOVE: [&str; 2] = ["a/", "b/"];

/// "@" is a suffix that can be added to symlinks. It appears in Git Bash's default configuration
/// for `ls`.
#[cfg(feature = "local_fs")]
const SUFFIXES_TO_REMOVE: [&str; 1] = ["@"];

#[cfg(feature = "local_fs")]
struct TrimmedSentencePunctuation<'a> {
    path: &'a str,
    removed_width: usize,
}

#[cfg(feature = "local_fs")]
fn is_trailing_sentence_punctuation(c: char) -> bool {
    if c == '.' {
        return true;
    }
    if c.is_ascii() {
        return false;
    }
    matches!(
        get_general_category(c),
        GeneralCategory::ClosePunctuation
            | GeneralCategory::FinalPunctuation
            | GeneralCategory::OtherPunctuation
    )
}

/// Strips trailing sentence punctuation from a captured path token when the
/// punctuation is prose around the path rather than a meaningful path component.
///
/// File paths written at the end of a sentence frequently capture the trailing
/// punctuation (e.g. `notes/README.md.` or `notes/README.md，`). On Windows the
/// NT path normalizer silently strips a trailing `.` during path resolution, so
/// without trimming, the captured token keeps the period in both the highlight
/// range and the file extension, defeating extension-based classification (e.g.
/// opening markdown in the viewer instead of as raw text).
///
/// Returns `None` when there is no trailing sentence punctuation, or when a
/// trailing period is part of a `.`/`..` path component (e.g. `.`, `..`, `foo/.`,
/// `foo/..`), which are legitimate path segments and must be preserved.
#[cfg(feature = "local_fs")]
fn path_without_trailing_sentence_punctuation(
    path: &str,
) -> Option<TrimmedSentencePunctuation<'_>> {
    let mut trimmed = path;
    let mut removed_width = 0;

    while let Some(c) = trimmed.chars().next_back() {
        if !is_trailing_sentence_punctuation(c) {
            break;
        }

        let new_trimmed = trimmed.strip_suffix(c)?;
        if new_trimmed.is_empty() {
            break;
        }

        if c == '.' {
            match new_trimmed.chars().next_back() {
                // Empty (`.`) or a dot/separator immediately before the trailing
                // `.` means the period is a real path component (`..`, `foo/.`,
                // `foo\.`), not sentence punctuation.
                None | Some('.') | Some('/') | Some('\\') => break,
                _ => {}
            }
        }

        trimmed = new_trimmed;
        removed_width += UnicodeWidthChar::width(c).unwrap_or(1);
    }

    (removed_width > 0).then_some(TrimmedSentencePunctuation {
        path: trimmed,
        removed_width,
    })
}

/// Highlighted link within a terminal model grid.
#[derive(Debug, Clone)]
pub enum GridHighlightedLink {
    Url(WithinModel<Link>),
    #[cfg(feature = "local_fs")]
    File(WithinModel<FileLink>),
    /// OSC 8 hyperlink span. Carries the URI directly because — unlike `Url`
    /// — it isn't recoverable from the cell text.
    Hyperlink {
        link: WithinModel<Link>,
        uri: String,
    },
}

impl GridHighlightedLink {
    pub fn contains(&self, position: &WithinModel<Point>) -> bool {
        match self {
            GridHighlightedLink::Url(url) => url.contains(position),
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(file_link) => file_link.contains(position),
            GridHighlightedLink::Hyperlink { link, .. } => link.contains(position),
        }
    }

    pub fn tooltip_text(&self) -> String {
        match &self {
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(file_link)
                if file_link
                    .get_inner()
                    .absolute_path()
                    .map(|path| path.is_dir())
                    .unwrap_or(false) =>
            {
                crate::t!("common-open-folder")
            }
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(_) => crate::t!("common-open-file"),
            GridHighlightedLink::Url(_) => crate::t!("common-open-link"),
            GridHighlightedLink::Hyperlink { .. } => crate::t!("common-open-link"),
        }
    }
}

impl Serialize for GridHighlightedLink {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match &self {
            GridHighlightedLink::Url(_) => {
                serializer.serialize_unit_variant("HighlightedLink", 0, "Url")
            }
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(_) => {
                serializer.serialize_unit_variant("HighlightedLink", 1, "File")
            }
            GridHighlightedLink::Hyperlink { .. } => {
                serializer.serialize_unit_variant("HighlightedLink", 2, "Hyperlink")
            }
        }
    }
}

impl TryFrom<GridHighlightedLink> for Link {
    type Error = anyhow::Error;

    fn try_from(value: GridHighlightedLink) -> Result<Self, Self::Error> {
        match value {
            GridHighlightedLink::Url(WithinModel::AltScreen(url)) => Ok(url),
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(WithinModel::AltScreen(file_link)) => Ok(file_link.link),
            GridHighlightedLink::Hyperlink {
                link: WithinModel::AltScreen(link),
                ..
            } => Ok(link),
            _ => Err(anyhow::anyhow!(
                "HighlightedLink is not within the alt screen"
            )),
        }
    }
}

impl TryFrom<GridHighlightedLink> for WithinBlock<Link> {
    type Error = anyhow::Error;

    fn try_from(value: GridHighlightedLink) -> Result<Self, Self::Error> {
        match value {
            GridHighlightedLink::Url(WithinModel::BlockList(url)) => Ok(url),
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(WithinModel::BlockList(file_link)) => {
                Ok(file_link.map(|file_link| file_link.link))
            }
            GridHighlightedLink::Hyperlink {
                link: WithinModel::BlockList(link),
                ..
            } => Ok(link),
            _ => Err(anyhow::anyhow!(
                "HighlightedLink is not within the block list"
            )),
        }
    }
}

/// The highlighted_link state is synced with both the BlockList and AltScreen so that they can
/// use the highlighted_link to override the normal smart-selection behavior. The
/// highlighted_link can, for example, verify that a file path actually exists on disk, and
/// include file paths with spaces. Smart-select can do neither of those things.
/// Since this value must be kept in sync, we need to prevent any mutation of the value outside
/// of this wrapper.
#[derive(Debug, Default)]
pub struct HighlightedLinkOption {
    inner: Option<GridHighlightedLink>,
    /// True if the underlying content has changed such that the link may no longer be valid.
    invalidated: bool,
}

#[derive(Clone, Debug)]
pub enum RichContentLink {
    Url(String),
    #[cfg(feature = "local_fs")]
    FilePath {
        absolute_path: PathBuf,
        line_and_column_num: Option<LineAndColumnArg>,
        target_override: Option<FileTarget>,
    },
}

impl RichContentLink {
    pub fn tooltip_text(&self) -> String {
        match &self {
            #[cfg(feature = "local_fs")]
            RichContentLink::FilePath { absolute_path, .. } if absolute_path.is_dir() => {
                crate::t!("common-open-folder")
            }
            #[cfg(feature = "local_fs")]
            RichContentLink::FilePath { .. } => crate::t!("common-open-file"),
            RichContentLink::Url(_) => crate::t!("common-open-link"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct RichContentLinkTooltipInfo {
    pub link: RichContentLink,
    pub position_id: String,
}

/// Why a URI carried by terminal content was not handed to the OS URL handler.
///
/// The two cases are kept apart deliberately: "this is not a URL at all" and "this is a URL
/// whose scheme we refuse" are different facts, and collapsing them is how the original bug
/// read -- `Url::parse(uri).is_err()` treated *every* parseable URI as safe to open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BlockedTerminalLink {
    /// The URI did not parse as a URL.
    NotAUrl,
    /// The URI parsed, but its scheme is not one we hand to an OS handler.
    DisallowedScheme(String),
}

impl BlockedTerminalLink {
    /// User-visible explanation. A click that silently does nothing is its own bug, so every
    /// refusal below is reported through `TerminalView::show_error_toast`.
    pub(super) fn toast_text(&self) -> String {
        match self {
            BlockedTerminalLink::NotAUrl => crate::t!("terminal-toast-link-invalid"),
            BlockedTerminalLink::DisallowedScheme(scheme) => {
                crate::t!(
                    "terminal-toast-link-blocked-scheme",
                    scheme = scheme.as_str()
                )
            }
        }
    }
}

/// Parse a URI that arrived as terminal content and decide whether it may be handed to the
/// operating system's URL handler.
///
/// Everything this guards is *byte stream the terminal was told to render*: an OSC 8 hyperlink
/// target, a URL smart-selected out of command output, a link in an AI block's rich content. Any
/// of it can come from a remote host over SSH, from `cat` of an attacker-controlled file, or from
/// an agent's own tool output, so it is at least as untrusted as notebook markdown -- and must
/// therefore never be *more* permissive than the notebook policy.
///
/// The scheme policy itself is not restated here. It is
/// [`crate::notebooks::link::is_openable_url_scheme`], the single definition also used by
/// `NotebookLinks::resolve`/`open` and by `set_before_open_url` in `lib.rs`; this function
/// narrows it. (Architecturally that predicate wants to live in `app/src/uri/` rather than under
/// `notebooks/`, since three subsystems now depend on it -- but importing the one definition is
/// strictly better than adding a fourth copy of the policy, which is already a filed defect.)
///
/// The narrowing is the app's own channel scheme (`warp`, `warppreview`, `phosphor`, ...), which
/// notebooks allow and terminal content does not. Our own scheme is not inert: `UriHost::Launch`
/// starts every tab and command a launch configuration defines, and `UriHost::Action` covers
/// `NewTab`/`OpenFileEditor` with a URL-supplied path. Terminal output is less trusted than
/// notebook content, so it does not get to drive those; and unlike the notebook path there is no
/// reason it would need to, because the `set_before_open_url` rewrite that *produces* this scheme
/// runs downstream of this check on an already-allowed `https` URL.
///
/// **This is ahead of the oracle, not a parity port.** Pinned Warp `42effe840` opens any URI that
/// `Url::parse` accepts (`app/src/terminal/view.rs:18513` there), i.e. `file:`, `vscode:` and
/// `ms-msdt:` from an SSH banner all reached the OS handler. Do not "restore" that during a
/// re-pin.
/// Whether a `file:` URL names this machine.
///
/// RFC 8089 allows an empty authority, `localhost`, or a host name. Only the first two mean
/// "here"; anything else is a remote host, which on Windows is a UNC path and on any platform
/// is a host chosen by whoever produced the terminal output. Mirrors
/// `notebooks::link::file_url_is_local`; kept as a local copy rather than an import because
/// that function is private to a module with a different trust model, and the shared
/// scheme policy's natural home (`app/src/uri/`) is where both should eventually live.
fn file_url_is_local(url: &Url) -> bool {
    matches!(url.host_str(), None | Some("") | Some("localhost"))
}

pub(super) fn openable_terminal_url(uri: &str) -> Result<Url, BlockedTerminalLink> {
    let Ok(url) = Url::parse(uri) else {
        return Err(BlockedTerminalLink::NotAUrl);
    };

    // `Url::parse` lower-cases the scheme, so neither comparison needs normalisation.
    //
    // `file:` is allowed only when the authority is local. A build tool or linter printing
    // a clickable path to a local file is the feature OSC 8 exists for and is why this fork
    // ported it (#11, `crates/integration/src/test/osc8_hyperlinks.rs`), so refusing it
    // outright — as this function briefly did on 2026-08-21 — breaks a real workflow for no
    // security gain. What is NOT allowed is a non-local authority: `file://host/share/x` is
    // a UNC path on Windows, so handing it to the OS opens an SMB connection to a host the
    // link names, and terminal output can arrive from a remote machine over SSH. Same rule,
    // and same reason, as `notebooks::link::file_url_is_local`.
    let scheme_allowed = if url.scheme() == "file" {
        file_url_is_local(&url)
    } else {
        is_openable_url_scheme(&url) && url.scheme() != ChannelState::url_scheme()
    };
    if !scheme_allowed {
        return Err(BlockedTerminalLink::DisallowedScheme(
            url.scheme().to_owned(),
        ));
    }

    Ok(url)
}

impl HighlightedLinkOption {
    /// Assigns the inner value and syncs it with the BlockList and AltScreen
    pub fn set(&mut self, link: GridHighlightedLink, model: &mut TerminalModel) {
        match &link {
            GridHighlightedLink::Url(within_model) => match within_model {
                WithinModel::BlockList(within_block) => {
                    let point_range = WithinBlock::new(
                        within_block.inner.range.clone(),
                        within_block.block_index,
                        within_block.grid,
                    );
                    model
                        .block_list_mut()
                        .set_smart_select_override(point_range);
                }
                WithinModel::AltScreen(link) => {
                    model
                        .alt_screen_mut()
                        .set_smart_select_override(link.range.clone());
                }
            },
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(within_model) => match within_model {
                WithinModel::BlockList(within_block) => {
                    let point_range = WithinBlock::new(
                        within_block.inner.link.range.clone(),
                        within_block.block_index,
                        within_block.grid,
                    );
                    model
                        .block_list_mut()
                        .set_smart_select_override(point_range);
                }
                WithinModel::AltScreen(file_link) => {
                    model
                        .alt_screen_mut()
                        .set_smart_select_override(file_link.link.range.clone());
                }
            },
            GridHighlightedLink::Hyperlink {
                link: within_model, ..
            } => match within_model {
                WithinModel::BlockList(within_block) => {
                    let point_range = WithinBlock::new(
                        within_block.inner.range.clone(),
                        within_block.block_index,
                        within_block.grid,
                    );
                    model
                        .block_list_mut()
                        .set_smart_select_override(point_range);
                }
                WithinModel::AltScreen(link) => {
                    model
                        .alt_screen_mut()
                        .set_smart_select_override(link.range.clone());
                }
            },
        }
        self.inner = Some(link);
    }

    /// Wrapper method for Option::take that also keeps the derived state in the BlockList and
    /// AltScreen in sync
    pub fn take(&mut self, model: &mut TerminalModel) -> Option<GridHighlightedLink> {
        model.block_list_mut().clear_smart_select_override();
        model.alt_screen_mut().clear_smart_select_override();
        self.invalidated = false;
        self.inner.take()
    }

    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    pub fn is_invalidated(&self) -> bool {
        self.invalidated
    }

    pub fn clone_inner(&self) -> Option<GridHighlightedLink> {
        self.inner.clone()
    }
}

impl Deref for HighlightedLinkOption {
    type Target = Option<GridHighlightedLink>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl super::TerminalView {
    pub(super) fn maybe_link_hover(
        &mut self,
        position: &Option<WithinModel<Point>>,
        from_editor: TerminalEditor,
        ctx: &mut ViewContext<Self>,
    ) {
        // Do not highlight the url while selecting text or blocks, or if the window is not active.
        if self.terminal_is_selecting(&self.model.lock(), ctx)
            || self.is_navigated_away_from_window(ctx)
        {
            if self.highlighted_link.take(&mut self.model.lock()).is_some() {
                ctx.reset_cursor();
                ctx.notify();
            }
            return;
        }

        // If the mouse isn't in the terminal view, we're not hovering any link.
        let Some(position) = position else {
            if self.highlighted_link.take(&mut self.model.lock()).is_some() {
                ctx.reset_cursor();
                // Clear last_hover_fragment_boundary when mouse is out of block bounds.
                self.last_hover_fragment_boundary = None;
                ctx.notify();
            }
            return;
        };

        // If the mouse is still on top of the previous highlighted link and that link is
        // still valid, we can keep highlighting it.
        if let Some(link) = self.highlighted_link.as_ref() {
            if link.contains(position) && !self.highlighted_link.is_invalidated() {
                // If already hovering on a highlighted link, return.
                return;
            }
        }

        // Updating the cursor shape repeatedly can cause flashing, so we only set it once, and only
        // when necessary.
        let mut new_cursor_shape = None;

        // If a link is highlighted and it's invalidated or we're not hovering it, remove that
        // hover and look for a new one.
        if self.highlighted_link.is_some() {
            // Remove the current highlighted link because we are no longer
            // hovering over it.
            self.highlighted_link.take(&mut self.model.lock());
            new_cursor_shape = Some(Cursor::Arrow);
        }

        let (hyperlink_at_point, url_at_point, new_fragment_boundary) = {
            let model = self.model.lock();
            // OSC 8 wins over auto-detected URLs on the same cell, so check for
            // a hyperlink first and only run the urlocator scan when no OSC 8
            // span covers `position`.
            let hyperlink_at_point = model.hyperlink_at_point(position);
            let url_at_point = if hyperlink_at_point.is_none() {
                model.url_at_point(position)
            } else {
                None
            };
            (
                hyperlink_at_point,
                url_at_point,
                model.fragment_boundary_at_point(position),
            )
        };

        match (
            hyperlink_at_point,
            url_at_point,
            &self.last_hover_fragment_boundary,
        ) {
            (Some((link, uri)), _, _) => {
                self.highlighted_link.set(
                    GridHighlightedLink::Hyperlink { link, uri },
                    &mut self.model.lock(),
                );
                new_cursor_shape = Some(Cursor::PointingHand);
            }
            (None, Some(url), _) => {
                self.highlighted_link
                    .set(GridHighlightedLink::Url(url), &mut self.model.lock());
                new_cursor_shape = Some(Cursor::PointingHand);
            }
            // Only scan for links if the mouse hovered on a new word.
            (_, _, Some(last_hover_fragment_boundary))
                if !last_hover_fragment_boundary.contains(position) =>
            {
                // Use try_send to return an error directly when the channel is full
                // instead of blocking main thread.
                let _ = self.find_link_tx.try_send(FindLinkArg {
                    position: *position,
                    from_editor,
                });
            }
            // If there's no last hover fragment boundary, we scan for links.
            (_, _, None) => {
                let _ = self.find_link_tx.try_send(FindLinkArg {
                    position: *position,
                    from_editor,
                });
            }
            _ => (),
        };

        if let Some(new_cursor_shape) = new_cursor_shape {
            ctx.set_cursor_shape(new_cursor_shape);
            ctx.notify();
        }

        self.last_hover_fragment_boundary = Some(new_fragment_boundary);
    }

    #[cfg_attr(not(feature = "local_fs"), allow(unused_variables))]
    pub(super) fn handle_find_link(
        &mut self,
        find_link_arg: FindLinkArg,
        ctx: &mut ViewContext<Self>,
    ) {
        let FindLinkArg {
            position,
            from_editor,
        } = find_link_arg;

        // Already highlighted the hovered link, returning.
        if self
            .highlighted_link
            .as_ref()
            .is_some_and(|url| url.contains(&position))
        {
            #[cfg_attr(not(feature = "local_fs"), allow(clippy::needless_return))]
            return;
        }

        #[cfg(feature = "local_fs")]
        self.scan_for_file_path(position, from_editor, ctx);
    }

    pub(super) fn open_highlighted_link(
        &mut self,
        link: &GridHighlightedLink,
        ctx: &mut ViewContext<Self>,
    ) {
        self.dismiss_tooltips(ctx);
        ctx.focus(&self.input);
        ctx.notify();

        send_telemetry_from_ctx!(
            TelemetryEvent::OpenLink {
                link: link.clone(),
                open_with: LinkOpenMethod::ToolTip
            },
            ctx
        );
        match link {
            #[cfg(feature = "local_fs")]
            GridHighlightedLink::File(link) => {
                let link = link.get_inner();
                if let Some(path) = link.absolute_path() {
                    self.open_file_path(path.clone(), link.line_and_column_num, ctx);
                }
            }
            GridHighlightedLink::Url(url) => {
                let uri = self
                    .model
                    .lock()
                    .link_at_range(url, RespectObfuscatedSecrets::No);
                // Smart-selected URLs are not limited to `http`/`https`: the scanner's scheme
                // table includes `file`, `ftp` and friends, so a printed `file:///...` is a
                // clickable link here.
                self.open_terminal_content_url(&uri, ctx);
            }
            GridHighlightedLink::Hyperlink { uri, .. } => {
                self.open_terminal_content_url(uri, ctx);
            }
        };
    }

    pub(super) fn open_rich_content_link(
        &mut self,
        link: &RichContentLink,
        ctx: &mut ViewContext<Self>,
    ) {
        self.dismiss_tooltips(ctx);
        ctx.focus(&self.input);
        ctx.notify();

        match link {
            #[cfg(feature = "local_fs")]
            RichContentLink::FilePath {
                absolute_path,
                line_and_column_num,
                target_override,
            } => {
                if let Some(target_override) = target_override {
                    self.open_file_path_with_target(
                        absolute_path.clone(),
                        target_override.clone(),
                        *line_and_column_num,
                        ctx,
                    );
                } else {
                    self.open_file_path(absolute_path.clone(), *line_and_column_num, ctx);
                }
            }
            RichContentLink::Url(url) => {
                // Rich content is rendered from model output, which is no more trusted than the
                // raw byte stream above.
                self.open_terminal_content_url(url, ctx);
            }
        };
    }
}

// A collection of link detection functions that are only valid on platforms
// where we can spawn a local tty.
#[cfg(feature = "local_fs")]
impl super::TerminalView {
    /// Zap: determines whether the given session is a remote-server (SSH) session.
    ///
    /// Always returns `false` when `local_tty` is disabled / on wasm / the
    /// `SshRemoteServer` feature flag is off — i.e. local behavior stays completely
    /// unchanged.
    fn session_is_remote(
        &self,
        session_id: Option<crate::terminal::model::session::SessionId>,
        ctx: &warpui::AppContext,
    ) -> bool {
        #[cfg(all(feature = "local_tty", not(target_family = "wasm")))]
        {
            use warpui::SingletonEntity as _;

            use crate::features::FeatureFlag;
            use crate::remote_server::manager::RemoteServerManager;

            if FeatureFlag::SshRemoteServer.is_enabled() {
                if let Some(session_id) = session_id {
                    return RemoteServerManager::handle(ctx)
                        .as_ref(ctx)
                        .host_id_for_session(session_id)
                        .is_some();
                }
            }
        }

        let _ = (session_id, ctx);
        false
    }

    /// Zap: gets the directory-listing validation context for a remote session's
    /// given cwd.
    ///
    /// On a cache hit, returns `Remote(Some(..))` directly; on a miss,
    /// asynchronously starts a daemon `ListDirectory` RPC to fetch that directory
    /// listing, returning `Remote(None)` this round (not highlighted). Once the
    /// fetch completes, the result is written to the cache and `ctx.notify()` is
    /// called to trigger a re-render that lights up the link.
    ///
    /// The cache stays bounded: fetching a new cwd clears all old entries, keeping
    /// only the current cwd.
    #[cfg(all(
        feature = "local_tty",
        feature = "local_fs",
        not(target_family = "wasm")
    ))]
    fn remote_dir_listing_context(
        &mut self,
        session_id: crate::terminal::model::session::SessionId,
        cwd: &str,
        ctx: &mut ViewContext<Self>,
    ) -> crate::util::file::LinkValidationContext {
        use std::path::PathBuf;
        use std::sync::Arc;

        use warpui::SingletonEntity as _;

        use crate::remote_server::manager::RemoteServerManager;
        use crate::util::file::{LinkValidationContext, RemoteDirListing};

        let cwd_path = PathBuf::from(cwd);
        // The cache is indexed by a composite (session, cwd) key, to avoid the same
        // path on different hosts cross-contaminating each other.
        let cache_key = (session_id, cwd_path.clone());

        // Return directly on a cache hit (whether ready or still fetching).
        if let Some(entry) = self.remote_dir_listing_cache.get(&cache_key) {
            return LinkValidationContext::Remote(entry.clone());
        }

        // Get the daemon client for this session.
        let Some(client) = RemoteServerManager::handle(ctx)
            .as_ref(ctx)
            .client_for_session(session_id)
            .cloned()
        else {
            return LinkValidationContext::Remote(None);
        };

        // Fetching a new cwd: when over capacity, evict the oldest entry FIFO-style
        // by insertion order, then insert a `None` placeholder (marking it as
        // in-flight). `MAX_ENTRIES` is chosen as 8, enough to cover the handful of
        // working directories a user commonly switches between in the terminal,
        // avoiding an RPC every time they switch back.
        const MAX_ENTRIES: usize = 8;
        while self.remote_dir_listing_cache.len() >= MAX_ENTRIES {
            // shift_remove_index preserves insertion order; the FIFO head is the oldest.
            self.remote_dir_listing_cache.shift_remove_index(0);
        }
        self.remote_dir_listing_cache
            .insert(cache_key.clone(), None);

        let cwd_for_request = cwd.to_string();
        let cwd_for_store = cwd_path.clone();
        let key_for_store = cache_key.clone();
        ctx.spawn(
            async move { client.list_directory(cwd_for_request).await },
            move |me, result, ctx| {
                use crate::remote_server::proto::list_directory_response;

                // The user may have switched cwd / cleared the cache while fetching
                // was in progress; only write if the placeholder is still there.
                if !me.remote_dir_listing_cache.contains_key(&key_for_store) {
                    return;
                }
                match result {
                    Ok(resp) => match resp.result {
                        Some(list_directory_response::Result::Success(success)) => {
                            let entries = success
                                .entries
                                .into_iter()
                                .map(|e| (e.name, e.is_dir))
                                .collect();
                            let listing =
                                Arc::new(RemoteDirListing::new(cwd_for_store.clone(), entries));
                            me.remote_dir_listing_cache
                                .insert(key_for_store.clone(), Some(listing));
                            // The listing arrived; trigger a re-render so links are
                            // rescanned and lit up.
                            ctx.notify();
                        }
                        Some(list_directory_response::Result::Error(err)) => {
                            log::warn!(
                                "Remote ListDirectory failed {cwd_for_store:?}: {}",
                                err.message
                            );
                            // Fetch failed: remove the placeholder; it'll retry on the
                            // next hover.
                            me.remote_dir_listing_cache.shift_remove(&key_for_store);
                        }
                        None => {
                            me.remote_dir_listing_cache.shift_remove(&key_for_store);
                        }
                    },
                    Err(err) => {
                        log::warn!("Remote ListDirectory RPC error {cwd_for_store:?}: {err}");
                        me.remote_dir_listing_cache.shift_remove(&key_for_store);
                    }
                }
            },
        );

        LinkValidationContext::Remote(None)
    }

    /// Scans the terminal model at the given position to see if it is
    /// contained within a path that should be linkified.
    fn scan_for_file_path(
        &mut self,
        position: WithinModel<Point>,
        from_editor: TerminalEditor,
        ctx: &mut ViewContext<Self>,
    ) {
        use crate::util::file::LinkValidationContext;

        // Zap: determines whether the session owning the hovered block is a
        // remote-server session. Files in a remote session aren't on the local
        // disk, so `LinkValidationContext::Remote` is needed, carrying the real
        // directory listing fetched by the daemon, for precise validation.
        let block_session_id = match position {
            WithinModel::AltScreen(_) => self.active_block_session_id(),
            WithinModel::BlockList(inner) => self
                .model
                .lock()
                .block_list()
                .block_at(inner.block_index)
                .and_then(|block| block.session_id()),
        };
        let is_remote = self.session_is_remote(block_session_id, ctx);

        // For AltScreen we scan for relative path with the current working directory.
        // For BlockList we scan for relative path with the pwd of the hovered block.
        //
        // Zap: for a remote session, the block's `pwd()` is the remote cwd reported
        // by shell-integration; joining it gives the correct remote absolute path,
        // so remote blocks now also participate in scanning (no longer skipped).
        let pwd_to_scan_for = match position {
            WithinModel::AltScreen(_) => {
                if is_remote {
                    // Remote session: `pwd()` returns the remote active cwd reported by
                    // shell-integration.
                    self.pwd()
                } else {
                    self.pwd_if_local(ctx)
                }
            }
            WithinModel::BlockList(inner) => self
                .model
                .lock()
                .block_list()
                .block_at(inner.block_index)
                .and_then(|block| block.pwd().map(String::from)),
        };

        // Zap: remote sessions use the cached cwd directory listing for precise
        // validation; local sessions stay `Local`.
        let validation_ctx = match (&pwd_to_scan_for, block_session_id) {
            #[cfg(all(feature = "local_tty", not(target_family = "wasm")))]
            (Some(cwd), Some(session_id)) if is_remote => {
                self.remote_dir_listing_context(session_id, cwd, ctx)
            }
            _ => LinkValidationContext::Local,
        };

        match pwd_to_scan_for {
            // Check if we are hovering on any file path. Don't scan for file path
            // if user is hovering from an editor like vim or nano.
            Some(path) if matches!(from_editor, TerminalEditor::No) => {
                let possible_paths = self.model.lock().possible_file_paths_at_point(position);
                let max_columns = self.size_info.columns;
                // Use the hovered block's own launch data, to avoid resolving the
                // path with the wrong shell rules across sessions/hosts/WSL.
                let shell_launch_data = block_session_id
                    .and_then(|session_id| self.sessions.as_ref(ctx).get(session_id))
                    .and_then(|session| session.launch_data().cloned());

                // Using the thread builder instead of ctx.spawn here so that the previous
                // scanning job will be dropped once there is a new scanning job created.
                let (tx, rx) = futures::channel::oneshot::channel();
                self.file_link_scanning_join_handle = std::thread::Builder::new()
                    .name("Compute file paths".into())
                    .spawn(move || {
                        let paths = Self::compute_valid_paths(
                            &path,
                            possible_paths,
                            max_columns,
                            shell_launch_data,
                            validation_ctx,
                        );
                        let _ = tx.send(paths);
                    })
                    .map_err(|e| {
                        log::error!("Unable to spawn thread {e:?}");
                    })
                    .ok();

                let _ = ctx.spawn(
                    async move { rx.await.ok().flatten() },
                    Self::handle_file_link_completed,
                );
            }
            _ if self.highlighted_link.take(&mut self.model.lock()).is_some() => {
                ctx.reset_cursor();
                ctx.notify();
            }
            _ => (),
        };
    }

    fn compute_valid_paths(
        working_directory: &str,
        possible_paths: impl Iterator<Item = WithinModel<grid_handler::PossiblePath>>,
        max_columns: usize,
        shell_launch_data: Option<ShellLaunchData>,
        validation_ctx: crate::util::file::LinkValidationContext,
    ) -> Option<GridHighlightedLink> {
        let mut link = None;
        'path_loop: for within_model_possible_path in possible_paths {
            let possible_path = within_model_possible_path.get_inner();

            // A file path at the end of a sentence often captures trailing prose
            // punctuation (e.g. `notes/README.md.` or `notes/README.md，`). Try the
            // punctuation-trimmed candidate first so the resolved file, highlight
            // range, and extension-based classification all exclude it. This must
            // run before the untrimmed lookup because on Windows the NT path
            // normalizer strips trailing dots, so the untrimmed path would
            // otherwise resolve and leave the period inside the captured link.
            if let Some(trimmed_path) =
                path_without_trailing_sentence_punctuation(&possible_path.path.path)
            {
                let trimmed_cleaned_path = CleanPathResult {
                    path: trimmed_path.path.into(),
                    line_and_column_num: possible_path.path.line_and_column_num,
                };
                if let Some(absolute_path) = absolute_path_if_valid(
                    &trimmed_cleaned_path,
                    ShellPathType::ShellNative(working_directory.to_string()),
                    shell_launch_data.as_ref(),
                    &validation_ctx,
                ) {
                    let new_end_point = possible_path
                        .range
                        .end()
                        .wrapping_sub(max_columns, trimmed_path.removed_width);
                    link = Some(Self::create_valid_link(
                        absolute_path,
                        trimmed_cleaned_path.line_and_column_num,
                        *possible_path.range.start()..=new_end_point,
                        &within_model_possible_path,
                    ));
                    break 'path_loop;
                }
            }

            // We want to check if the clean path result is a valid path and get the canonical
            // absolute path back.
            let absolute_path = absolute_path_if_valid(
                &possible_path.path,
                ShellPathType::ShellNative(working_directory.to_string()),
                shell_launch_data.as_ref(),
                &validation_ctx,
            );

            if let Some(absolute_path) = absolute_path {
                link = Some(Self::create_valid_link(
                    absolute_path,
                    possible_path.path.line_and_column_num,
                    possible_path.range.clone(),
                    &within_model_possible_path,
                ));
                break;
            }

            for prefix in PREFIXES_TO_REMOVE {
                if let Some(new_possible_path) = possible_path.path.path.strip_prefix(prefix) {
                    let new_possible_cleaned_path = CleanPathResult {
                        path: new_possible_path.into(),
                        line_and_column_num: possible_path.path.line_and_column_num,
                    };
                    let absolute_path = absolute_path_if_valid(
                        &new_possible_cleaned_path,
                        ShellPathType::ShellNative(working_directory.to_string()),
                        shell_launch_data.as_ref(),
                        &validation_ctx,
                    );

                    // check if new_possible_path is valid
                    if let Some(absolute_path) = absolute_path {
                        let new_start_point = possible_path
                            .range
                            .start()
                            .wrapping_add(max_columns, prefix.len());

                        link = Some(Self::create_valid_link(
                            absolute_path,
                            new_possible_cleaned_path.line_and_column_num,
                            new_start_point..=*possible_path.range.end(),
                            &within_model_possible_path,
                        ));

                        // break outer_loop
                        break 'path_loop;
                    }
                }
            }

            for suffix in SUFFIXES_TO_REMOVE {
                if let Some(new_possible_path) = possible_path.path.path.strip_suffix(suffix) {
                    let new_possible_cleaned_path = CleanPathResult {
                        path: new_possible_path.into(),
                        line_and_column_num: possible_path.path.line_and_column_num,
                    };
                    let absolute_path = absolute_path_if_valid(
                        &new_possible_cleaned_path,
                        ShellPathType::ShellNative(working_directory.to_string()),
                        shell_launch_data.as_ref(),
                        &validation_ctx,
                    );

                    // check if new_possible_path is valid
                    if let Some(absolute_path) = absolute_path {
                        let new_end_point = possible_path
                            .range
                            .end()
                            .wrapping_sub(max_columns, suffix.len());

                        link = Some(Self::create_valid_link(
                            absolute_path,
                            new_possible_cleaned_path.line_and_column_num,
                            *possible_path.range.start()..=new_end_point,
                            &within_model_possible_path,
                        ));

                        // break outer_loop
                        break 'path_loop;
                    }
                }
            }
        }

        link.map(GridHighlightedLink::File)
    }

    fn create_valid_link(
        absolute_path: PathBuf,
        line_and_column_num: Option<LineAndColumnArg>,
        path_range: std::ops::RangeInclusive<Point>,
        possible_path: &WithinModel<grid_handler::PossiblePath>,
    ) -> WithinModel<FileLink> {
        let inner_link = FileLink {
            link: Link {
                range: path_range,
                is_empty: false,
            },
            absolute_path,
            line_and_column_num,
        };

        match possible_path {
            WithinModel::AltScreen(_) => WithinModel::AltScreen(inner_link),
            WithinModel::BlockList(inner) => {
                WithinModel::BlockList(WithinBlock::new(inner_link, inner.block_index, inner.grid))
            }
        }
    }

    fn handle_file_link_completed(
        &mut self,
        link_result: Option<GridHighlightedLink>,
        ctx: &mut ViewContext<Self>,
    ) {
        let mut model = self.model.lock();
        if self.highlighted_link.take(&mut model).is_some() {
            ctx.reset_cursor();
            ctx.notify();
        }

        if let Some(new_link) = link_result {
            self.highlighted_link.set(new_link, &mut model);
            ctx.set_cursor_shape(Cursor::PointingHand);
            ctx.notify();
        }
    }
}

#[cfg(all(test, feature = "local_fs"))]
#[path = "link_detection_tests.rs"]
mod tests;

/// Coverage for the scheme policy applied to terminal content. Inline (rather than in
/// `link_detection_tests.rs`) so the policy, its enforcement and its coverage stay in one file,
/// and unconditional on `local_fs` because the hole is reachable on every platform.
#[cfg(test)]
mod scheme_policy_tests {
    use warp_core::channel::ChannelState;

    use super::{openable_terminal_url, BlockedTerminalLink};

    #[track_caller]
    fn assert_blocked_scheme(uri: &str, scheme: &str) {
        assert_eq!(
            openable_terminal_url(uri),
            Err(BlockedTerminalLink::DisallowedScheme(scheme.to_owned())),
            "{uri} must not be handed to an OS handler from terminal content"
        );
    }

    #[test]
    fn os_handler_schemes_are_blocked() {
        // The exact payloads a hostile SSH banner, `cat`ted file or tool output would emit.
        assert_blocked_scheme("vscode://file/tmp/payload", "vscode");
        assert_blocked_scheme("ms-msdt:/id%20PCWDiagnostic", "ms-msdt");
        assert_blocked_scheme("javascript:alert(1)", "javascript");
        assert_blocked_scheme("data:text/html,<script>alert(1)</script>", "data");
        assert_blocked_scheme("smb://attacker.example/share", "smb");
        // The URL scanner in `grid_handler` recognises these schemes, so they really do become
        // clickable links from plain command output.
        assert_blocked_scheme("ftp://attacker.example/payload", "ftp");
    }

    /// A `file:` URL naming a REMOTE host is the one that must not open: it is a UNC path on
    /// Windows, so the OS opens an SMB connection to a host the link chose, and terminal output
    /// can arrive from a machine over SSH.
    #[test]
    fn remote_file_authorities_are_blocked() {
        assert_blocked_scheme("file://attacker.example/share/payload", "file");
        assert_blocked_scheme("FILE://attacker.example/share/payload", "file");
    }

    /// The counterpart, and the reason `file:` is not blocked outright: a build tool or linter
    /// printing a clickable path to a local file is what OSC 8 is for. Pinned by
    /// `crates/integration/src/test/osc8_hyperlinks.rs::test_osc8_file_scheme_opens_url`, which
    /// went red when this function briefly refused every `file:` URL.
    #[test]
    fn local_file_urls_still_open() {
        for uri in [
            "file:///tmp/osc8-test.txt",
            "file://localhost/tmp/osc8-test.txt",
            "FILE:///tmp/osc8-test.txt",
        ] {
            assert!(
                openable_terminal_url(uri).is_ok(),
                "{uri} names this machine and must still open"
            );
        }
    }

    /// Terminal content is less trusted than notebook content, so it must not be more
    /// permissive: the app's own scheme reaches `UriHost::Launch` / `UriHost::Action`, and is
    /// refused here even though `is_openable_url_scheme` allows it for notebooks.
    #[test]
    fn the_apps_own_scheme_is_blocked() {
        let scheme = ChannelState::url_scheme();
        assert_blocked_scheme(&format!("{scheme}://action/new-tab?path=/tmp"), scheme);
        assert_blocked_scheme(&format!("{scheme}://launch/whatever"), scheme);
    }

    /// The original check was `Url::parse(uri).is_err()`, which fused "unparseable" with "safe to
    /// open" in the wrong direction. Unparseable input is still refused -- and refused as its own
    /// case, not as a blocked scheme.
    #[test]
    fn unparseable_uris_are_reported_separately() {
        for uri in ["", "not a url", "/etc/passwd", "example.com/no-scheme"] {
            assert_eq!(
                openable_terminal_url(uri),
                Err(BlockedTerminalLink::NotAUrl),
                "{uri:?} is not a URL"
            );
        }
    }

    #[test]
    fn web_and_mail_links_still_open() {
        for uri in [
            "https://example.com/path?q=1",
            "http://example.com/path",
            "HTTPS://example.com/",
            "mailto:support@example.com",
        ] {
            let opened = openable_terminal_url(uri)
                .unwrap_or_else(|err| panic!("{uri} must still open, got {err:?}"));
            assert!(
                matches!(opened.scheme(), "http" | "https" | "mailto"),
                "{uri} resolved to an unexpected scheme {:?}",
                opened.scheme()
            );
        }
    }

    /// Every refusal has something to say. A blocked click that produced an empty toast would be
    /// the silent failure this guard exists to avoid.
    #[test]
    fn every_refusal_has_a_message() {
        // `t!` returns the key itself when the loader is not initialised, which is still
        // non-empty; the assertion that matters is that no arm returns nothing.
        assert!(!BlockedTerminalLink::NotAUrl.toast_text().is_empty());
        let blocked_scheme = BlockedTerminalLink::DisallowedScheme("file".to_owned());
        assert!(!blocked_scheme.toast_text().is_empty());
    }
}
