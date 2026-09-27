//! File type detection utilities for determining if files can be opened in Zap.

#[cfg(feature = "local_fs")]
use crate::util::file::external_editor::{settings::EditorChoice, Editor, EditorSettings};
use serde::{Deserialize, Serialize};
use std::path::Path;
use warp_core::features::FeatureFlag;
pub use warp_util::file_type::{
    is_binary_file, is_file_content_binary, is_jupyter_notebook_file, is_markdown_file,
};
pub use warp_util::launch_policy::is_launchable_path;

#[derive(
    Debug,
    Clone,
    Copy,
    Serialize,
    Deserialize,
    PartialEq,
    Eq,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "Layout used when opening files in the editor.",
    rename_all = "snake_case"
)]
pub enum EditorLayout {
    SplitPane,
    NewTab,
}

/// The type of file that can be opened in Zap. The in-product treatment for "opening" a file
/// depends on its type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenableFileType {
    /// A Markdown file, which should be opened in a Markdown viewer pane.
    Markdown,
    /// A code file, which should be opened in a code editor pane.
    Code,
    /// Other types of text files, e.g. txt, csv, svg files, which can still be opened in a code editor pane.
    Text,
}

/// The target application or viewer to use when opening a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileTarget {
    /// Open in Zap's Markdown viewer.
    MarkdownViewer(EditorLayout),
    /// Open in Zap's Code Editor.
    CodeEditor(EditorLayout),
    /// Open in Zap's in-app image viewer.
    ImageViewer(EditorLayout),
    /// Open in an external editor (e.g. VS Code, Emacs).
    #[cfg(feature = "local_fs")]
    ExternalEditor(Editor),
    /// Open in the environment editor ($EDITOR).
    EnvEditor,
    /// Open in the system default application.
    SystemDefault,
    /// Open in the system default application (generic open, e.g. for binary files).
    SystemGeneric,
    /// Reveal the file in Finder / Explorer / the file manager instead of opening it.
    ///
    /// The target for a path the OS default handler would *launch* -- an app bundle, installer,
    /// executable, script, shortcut or macro-bearing document (#681). See
    /// [`is_launchable_path`] for the policy and [`guard_system_handler_target`] for where it
    /// replaces `SystemGeneric` / `SystemDefault`.
    RevealInFileManager,
    /// Open in the OS default application *only if* that application is a known editor (which
    /// reads the file, never runs it); otherwise open in Zap's code editor with this layout.
    ///
    /// The target for launchable *text* -- a `.command`, `.py`, `.desktop` or `+x` script --
    /// under the system-default editor choice (#681). Its OS default handler may be Terminal or
    /// Python Launcher, which would run it; its default editor, when it has one, is what the
    /// user expects to see. Never hands the file to the OS handler.
    DefaultEditorOnly(EditorLayout),
}

impl FileTarget {
    /// Whether this target hands the file to the OS default handler.
    pub fn is_system_handler(&self) -> bool {
        matches!(self, FileTarget::SystemGeneric | FileTarget::SystemDefault)
    }
}

/// The one place the launch policy meets a [`FileTarget`] (#681): an OS-handler target for a
/// path the handler would launch becomes [`FileTarget::RevealInFileManager`]. Every other
/// target is returned unchanged -- the in-app viewers and editors never execute a file, so a
/// `.command` or `.desktop` file may still be *read* in the code editor.
///
/// Callers that build a `SystemGeneric` target by hand at click time (the raster-image
/// shortcuts in the AI document view and notebook links) pass through this; the AI block's
/// shortcut is built on hover, so it stays extension-only and is checked by the workspace sink
/// on click. Either way a path cannot reach the OS handler by skipping [`resolve_file_target`].
///
/// Touches the filesystem (see [`is_launchable_path`]): call it on click, never on hover.
pub fn guard_system_handler_target(path: &Path, target: FileTarget) -> FileTarget {
    if target.is_system_handler() && is_launchable_path(path) {
        FileTarget::RevealInFileManager
    } else {
        target
    }
}

/// The path to reveal for a `file:` URL, when handing that URL to the OS would launch its path
/// (#681): the resolved path the policy checked (symlinks, `..` and `~` resolved). `None` for
/// every other scheme, for a `file:` URL that names another host or does not convert to a path,
/// and for a path that is not [`is_launchable_path`].
///
/// OSC 8 hyperlinks and `file://` URLs printed in terminal output reach `AppContext::open_url`,
/// which bypasses `open_file_path`; callers that accept `file:` URLs must check this first and
/// reveal the path instead. [`before_open_url_launch_policy`] is the backstop.
///
/// `local_fs` only: `Url::to_file_path` does not exist on wasm, and there is no local file to
/// launch there.
#[cfg(feature = "local_fs")]
pub fn launchable_file_url_path(url: &url::Url) -> Option<std::path::PathBuf> {
    if url.scheme() != "file" {
        return None;
    }
    let path = url.to_file_path().ok()?;
    let resolved = warp_util::launch_policy::resolve_for_open(&path);
    resolved.launchable.then_some(resolved.path)
}

/// The `set_before_open_url` backstop for the launch policy (#681). The callback cannot veto an
/// open, but it can rewrite the URL, and `AppContext::open_url` treats a rewrite to `""` as a
/// veto. Returns `Some(rewrite)` to replace the URL, `None` to leave it alone.
///
/// * A `file:` URL naming another host is refused (`""`): on Windows it is a UNC path, i.e. an
///   SMB connection to a host the link chose; on Unix it does not name a local file at all.
/// * A local `file:` URL that does not convert to a path is refused.
/// * A string that is not a URL but is a path -- `/tmp/x.AppImage`, `~/x.app`, `C:\x\setup.exe`
///   (which `Url::parse` reads as scheme `c:`), `\\host\share\x.exe` -- is treated as the path
///   the opener would treat it as: UNC paths are refused, local ones go through the policy.
/// * A local file path or URL that is launchable is rewritten to the `file:` URL of its nearest
///   non-launchable, non-symlinked folder, which the OS opens in the file manager.
#[cfg(feature = "local_fs")]
pub fn before_open_url_launch_policy(url_str: &str) -> Option<String> {
    const REFUSE: &str = "";
    let trimmed = url_str.trim();
    if is_unc_path(trimmed) {
        return Some(REFUSE.to_owned());
    }
    let path = match url::Url::parse(trimmed) {
        Ok(url) if url.scheme() == "file" => {
            if !matches!(url.host_str(), None | Some("") | Some("localhost")) {
                return Some(REFUSE.to_owned());
            }
            match url.to_file_path() {
                Ok(path) => path,
                Err(()) => return Some(REFUSE.to_owned()),
            }
        }
        // A Windows drive path parses as a one-letter scheme; it is a path, not a URL.
        Ok(url) if url.scheme().len() == 1 => std::path::PathBuf::from(trimmed),
        Ok(_) => return None,
        Err(_) if looks_like_local_path(trimmed) => std::path::PathBuf::from(trimmed),
        Err(_) => return None,
    };
    if !is_launchable_path(&path) {
        return None;
    }
    Some(
        warp_util::launch_policy::reveal_directory_for(&path)
            .and_then(|dir| url::Url::from_directory_path(dir).ok())
            .map(String::from)
            .unwrap_or_else(|| REFUSE.to_owned()),
    )
}

/// `\\host\share`, `//host/share` or `\\?\UNC\host\share`: a network path. The verbatim local
/// form `\\?\C:\...` is not one.
#[cfg(feature = "local_fs")]
fn is_unc_path(s: &str) -> bool {
    if s.to_ascii_uppercase().starts_with(r"\\?\UNC\") {
        return true;
    }
    if s.starts_with(r"\\?\") || s.starts_with(r"\\.\") {
        return false;
    }
    let mut chars = s.chars();
    matches!(chars.next(), Some('\\' | '/')) && matches!(chars.next(), Some('\\' | '/'))
}

/// Whether a string that failed to parse as a URL is a local filesystem path.
#[cfg(feature = "local_fs")]
fn looks_like_local_path(s: &str) -> bool {
    s.starts_with('/')
        || s.starts_with('\\')
        || s.starts_with('~')
        || s.starts_with("./")
        || s.starts_with("../")
        || std::path::Path::new(s).is_absolute()
}

/// Checks if a file is a code file with language support.
#[cfg(feature = "local_fs")]
pub fn is_supported_code_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    languages::language_by_local_filename(path).is_some()
}

#[cfg(not(feature = "local_fs"))]
pub fn is_supported_code_file(_path: impl AsRef<Path>) -> bool {
    false
}

/// Whether `path` renders in Zap's notebook viewer (with a Rendered/Raw
/// toggle): Markdown files always, Jupyter notebooks when the feature flag is
/// enabled.
pub fn renders_in_warp_notebook_viewer(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    is_markdown_file(path)
        || (FeatureFlag::JupyterNotebookRendering.is_enabled() && is_jupyter_notebook_file(path))
}

/// Whether `path` is an image Zap can display (in the in-app image viewer, as an inline image,
/// etc.). This includes SVG. Do NOT use it to decide whether a file is safe to hand to the OS
/// default handler -- use [`is_supported_raster_image_file`] for that.
pub fn is_supported_image_file(path: impl AsRef<Path>) -> bool {
    path.as_ref()
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "jpg" | "jpeg" | "png" | "gif" | "webp" | "svg"
            )
        })
        .unwrap_or(false)
}

/// Whether `path` is a supported *raster* image: [`is_supported_image_file`] minus SVG.
///
/// This is the predicate for "may this image be handed to the OS default handler". SVG may not:
/// it is XML that can embed `<script>` and external references, and its registered handler on a
/// normal desktop is a browser, which executes it. `jpg`/`jpeg`/`png`/`gif`/`webp` are decoded by
/// their handler. Paths from model output, notebook links, and AI documents are attacker-namable,
/// so every "open this image in the system viewer" shortcut must use this, and let SVG fall
/// through to [`resolve_file_target`] (in-app image viewer or an editor, never the OS handler).
pub fn is_supported_raster_image_file(path: impl AsRef<Path>) -> bool {
    let path = path.as_ref();
    is_supported_image_file(path)
        && !path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("svg"))
}

/// Returns true if `path` looks like a shell script the user intends to run when
/// "Open with Zap" is invoked from Finder/another app via a `file://` URL.
///
/// Policy: extension in {sh, bash, zsh, fish, ksh} with the user-execute bit set on Unix,
/// or extension in {ps1, bat, cmd} on Windows (no x-bit concept). On Unix, files with no
/// extension but a `#!` shebang and the user-execute bit set also qualify.
///
/// Narrow on purpose: this only affects the URI entry point, not "Open in New Tab" from
/// other UI surfaces, which still want shell scripts viewable in the editor.
/// Returns true if `path` exists and starts with a `#!` shebang. Reads only the
/// first two bytes — the URI entry point is reached from a `file://` URL, so the
/// file is attacker-controlled in size and `std::fs::read` would risk an OOM.
pub(crate) fn starts_with_shebang(path: &Path) -> bool {
    use std::io::Read;
    let mut prefix = [0u8; 2];
    match std::fs::File::open(path) {
        Ok(mut file) => file.read_exact(&mut prefix).is_ok() && prefix == [b'#', b'!'],
        Err(_) => false,
    }
}

#[cfg(unix)]
pub fn is_runnable_shell_script(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    // Match the documented routing policy: only the owner's execute bit counts.
    // A file `chmod 070` belongs to a group, not to the user invoking Zap.
    let has_user_x_bit = std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o100 != 0)
        .unwrap_or(false);
    if !has_user_x_bit {
        return false;
    }
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase());
    if let Some(ext) = ext.as_deref() {
        return matches!(ext, "sh" | "bash" | "zsh" | "fish" | "ksh" | "command");
    }
    starts_with_shebang(path)
}

#[cfg(windows)]
pub fn is_runnable_shell_script(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .is_some_and(|ext| matches!(ext.as_str(), "ps1" | "bat" | "cmd"))
}

#[cfg(not(any(unix, windows)))]
pub fn is_runnable_shell_script(_path: &Path) -> bool {
    false
}

/// Determines if a file can be opened in Zap and returns its type.
/// Returns `None` if the file is binary and should not be opened.
pub fn is_file_openable_in_warp(path: &Path) -> Option<OpenableFileType> {
    if is_binary_file(path) {
        return None;
    }

    if is_markdown_file(path) {
        Some(OpenableFileType::Markdown)
    } else if is_supported_code_file(path) {
        Some(OpenableFileType::Code)
    } else {
        // We allow opening the file, even if we don't have particular syntax highlighting support
        // for it e.g. txt files.
        Some(OpenableFileType::Text)
    }
}

/// Only use this for UI elements that must explicitly open a file in Zap (i.e. "Open in New Tab").
/// Prefer `resolve_file_target` for all other cases to respect users' preferences.
/// This would also force any binary file to be opened in Zap's Code Editor, so you should likely check
/// `is_file_openable_in_warp` before rendering any such UI Elements.
#[cfg(feature = "local_fs")]
pub fn resolve_file_target_to_open_in_warp(
    path: &Path,
    settings: &EditorSettings,
    layout: Option<EditorLayout>,
) -> FileTarget {
    let openable_file_type = is_file_openable_in_warp(path);
    let is_markdown = matches!(openable_file_type, Some(OpenableFileType::Markdown));
    let layout = layout.unwrap_or(*settings.open_file_layout);

    // Jupyter notebooks render in Zap's notebook viewer unconditionally when
    // the feature flag is enabled (the whole point is to avoid raw JSON).
    if openable_file_type.is_some()
        && FeatureFlag::JupyterNotebookRendering.is_enabled()
        && is_jupyter_notebook_file(path)
    {
        return FileTarget::MarkdownViewer(layout);
    }
    if is_markdown && *settings.prefer_markdown_viewer {
        return FileTarget::MarkdownViewer(layout);
    }
    FileTarget::CodeEditor(layout)
}

/// Resolves the target application or viewer for opening a file based on its path and editor settings.
#[cfg(feature = "local_fs")]
pub fn resolve_file_target(
    path: &Path,
    settings: &EditorSettings,
    layout: Option<EditorLayout>,
) -> FileTarget {
    resolve_file_target_with_editor_choice(
        path,
        *settings.open_file_editor,
        *settings.prefer_markdown_viewer,
        *settings.open_file_layout,
        layout,
    )
}

#[cfg(feature = "local_fs")]
pub fn resolve_file_target_with_editor_choice(
    path: &Path,
    editor_choice: EditorChoice,
    prefer_markdown_viewer: bool,
    default_layout: EditorLayout,
    layout: Option<EditorLayout>,
) -> FileTarget {
    let is_openable_in_warp = is_file_openable_in_warp(path);
    let is_markdown = matches!(is_openable_in_warp, Some(OpenableFileType::Markdown));
    let layout = layout.unwrap_or(default_layout);
    let is_openable_in_warp = is_openable_in_warp.is_some();

    // 0. Jupyter notebooks render in Zap's notebook viewer unconditionally
    // when the feature flag is enabled (not gated on `prefer_markdown_viewer`
    // or `editor_choice`, since rendering-instead-of-JSON is the whole point).
    if is_openable_in_warp
        && FeatureFlag::JupyterNotebookRendering.is_enabled()
        && is_jupyter_notebook_file(path)
    {
        return FileTarget::MarkdownViewer(layout);
    }

    // 1. Markdown Viewer (only if user preference specified)
    if is_markdown && prefer_markdown_viewer {
        return FileTarget::MarkdownViewer(layout);
    }

    // 2. Zap Code Editor (Explicit user preference)
    if is_openable_in_warp && matches!(editor_choice, EditorChoice::Zap) {
        return FileTarget::CodeEditor(layout);
    }

    // 3. Env Editor
    if matches!(editor_choice, EditorChoice::EnvEditor) {
        return FileTarget::EnvEditor;
    }

    // 3.5 Image files -> in-app image viewer (before binary fallback)
    if is_supported_image_file(path) {
        return FileTarget::ImageViewer(layout);
    }

    // 4. Binary files -> System Default, unless the handler would launch it (#681): `.app`,
    // `.pkg`, `.exe`, `.msi`, `.docm`, ... are all "binary" and all run when opened.
    if !is_openable_in_warp {
        return guard_system_handler_target(path, FileTarget::SystemGeneric);
    }

    // 5. External Editor or System Default (for text files). Text can be launchable too:
    // `.command`, `.desktop`, `.bat`, `.ps1`, `.py` and executable scripts are all text, and
    // their default handler may run them (#681). Being text, they have safe ways to "open":
    // the platform's default app when that is a known editor (the old route already preferred
    // it), else Zap's code editor. Revealing them instead would turn every click on a
    // traceback's `foo.py`, or on `./configure` on a 0777 mount, into a Finder window.
    match editor_choice {
        EditorChoice::ExternalEditor(editor) => FileTarget::ExternalEditor(editor),
        EditorChoice::SystemDefault if is_launchable_path(path) => {
            FileTarget::DefaultEditorOnly(layout)
        }
        EditorChoice::SystemDefault => FileTarget::SystemDefault,
        EditorChoice::Zap | EditorChoice::EnvEditor => unreachable!("Already matched above"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "local_fs")]
    use settings::Setting as _;
    use std::path::Path;

    #[test]
    fn test_binary_files_not_openable() {
        assert!(is_file_openable_in_warp(Path::new("image.png")).is_none());
        assert!(is_file_openable_in_warp(Path::new("video.mp4")).is_none());
        assert!(is_file_openable_in_warp(Path::new("binary.exe")).is_none());
        assert!(is_file_openable_in_warp(Path::new("archive.zip")).is_none());
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_open_code_panels_file_editor_default_is_warp() {
        use crate::util::file::external_editor::settings::OpenCodePanelsFileEditor;

        assert_eq!(
            OpenCodePanelsFileEditor::default_value(),
            EditorChoice::Zap
        );
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_markdown_viewer_precedence() {
        let target = resolve_file_target_with_editor_choice(
            Path::new("README.md"),
            EditorChoice::ExternalEditor(Editor::VSCode),
            true, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );

        assert_eq!(target, FileTarget::MarkdownViewer(EditorLayout::SplitPane));
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_to_open_in_warp_never_leaves_warp() {
        use crate::util::file::external_editor::settings::{
            OpenCodePanelsFileEditor, OpenConversationLayoutPreference, OpenFileEditor,
            OpenFileLayout, PreferMarkdownViewer, PreferTabbedEditorView,
        };

        let settings = EditorSettings {
            open_file_editor: OpenFileEditor::new(Some(EditorChoice::ExternalEditor(
                Editor::VSCode,
            ))),
            open_code_panels_file_editor: OpenCodePanelsFileEditor::new(Some(
                EditorChoice::ExternalEditor(Editor::VSCode),
            )),
            open_file_layout: OpenFileLayout::new(None),
            prefer_markdown_viewer: PreferMarkdownViewer::new(Some(false)),
            prefer_tabbed_editor_view: PreferTabbedEditorView::new(None),
            open_conversation_layout_preference: OpenConversationLayoutPreference::new(None),
        };
        for path in ["README.md", "data.txt", "main.rs", "image.png", "script.sh"] {
            let target = resolve_file_target_to_open_in_warp(Path::new(path), &settings, None);
            assert!(
                matches!(
                    target,
                    FileTarget::CodeEditor(_) | FileTarget::MarkdownViewer(_)
                ),
                "{path} must resolve to an in-Phosphor surface, got {target:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_warp_uses_default_layout() {
        let target = resolve_file_target_with_editor_choice(
            Path::new("data.txt"),
            EditorChoice::Zap,
            true, /* prefer_markdown_viewer */
            EditorLayout::NewTab,
            None,
        );

        assert_eq!(target, FileTarget::CodeEditor(EditorLayout::NewTab));
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_binary_is_system_generic() {
        let target = resolve_file_target_with_editor_choice(
            Path::new("video.mp4"),
            EditorChoice::Zap,
            true, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );

        assert_eq!(target, FileTarget::SystemGeneric);
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_image_uses_image_viewer() {
        let target = resolve_file_target_with_editor_choice(
            Path::new("photo.png"),
            EditorChoice::Zap,
            true, /* prefer_markdown_viewer */
            EditorLayout::NewTab,
            None,
        );

        assert_eq!(target, FileTarget::ImageViewer(EditorLayout::NewTab));
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_binary_uses_env_editor() {
        let target = resolve_file_target_with_editor_choice(
            Path::new("image.png"),
            EditorChoice::EnvEditor,
            true, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );
        assert_eq!(target, FileTarget::EnvEditor);
    }

    #[test]
    fn test_renders_in_warp_notebook_viewer() {
        // Markdown always renders in the notebook viewer, independent of the flag.
        let off = FeatureFlag::JupyterNotebookRendering.override_enabled(false);
        assert!(renders_in_warp_notebook_viewer(Path::new("README.md")));
        assert!(!renders_in_warp_notebook_viewer(Path::new(
            "notebook.ipynb"
        )));
        assert!(!renders_in_warp_notebook_viewer(Path::new("main.rs")));
        drop(off);

        // With the flag on, Jupyter notebooks also render in the notebook viewer.
        let _on = FeatureFlag::JupyterNotebookRendering.override_enabled(true);
        assert!(renders_in_warp_notebook_viewer(Path::new("notebook.ipynb")));
        assert!(renders_in_warp_notebook_viewer(Path::new("README.md")));
        assert!(!renders_in_warp_notebook_viewer(Path::new("main.rs")));
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_jupyter_notebook_flag_on() {
        let _flag = FeatureFlag::JupyterNotebookRendering.override_enabled(true);
        // Even with prefer_markdown_viewer off and an explicit Zap editor choice,
        // a Jupyter notebook routes to the notebook viewer (not the JSON editor).
        let target = resolve_file_target_with_editor_choice(
            Path::new("analysis.ipynb"),
            EditorChoice::Zap,
            false, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );
        assert_eq!(target, FileTarget::MarkdownViewer(EditorLayout::SplitPane));
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_resolve_file_target_jupyter_notebook_flag_off() {
        let _flag = FeatureFlag::JupyterNotebookRendering.override_enabled(false);
        // With the flag off, a Jupyter notebook opens as JSON in the code editor,
        // exactly as it does today.
        let target = resolve_file_target_with_editor_choice(
            Path::new("analysis.ipynb"),
            EditorChoice::Zap,
            true, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );
        assert_eq!(target, FileTarget::CodeEditor(EditorLayout::SplitPane));
    }

    #[test]
    fn test_markdown_files() {
        assert_eq!(
            is_file_openable_in_warp(Path::new("README.md")),
            Some(OpenableFileType::Markdown)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("doc.markdown")),
            Some(OpenableFileType::Markdown)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("README")),
            Some(OpenableFileType::Markdown)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("CHANGELOG")),
            Some(OpenableFileType::Markdown)
        );
    }

    #[test]
    #[cfg(feature = "local_fs")]
    fn test_code_files() {
        assert_eq!(
            is_file_openable_in_warp(Path::new("main.rs")),
            Some(OpenableFileType::Code)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("app.js")),
            Some(OpenableFileType::Code)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("script.py")),
            Some(OpenableFileType::Code)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("config.json")),
            Some(OpenableFileType::Code)
        );
    }

    #[test]
    #[cfg(not(feature = "local_fs"))]
    fn test_code_files() {
        assert_eq!(
            is_file_openable_in_warp(Path::new("main.rs")),
            Some(OpenableFileType::Text)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("app.js")),
            Some(OpenableFileType::Text)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("script.py")),
            Some(OpenableFileType::Text)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("config.json")),
            Some(OpenableFileType::Text)
        );
    }

    #[test]
    fn test_text_files() {
        // Files that are text but don't have language support
        assert_eq!(
            is_file_openable_in_warp(Path::new("data.txt")),
            Some(OpenableFileType::Text)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("data.csv")),
            Some(OpenableFileType::Text)
        );
        assert_eq!(
            is_file_openable_in_warp(Path::new("file.svg")),
            Some(OpenableFileType::Text)
        );
    }

    #[test]
    fn test_is_supported_code_file() {
        assert!(is_supported_code_file(Path::new("main.rs")));
        assert!(is_supported_code_file(Path::new("app.js")));
        assert!(is_supported_code_file(Path::new("script.py")));
        assert!(!is_supported_code_file(Path::new("data.txt")));
        assert!(!is_supported_code_file(Path::new("image.png")));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_executable_sh() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hello.sh");
        std::fs::write(&p, b"#!/bin/bash\necho hi\n").unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&p, perms).unwrap();
        assert!(is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_non_executable_sh() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("hello.sh");
        std::fs::write(&p, b"#!/bin/bash\necho hi\n").unwrap();
        let mut perms = std::fs::metadata(&p).unwrap().permissions();
        perms.set_mode(0o644);
        std::fs::set_permissions(&p, perms).unwrap();
        assert!(!is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_group_only_executable_rejected() {
        // Mode 0o070: group-x and group-r/w only, no user-execute. Must NOT classify
        // as runnable — only the owner's execute bit drives the routing decision.
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("group_only.sh");
        std::fs::write(&p, b"#!/bin/bash\necho hi\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o070)).unwrap();
        assert!(!is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_other_shell_extensions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        for name in ["run.bash", "run.zsh", "run.fish", "run.ksh"] {
            let p = dir.path().join(name);
            std::fs::write(&p, b"#!/bin/sh\n:\n").unwrap();
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert!(is_runnable_shell_script(&p), "{name} should be runnable");
        }
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_shebang_no_extension() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("noext");
        std::fs::write(&p, b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_shebang_no_extension_no_x_bit() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("noext");
        std::fs::write(&p, b"#!/bin/sh\necho hi\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert!(!is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_plain_text_rejected() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::write(&p, b"just some text\n").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert!(!is_runnable_shell_script(&p));
    }

    #[test]
    #[cfg(unix)]
    fn test_is_runnable_shell_script_symlink_to_executable() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("real.sh");
        std::fs::write(&target, b"#!/bin/sh\n:\n").unwrap();
        std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o755)).unwrap();
        let link = dir.path().join("link.sh");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(is_runnable_shell_script(&link));
    }

    #[test]
    fn test_starts_with_shebang_present() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("script");
        std::fs::write(&p, b"#!/bin/sh\necho hi\n").unwrap();
        assert!(starts_with_shebang(&p));
    }

    #[test]
    fn test_starts_with_shebang_absent() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("plain");
        std::fs::write(&p, b"echo hi\n").unwrap();
        assert!(!starts_with_shebang(&p));
    }

    #[test]
    fn test_starts_with_shebang_one_byte_file() {
        // `read_exact(&mut [0u8; 2])` must short-read on a single-byte file.
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("tiny");
        std::fs::write(&p, b"#").unwrap();
        assert!(!starts_with_shebang(&p));
    }

    #[test]
    fn test_starts_with_shebang_missing_path() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("nope");
        assert!(!starts_with_shebang(&p));
    }

    /// #681: every editor choice, every launchable path -> never the OS default handler.
    /// Binary ones (`.app`, `.pkg`, `.exe`, `.docx`) take step 4 and are revealed; text ones
    /// (`.command`, `.desktop`, `.bat`) take step 5 and open in an editor only.
    #[test]
    #[cfg(feature = "local_fs")]
    fn launchable_paths_never_resolve_to_os_handler() {
        for path in [
            "/tmp/model-named/Evil.app",
            "/tmp/model-named/setup.pkg",
            "/tmp/model-named/image.dmg",
            "/tmp/model-named/setup.exe",
            "/tmp/model-named/setup.msi",
            "/tmp/model-named/pkg.deb",
            "/tmp/model-named/report.docx",
            "/tmp/model-named/report.docm",
            "/tmp/model-named/app.jar",
            "/tmp/model-named/run.command",
            "/tmp/model-named/app.desktop",
            "/tmp/model-named/run.bat",
            "/tmp/model-named/link.webloc",
            "/tmp/model-named/App.AppImage",
        ] {
            for editor_choice in [
                EditorChoice::Zap,
                EditorChoice::EnvEditor,
                EditorChoice::SystemDefault,
                EditorChoice::ExternalEditor(Editor::VSCode),
            ] {
                let target = resolve_file_target_with_editor_choice(
                    Path::new(path),
                    editor_choice,
                    false, /* prefer_markdown_viewer */
                    EditorLayout::SplitPane,
                    None,
                );
                assert!(
                    !target.is_system_handler(),
                    "{path} resolved to {target:?} under {editor_choice:?}"
                );
            }
        }
    }

    /// #681: the issue's exact case -- a binary, launchable path with no override resolves to
    /// Reveal where it used to resolve to `SystemGeneric`.
    #[test]
    #[cfg(feature = "local_fs")]
    fn launchable_binary_resolves_to_reveal() {
        for path in ["Evil.app", "setup.pkg", "setup.exe", "report.xlsx"] {
            let target = resolve_file_target_with_editor_choice(
                Path::new(path),
                EditorChoice::Zap,
                true, /* prefer_markdown_viewer */
                EditorLayout::SplitPane,
                None,
            );
            assert_eq!(target, FileTarget::RevealInFileManager, "{path}");
        }
    }

    /// #681: a launchable *text* file under the system-default editor choice (the default for
    /// `open_file_editor`) opens in the default app only if that is a known editor, else in
    /// Zap's code editor -- never the OS handler. The policy blocks launching, not reading.
    #[test]
    #[cfg(feature = "local_fs")]
    fn launchable_text_file_opens_in_an_editor_not_the_os_handler() {
        for path in [
            "/tmp/run.command",
            "/tmp/app.desktop",
            "/tmp/run.bat",
            "/tmp/build.sh",
            "/tmp/traceback.py",
        ] {
            let target = resolve_file_target_with_editor_choice(
                Path::new(path),
                EditorChoice::SystemDefault,
                false, /* prefer_markdown_viewer */
                EditorLayout::NewTab,
                None,
            );
            assert_eq!(
                target,
                FileTarget::DefaultEditorOnly(EditorLayout::NewTab),
                "{path}"
            );
        }
    }

    /// #681: ordinary documents, media and text keep today's targets.
    #[test]
    #[cfg(feature = "local_fs")]
    fn ordinary_files_keep_their_targets() {
        for path in ["paper.pdf", "video.mp4", "archive.zip", "song.mp3"] {
            let target = resolve_file_target_with_editor_choice(
                Path::new(path),
                EditorChoice::Zap,
                true, /* prefer_markdown_viewer */
                EditorLayout::SplitPane,
                None,
            );
            assert_eq!(target, FileTarget::SystemGeneric, "{path}");
        }
        let text = resolve_file_target_with_editor_choice(
            Path::new("notes.txt"),
            EditorChoice::SystemDefault,
            false, /* prefer_markdown_viewer */
            EditorLayout::SplitPane,
            None,
        );
        assert_eq!(text, FileTarget::SystemDefault);
        let image = resolve_file_target_with_editor_choice(
            Path::new("photo.png"),
            EditorChoice::SystemDefault,
            false, /* prefer_markdown_viewer */
            EditorLayout::NewTab,
            None,
        );
        assert_eq!(image, FileTarget::ImageViewer(EditorLayout::NewTab));
    }

    /// #681: the guard only rewrites OS-handler targets, and only for launchable paths.
    #[test]
    fn guard_rewrites_only_os_handler_targets_for_launchable_paths() {
        let app = Path::new("/tmp/Evil.app");
        let pdf = Path::new("/tmp/paper.pdf");
        for target in [FileTarget::SystemGeneric, FileTarget::SystemDefault] {
            assert_eq!(
                guard_system_handler_target(app, target.clone()),
                FileTarget::RevealInFileManager
            );
            assert_eq!(guard_system_handler_target(pdf, target.clone()), target);
        }
        for target in [
            FileTarget::CodeEditor(EditorLayout::SplitPane),
            FileTarget::MarkdownViewer(EditorLayout::SplitPane),
            FileTarget::ImageViewer(EditorLayout::SplitPane),
            FileTarget::EnvEditor,
            FileTarget::RevealInFileManager,
        ] {
            assert_eq!(guard_system_handler_target(app, target.clone()), target);
        }
    }

    /// #681: `file:` URLs to launchable paths are recognised, including percent-encoded ones;
    /// web URLs and ordinary files are left alone. Unix-only because the literals are Unix
    /// `file:` URLs (on Windows they have no drive letter and do not convert to a path).
    #[test]
    #[cfg(all(unix, feature = "local_fs"))]
    fn launchable_file_url_path_detects_file_urls_only() {
        let parse = |s: &str| url::Url::parse(s).unwrap();
        for launchable in [
            "file:///tmp/Evil.app",
            "file:///tmp/My%20Setup.pkg",
            "file:///tmp/setup.EXE",
            "file://localhost/tmp/run.command",
        ] {
            assert!(
                launchable_file_url_path(&parse(launchable)).is_some(),
                "{launchable}"
            );
        }
        for inert in [
            "file:///tmp/paper.pdf",
            "file:///tmp/notes.txt",
            "https://example.com/Evil.app",
            "http://example.com/setup.exe",
            "mailto:someone@example.com",
        ] {
            assert!(launchable_file_url_path(&parse(inert)).is_none(), "{inert}");
        }
    }

    /// #681: the `set_before_open_url` backstop. A launchable local path or `file:` URL -- in
    /// any spelling the opener accepts, including a bare path that is not a URL at all -- is
    /// rewritten to its nearest non-launchable, resolved folder; network paths and non-local
    /// `file:` URLs are vetoed (`""`); everything else passes through (`None`).
    #[test]
    #[cfg(all(unix, feature = "local_fs"))]
    fn before_open_url_backstop() {
        let dir = tempfile::tempdir().unwrap();
        let base = std::fs::canonicalize(dir.path()).unwrap();
        let folder = |p: &Path| url::Url::from_directory_path(p).unwrap().to_string();
        let file_url = |p: &Path| url::Url::from_file_path(p).unwrap().to_string();
        let base_str = base.to_string_lossy().into_owned();

        // Launchable: file URLs and bare paths, rewritten to the containing folder.
        for launchable in [
            file_url(&base.join("Evil.app")),
            file_url(&base.join("setup.pkg")),
            format!("{base_str}/App.AppImage"),
            format!("  {base_str}/setup.exe  "),
            format!("{base_str}/sub/../Evil.app"),
        ] {
            assert_eq!(
                before_open_url_launch_policy(&launchable),
                Some(folder(&base)),
                "{launchable}"
            );
        }
        // Never a bundle ancestor.
        assert_eq!(
            before_open_url_launch_policy(&file_url(&base.join("Evil.app/Contents/run.command"))),
            Some(folder(&base.join("Evil.app/Contents")))
        );
        assert_eq!(
            before_open_url_launch_policy(&file_url(&base.join("Outer.app/Inner.pkg"))),
            Some(folder(&base))
        );
        // A symlink with a harmless name to a bundle.
        std::fs::create_dir(base.join("Real.app")).unwrap();
        std::os::unix::fs::symlink(base.join("Real.app"), base.join("guide.pdf")).unwrap();
        assert_eq!(
            before_open_url_launch_policy(&file_url(&base.join("guide.pdf"))),
            Some(folder(&base))
        );

        // Refused outright: network paths and non-local file URLs.
        for network in [
            r"\\attacker.example\share\x.exe",
            r"\\attacker.example\share\notes.txt",
            "//attacker.example/share/x.exe",
            r"\\?\UNC\attacker.example\share\x.exe",
            "file://attacker.example/share/x.exe",
            "file://attacker.example/share/notes.txt",
        ] {
            assert_eq!(
                before_open_url_launch_policy(network),
                Some(String::new()),
                "{network}"
            );
        }

        // Passed through unchanged.
        for inert in [
            "https://example.com/Evil.app",
            "mailto:someone@example.com",
            "phosphor://action/x",
            "file:///tmp/paper.pdf",
            "/tmp/paper.pdf",
            "not a path",
        ] {
            assert_eq!(before_open_url_launch_policy(inert), None, "{inert}");
        }
    }

    /// A Windows drive path parses as a one-letter URL scheme; the backstop treats it as the
    /// path it is.
    #[test]
    #[cfg(all(windows, feature = "local_fs"))]
    fn before_open_url_backstop_windows_drive_paths() {
        for launchable in [r"C:\Users\x\Downloads\setup.exe", "c:/x/Evil.msi"] {
            let rewrite = before_open_url_launch_policy(launchable);
            assert!(
                rewrite
                    .as_deref()
                    .is_some_and(|r| r.starts_with("file:///")),
                "{launchable} -> {rewrite:?}"
            );
        }
        assert_eq!(before_open_url_launch_policy(r"C:\x\paper.pdf"), None);
    }

    /// #675: the raster predicate is exactly the display predicate minus SVG. If a format is
    /// added to `is_supported_image_file`, decide deliberately whether it may reach the OS
    /// handler -- this test fails until you do.
    #[test]
    fn raster_image_predicate_is_image_predicate_minus_svg() {
        for raster in ["a.jpg", "a.JPEG", "a.png", "a.gif", "a.webp"] {
            assert!(is_supported_image_file(raster), "{raster}");
            assert!(is_supported_raster_image_file(raster), "{raster}");
        }
        for svg in ["a.svg", "a.SVG", "a.Svg", "/tmp/dir.png/evil.svg"] {
            assert!(is_supported_image_file(svg), "{svg} is still displayable");
            assert!(
                !is_supported_raster_image_file(svg),
                "{svg} must not be treated as a raster image"
            );
        }
        for other in ["a.txt", "a", "a.svgz", "svg", "a.png.exe"] {
            assert!(!is_supported_raster_image_file(other), "{other}");
        }
    }

    /// #675: an SVG that falls through the raster shortcut must resolve to an in-app target
    /// under every editor choice -- never the OS default handler.
    #[test]
    #[cfg(feature = "local_fs")]
    fn svg_never_resolves_to_os_handler() {
        for editor_choice in [
            EditorChoice::Zap,
            EditorChoice::EnvEditor,
            EditorChoice::SystemDefault,
            EditorChoice::ExternalEditor(Editor::VSCode),
        ] {
            for prefer_markdown_viewer in [false, true] {
                let target = resolve_file_target_with_editor_choice(
                    Path::new("/tmp/model-named.svg"),
                    editor_choice,
                    prefer_markdown_viewer,
                    EditorLayout::SplitPane,
                    None,
                );
                assert!(
                    !matches!(
                        target,
                        FileTarget::SystemGeneric | FileTarget::SystemDefault
                    ),
                    "svg resolved to {target:?} under {editor_choice:?}"
                );
            }
        }
    }
}
