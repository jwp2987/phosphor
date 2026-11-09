use std::env;
use std::fs;
use std::io::{self, Write};
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock};

use async_trait::async_trait;

use super::{
    CliAgentPluginManager, ConfigFileCheckCache, PluginCommandRunner, PluginInstallError,
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, bundle_missing_error,
    bundled_plugin_dir, is_warp_plugin_source, looks_like_bundle_path, same_folder,
};
use crate::terminal::shell::ShellType;

/// Overrides where the bundled plugin folder is looked up (unit tests, app tests and dev
/// builds that run outside an installed bundle). Unset in a normal install.
pub(crate) const BUNDLE_DIR_ENV: &str = "PHOSPHOR_OPENCODE_PLUGIN_DIR";
/// Where the plugin lives inside the app's resources (`resources/bundled/opencode_plugin` in
/// the repo): an npm-shaped folder (`package.json` with `exports` for `./server` and
/// `./tui`, and its built `dist/`). Phosphor writes a `file://` entry for it into the
/// `plugin` array of OpenCode's global config; nothing is fetched and no npm install runs.
const BUNDLE_RELATIVE: &str = "bundled/opencode_plugin";
const BUNDLE_MARKER: &str = "package.json";

/// Warp's npm package, which existing installs name in their `plugin` array.
const WARP_NPM_PACKAGE: &str = "@warp-dot-dev/opencode-warp";
/// The repository name, shared by Warp's and Phosphor's earlier copies.
const REPO_NAME: &str = "opencode-warp";
/// Phosphor's earlier GitLab copy (a git spec), which entries written by older builds name.
const OLD_PHOSPHOR_REPO_PATH: &str = "winters-phosphor/opencode-warp";
/// The global config files OpenCode reads, in its own order
/// (`packages/core/src/config/discovery.ts`).
const CONFIG_FILE_NAMES: [&str; 2] = ["opencode.json", "opencode.jsonc"];

/// The plugin version shipped in `resources/bundled/opencode_plugin` (a unit test pins this to
/// that folder's `package.json`), compared against the version the running plugin reports
/// over OSC 777. Updates arrive with Phosphor releases.
const MINIMUM_PLUGIN_VERSION: &str = "0.2.0";

/// The folder holding the plugin that ships with this build, if it can be found
/// (`$PHOSPHOR_OPENCODE_PLUGIN_DIR`, else the app's bundled resources).
pub(crate) fn bundled_plugin_folder() -> Option<PathBuf> {
    bundled_plugin_dir(BUNDLE_DIR_ENV, BUNDLE_RELATIVE, BUNDLE_MARKER)
}

/// The `plugin` entry for the bundled folder: its `file://` URL.
pub(crate) fn plugin_spec_for(bundle: &Path) -> Option<String> {
    url::Url::from_file_path(bundle).ok().map(String::from)
}

/// The `plugin` entry for this install's bundled folder, if there is one.
pub(crate) fn plugin_spec() -> Option<String> {
    bundled_plugin_folder().and_then(|bundle| plugin_spec_for(&bundle))
}

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();
/// `is_installed` is asked from the footer's render path too.
static INSTALLED_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the bundle was found and the
/// rewrite ran, whatever its outcome); after it the move chip is not offered again in this
/// process. A move with no bundle to move to does not set it.
static SOURCE_MOVE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Serialises this process's moves; each re-checks once it holds the lock.
static OPERATION_LOCK: LazyLock<futures::lock::Mutex<()>> =
    LazyLock::new(|| futures::lock::Mutex::new(()));

pub(super) struct OpenCodePluginManager {
    runner: Arc<dyn PluginCommandRunner>,
}

impl OpenCodePluginManager {
    pub(super) fn new(
        shell_path: Option<PathBuf>,
        shell_type: Option<ShellType>,
        path_env_var: Option<String>,
    ) -> Self {
        Self::with_runner(Arc::new(ShellCommandRunner::new(
            shell_path,
            shell_type,
            path_env_var,
        )))
    }

    pub(super) fn with_runner(runner: Arc<dyn PluginCommandRunner>) -> Self {
        Self { runner }
    }
}

#[async_trait]
impl CliAgentPluginManager for OpenCodePluginManager {
    fn minimum_plugin_version(&self) -> &'static str {
        MINIMUM_PLUGIN_VERSION
    }

    /// Installing means adding an entry to the user's own config, which the
    /// user does from the instructions; Phosphor does not add it unasked. A
    /// version update is manual too (the instructions modal). Only the move
    /// chip edits the config, when the user clicks it.
    fn can_auto_install(&self) -> bool {
        false
    }

    fn is_installed(&self) -> bool {
        let files = global_config_files();
        INSTALLED_CHECK.get_or_compute(&files, || {
            files
                .iter()
                .any(|path| !plugin_entry_kinds_in_file(path).is_empty())
        })
    }

    fn needs_source_migration(&self) -> bool {
        if SOURCE_MOVE_ATTEMPTED.load(Ordering::Relaxed) {
            return false;
        }
        let Some(bundle) = bundled_plugin_folder() else {
            return false;
        };
        let files = global_config_files();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            files
                .iter()
                .any(|path| file_needs_source_migration(path, &bundle))
        })
    }

    /// Rewrites every remote-sourced entry (Warp's package, Warp's or an older Phosphor git
    /// spec) and every stale bundle-folder entry in the global OpenCode config to the
    /// `file://` entry of the bundled folder ([`migrate_plugin_entries`]): only that entry
    /// changes, so the user's other plugins, options, comments and formatting stay as they
    /// were. A `file://` or path entry of the user's own is never touched. Each file is
    /// replaced atomically ([`write_config_atomically`]); if anything fails the original is
    /// untouched. Every step is local.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let log = String::new();
        let Some(bundle) = bundled_plugin_folder() else {
            return Err(bundle_missing_error("OpenCode", log));
        };
        let mut log = log;
        let files = global_config_files();
        if !files
            .iter()
            .any(|path| file_needs_source_migration(path, &bundle))
        {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        }
        let result = rewrite_config_files(&files, &bundle, &mut log);
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        result.map_err(|message| PluginInstallError { message, log })
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to the copy bundled with Phosphor. Please restart OpenCode to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }
}

/// `"plugin": ["file:///<bundled folder>"]`, as text a user can paste into their config. The
/// folder is resolved once; a build run outside an install shows a placeholder path.
fn plugin_entry_text() -> &'static str {
    static TEXT: LazyLock<&'static str> = LazyLock::new(|| {
        let spec = plugin_spec().unwrap_or_else(|| {
            format!("file:///<Phosphor install folder>/resources/{BUNDLE_RELATIVE}")
        });
        Box::leak(format!("\"plugin\": [\"{spec}\"]").into_boxed_str())
    });
    *TEXT
}

static INSTALL_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-opencode-install-title"),
    subtitle: crate::t_static!("cli-agent-plugin-opencode-install-subtitle"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-opencode-open-config-step"),
            command: "~/.config/opencode/opencode.json",
            executable: false,
            link: None,
        },
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-opencode-add-plugin-step"),
            command: plugin_entry_text(),
            executable: false,
            link: None,
        },
    ],
    post_install_notes: vec![crate::t_static!("cli-agent-plugin-opencode-restart-note")],
});

static UPDATE_INSTRUCTIONS: LazyLock<PluginInstructions> = LazyLock::new(|| PluginInstructions {
    title: crate::t_static!("cli-agent-plugin-opencode-update-title"),
    subtitle: crate::t_static!("cli-agent-plugin-opencode-update-subtitle"),
    steps: vec![
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-opencode-open-config-step"),
            command: "~/.config/opencode/opencode.json",
            executable: false,
            link: None,
        },
        PluginInstructionStep {
            description: crate::t_static!("cli-agent-plugin-opencode-replace-plugin-step"),
            command: plugin_entry_text(),
            executable: false,
            link: None,
        },
    ],
    post_install_notes: vec![crate::t_static!(
        "cli-agent-plugin-opencode-restart-update-note"
    )],
});

/// Moves the remote and stale entries in each of `files` to the bundled folder; `Err` carries
/// the message for the toast (the details are in `log`).
fn rewrite_config_files(files: &[PathBuf], bundle: &Path, log: &mut String) -> Result<(), String> {
    let Some(spec) = plugin_spec_for(bundle) else {
        log.push_str("The bundled plugin folder has no file:// form\n");
        return Err("The bundled OpenCode plugin path cannot be used".to_owned());
    };
    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Some(updated) = migrate_plugin_entries(&text, bundle) else {
            continue;
        };
        if let Err(err) = write_config_atomically(path, &updated) {
            log.push_str(&format!(
                "Could not write {} ({err}); it was left as it was\n",
                path.display()
            ));
            return Err(format!("Failed to write {}", path.display()));
        }
        log.push_str(&format!(
            "Pointed the plugin entry in {} at {spec}\n",
            path.display()
        ));
    }
    Ok(())
}

/// Replaces the file at `path` with `contents` atomically: a temporary file
/// in the directory of the *resolved* target is written, synced and renamed
/// over it, so a symlinked (dotfile-managed) config keeps its link and gets
/// its target replaced, and the file keeps its permissions. If any step fails
/// the temporary file is removed and the original is untouched.
fn write_config_atomically(path: &Path, contents: &str) -> io::Result<()> {
    let target = fs::canonicalize(path)?;
    let directory = target.parent().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "config file has no parent directory",
        )
    })?;
    let mut temp = tempfile::NamedTempFile::new_in(directory)?;
    temp.write_all(contents.as_bytes())?;
    if let Ok(metadata) = fs::metadata(&target) {
        temp.as_file().set_permissions(metadata.permissions())?;
    }
    temp.as_file().sync_all()?;
    temp.persist(&target).map_err(|err| err.error)?;
    Ok(())
}

/// What a `plugin` entry names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PluginEntryKind {
    /// A `file://` entry for one of Phosphor's bundle folders (any install location).
    Phosphor,
    /// A remote source: Warp's package (the npm name -- bare, `@<version>`, `npm:`-aliased --
    /// or a git spec for `warpdotdev/opencode-warp`) or Phosphor's earlier GitLab git spec.
    /// Phosphor never fetches either, so both are moved to the bundled folder.
    Warp,
}

/// Classifies one `plugin` entry. Anything else -- another plugin, a `file://` or filesystem
/// path that is not a Phosphor bundle folder, a similarly named repository -- is `None` and
/// is never rewritten.
fn classify_plugin_entry(spec: &str) -> Option<PluginEntryKind> {
    if bundle_folder_of_entry(spec).is_some() {
        return Some(PluginEntryKind::Phosphor);
    }
    let spec = spec.trim().to_ascii_lowercase();
    let npm = spec.strip_prefix("npm:").unwrap_or(&spec);
    if let Some(rest) = npm.strip_prefix(WARP_NPM_PACKAGE)
        && (rest.is_empty() || rest.starts_with('@'))
    {
        return Some(PluginEntryKind::Warp);
    }
    if is_old_phosphor_git_spec(&spec) {
        return Some(PluginEntryKind::Warp);
    }
    let git = spec.strip_prefix("git+").unwrap_or(&spec);
    if is_warp_plugin_source(git, REPO_NAME) {
        return Some(PluginEntryKind::Warp);
    }
    None
}

/// The folder a `file://` entry names, when it is one of Phosphor's bundle folders.
fn bundle_folder_of_entry(spec: &str) -> Option<String> {
    let path = url::Url::parse(spec.trim()).ok()?.to_file_path().ok()?;
    let path = path.to_string_lossy().into_owned();
    looks_like_bundle_path(&path, BUNDLE_RELATIVE).then_some(path)
}

/// `git+https://gitlab.com/winters-phosphor/opencode-warp(.git)(#ref)` and the other
/// spellings `npm-package-arg` accepts for the same repository: what older builds wrote.
fn is_old_phosphor_git_spec(spec: &str) -> bool {
    let git = spec.strip_prefix("git+").unwrap_or(spec);
    let path = [
        "https://gitlab.com/",
        "http://gitlab.com/",
        "ssh://git@gitlab.com/",
    ]
    .iter()
    .find_map(|prefix| git.strip_prefix(prefix))
    .or_else(|| git.strip_prefix("git@gitlab.com:"))
    .or_else(|| git.strip_prefix("gitlab:"));
    let Some(rest) = path.and_then(|path| path.strip_prefix(OLD_PHOSPHOR_REPO_PATH)) else {
        return false;
    };
    let rest = rest.strip_prefix(".git").unwrap_or(rest);
    rest.is_empty() || rest.starts_with(['#', '/'])
}

/// Whether `spec` is an entry that must be rewritten to point at `bundle`: a remote source,
/// or a bundle-folder entry for another install location.
fn entry_needs_rewrite(spec: &str, bundle: &Path) -> bool {
    match classify_plugin_entry(spec) {
        Some(PluginEntryKind::Warp) => true,
        Some(PluginEntryKind::Phosphor) => {
            bundle_folder_of_entry(spec).is_some_and(|path| !same_folder(&path, bundle))
        }
        None => false,
    }
}

/// The kinds of plugin entry the config file at `path` holds (empty when it
/// is missing, unreadable or holds none of ours).
fn plugin_entry_kinds_in_file(path: &Path) -> Vec<PluginEntryKind> {
    let Ok(text) = fs::read_to_string(path) else {
        return Vec::new();
    };
    plugin_array_elements(&text)
        .unwrap_or_default()
        .iter()
        .filter_map(|element| element.spec.as_ref())
        .filter_map(|(_, spec)| classify_plugin_entry(spec))
        .collect()
}

fn file_needs_source_migration(path: &Path, bundle: &Path) -> bool {
    let Ok(text) = fs::read_to_string(path) else {
        return false;
    };
    plugin_array_elements(&text)
        .unwrap_or_default()
        .iter()
        .filter_map(|element| element.spec.as_ref())
        .any(|(_, spec)| entry_needs_rewrite(spec, bundle))
}

/// Rewrites the remote-sourced and stale entries of the top-level `plugin` array in `text`
/// to the `file://` entry of `bundle` ([`entry_needs_rewrite`] says which), returning `None`
/// when there is nothing to change or the text cannot be read as JSON(C).
///
/// Exactly one entry is kept for the plugin: the first `[spec, options]` tuple among the
/// Warp and Phosphor entries (so the user's options survive), else an existing up-to-date
/// Phosphor entry, else the first entry needing a rewrite. A kept entry needing a rewrite
/// has its spec string replaced. Every other plain string Warp or Phosphor entry is removed
/// so the plugin is not loaded twice; another *tuple* is rewritten rather than removed, so
/// no options are ever dropped. A removal deletes only the entry, its comma and the
/// indentation of its own line -- never a comment or any other text between entries. Every
/// other byte of the file is kept.
fn migrate_plugin_entries(text: &str, bundle: &Path) -> Option<String> {
    let elements = plugin_array_elements(text)?;
    let kinds: Vec<Option<PluginEntryKind>> = elements
        .iter()
        .map(|element| {
            element
                .spec
                .as_ref()
                .and_then(|(_, spec)| classify_plugin_entry(spec))
        })
        .collect();
    let stale: Vec<bool> = elements
        .iter()
        .map(|element| {
            element
                .spec
                .as_ref()
                .is_some_and(|(_, spec)| entry_needs_rewrite(spec, bundle))
        })
        .collect();
    if !stale.contains(&true) {
        return None;
    }

    let ours = |index: &usize| kinds[*index].is_some();
    let keeper = (0..elements.len())
        .filter(ours)
        .find(|index| elements[*index].tuple)
        .or_else(|| {
            (0..elements.len())
                .find(|index| kinds[*index] == Some(PluginEntryKind::Phosphor) && !stale[*index])
        })
        .or_else(|| (0..elements.len()).find(|index| stale[*index]))?;

    let quoted_spec = serde_json::to_string(&plugin_spec_for(bundle)?).ok()?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut removed = vec![false; elements.len()];
    for index in (0..elements.len()).filter(ours) {
        let element = &elements[index];
        let rewrite = stale[index] && (index == keeper || element.tuple);
        if rewrite {
            let (span, _) = element.spec.as_ref()?;
            edits.push((span.clone(), quoted_spec.clone()));
        } else if index != keeper && !element.tuple {
            removed[index] = true;
        }
    }
    for index in (0..elements.len()).filter(|index| removed[*index]) {
        edits.extend(removal_edits(text, &elements, &removed, index));
    }

    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut updated = text.to_owned();
    for (range, replacement) in edits {
        updated.replace_range(range, &replacement);
    }
    Some(updated)
}

/// The deletions that remove element `index` and one separating comma.
///
/// An element alone on its line (only indentation before it) goes with that
/// indentation and the line break before it, so no blank line is left. With
/// a comma after it, that comma goes too (plus the spaces after it when the
/// entry shares its line with others). The last element has no comma after
/// it, so the comma after the nearest preceding *kept* element goes instead.
/// Only whitespace is ever deleted besides the entry and the comma.
fn removal_edits(
    text: &str,
    elements: &[PluginArrayElement],
    removed: &[bool],
    index: usize,
) -> Vec<(Range<usize>, String)> {
    let element = &elements[index];
    let (start, own_line) = line_start_before(text, element.span.start);
    let mut edits = Vec::new();
    if let Some(comma) = &element.comma_after {
        edits.push((start..element.span.end, String::new()));
        let comma_end = if own_line {
            comma.end
        } else {
            comma.end + leading_blanks(&text[comma.end..])
        };
        edits.push((comma.start..comma_end, String::new()));
        return edits;
    }
    let previous_comma = (0..index)
        .rev()
        .find(|previous| !removed[*previous])
        .and_then(|previous| elements[previous].comma_after.clone());
    match previous_comma {
        Some(comma) if text[comma.end..element.span.start].trim().is_empty() => {
            edits.push((comma.start..element.span.end, String::new()));
        }
        Some(comma) => {
            edits.push((comma, String::new()));
            edits.push((start..element.span.end, String::new()));
        }
        None => edits.push((start..element.span.end, String::new())),
    }
    edits
}

/// Where a removal of the element starting at `start` should begin: the line
/// break before it when only spaces or tabs precede it on its line (so the
/// whole line goes), else `start` itself. The flag says which.
fn line_start_before(text: &str, start: usize) -> (usize, bool) {
    let line_begin = text[..start].rfind('\n').map_or(0, |newline| newline + 1);
    let indentation_only = text[line_begin..start]
        .bytes()
        .all(|byte| byte == b' ' || byte == b'\t');
    if line_begin == 0 || !indentation_only {
        return (start, false);
    }
    let newline = line_begin - 1;
    // Keep CRLF files CRLF: take the `\r` with its `\n`.
    if newline > 0 && text.as_bytes()[newline - 1] == b'\r' {
        (newline - 1, true)
    } else {
        (newline, true)
    }
}

/// The number of spaces and tabs at the start of `text`.
fn leading_blanks(text: &str) -> usize {
    text.bytes()
        .take_while(|byte| *byte == b' ' || *byte == b'\t')
        .count()
}

/// One element of the top-level `plugin` array.
struct PluginArrayElement {
    /// The whole element, from its first to its last byte.
    span: Range<usize>,
    /// The spec string literal (quotes included) and its decoded value: the
    /// element itself, or the first item of a `[spec, options]` tuple.
    spec: Option<(Range<usize>, String)>,
    /// Whether the element is a `[spec, options]` tuple.
    tuple: bool,
    /// The comma token that follows the element, if any.
    comma_after: Option<Range<usize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    Open(u8),
    Close,
    Colon,
    Comma,
    String,
    Other,
}

struct Token {
    kind: TokenKind,
    span: Range<usize>,
}

/// Splits JSON with comments (what OpenCode reads, for `.json` and `.jsonc`
/// alike) into tokens, skipping whitespace and `//` / `/* */` comments.
/// Returns `None` for an unterminated string or comment.
fn tokenize(text: &str) -> Option<Vec<Token>> {
    let bytes = text.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let byte = bytes[at];
        let start = at;
        let kind = match byte {
            b' ' | b'\t' | b'\r' | b'\n' => {
                at += 1;
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'/') => {
                while at < bytes.len() && bytes[at] != b'\n' {
                    at += 1;
                }
                continue;
            }
            b'/' if bytes.get(at + 1) == Some(&b'*') => {
                let close = text[at + 2..].find("*/")?;
                at += 2 + close + 2;
                continue;
            }
            b'"' => {
                at += 1;
                loop {
                    match bytes.get(at)? {
                        b'\\' => at += 2,
                        b'"' => break,
                        _ => at += 1,
                    }
                }
                at += 1;
                TokenKind::String
            }
            b'{' | b'[' => {
                at += 1;
                TokenKind::Open(byte)
            }
            b'}' | b']' => {
                at += 1;
                TokenKind::Close
            }
            b':' => {
                at += 1;
                TokenKind::Colon
            }
            b',' => {
                at += 1;
                TokenKind::Comma
            }
            _ => {
                at += 1;
                while at < bytes.len() && !is_delimiter(bytes[at]) {
                    at += 1;
                }
                TokenKind::Other
            }
        };
        tokens.push(Token {
            kind,
            span: start..at,
        });
    }
    Some(tokens)
}

fn is_delimiter(byte: u8) -> bool {
    matches!(
        byte,
        b' ' | b'\t' | b'\r' | b'\n' | b'{' | b'}' | b'[' | b']' | b':' | b',' | b'"' | b'/'
    )
}

/// Index of the token closing the container opened at `open`.
fn matching_close(tokens: &[Token], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate().skip(open) {
        match token.kind {
            TokenKind::Open(_) => depth += 1,
            TokenKind::Close => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return Some(index);
                }
            }
            _ => {}
        }
    }
    None
}

fn decode_string(text: &str, token: &Token) -> Option<String> {
    serde_json::from_str(&text[token.span.clone()]).ok()
}

/// The elements of the root object's `plugin` array (the last one, if the key
/// repeats, as a JSON parser keeps the last). `None` when the text is not an
/// object, has no `plugin` array, or does not tokenize.
fn plugin_array_elements(text: &str) -> Option<Vec<PluginArrayElement>> {
    let tokens = tokenize(text)?;
    if tokens.first()?.kind != TokenKind::Open(b'{') {
        return None;
    }

    let mut array_open = None;
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.kind {
            TokenKind::Open(_) => depth += 1,
            TokenKind::Close => depth = depth.saturating_sub(1),
            TokenKind::String
                if depth == 1
                    && tokens.get(index + 1).map(|next| next.kind) == Some(TokenKind::Colon)
                    && tokens.get(index + 2).map(|next| next.kind)
                        == Some(TokenKind::Open(b'['))
                    && decode_string(text, token).as_deref() == Some("plugin") =>
            {
                array_open = Some(index + 2);
            }
            _ => {}
        }
    }
    let open = array_open?;
    let close = matching_close(&tokens, open)?;

    let mut elements: Vec<PluginArrayElement> = Vec::new();
    let mut index = open + 1;
    while index < close {
        let token = &tokens[index];
        match token.kind {
            TokenKind::Comma => {
                if let Some(last) = elements.last_mut()
                    && last.comma_after.is_none()
                {
                    last.comma_after = Some(token.span.clone());
                }
                index += 1;
            }
            TokenKind::String => {
                let spec = decode_string(text, token).map(|spec| (token.span.clone(), spec));
                elements.push(PluginArrayElement {
                    span: token.span.clone(),
                    spec,
                    tuple: false,
                    comma_after: None,
                });
                index += 1;
            }
            TokenKind::Open(kind) => {
                let end = matching_close(&tokens, index)?;
                let first = tokens
                    .get(index + 1)
                    .filter(|first| kind == b'[' && first.kind == TokenKind::String);
                let spec = first.and_then(|first| {
                    decode_string(text, first).map(|spec| (first.span.clone(), spec))
                });
                elements.push(PluginArrayElement {
                    span: token.span.start..tokens[end].span.end,
                    spec,
                    tuple: kind == b'[',
                    comma_after: None,
                });
                index = end + 1;
            }
            TokenKind::Other | TokenKind::Colon | TokenKind::Close => {
                elements.push(PluginArrayElement {
                    span: token.span.clone(),
                    spec: None,
                    tuple: false,
                    comma_after: None,
                });
                index += 1;
            }
        }
    }
    Some(elements)
}

/// OpenCode's global config directory: `OPENCODE_CONFIG_DIR`, else
/// `$XDG_CONFIG_HOME/opencode`, else `~/.config/opencode`, on every platform
/// (`packages/util/src/global.ts`, `global-roots.ts`).
fn opencode_config_dir() -> Option<PathBuf> {
    let non_empty = |name: &str| env::var(name).ok().filter(|value| !value.is_empty());
    if let Some(dir) = non_empty("OPENCODE_CONFIG_DIR") {
        return Some(PathBuf::from(dir));
    }
    let config_home = non_empty("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".config")))?;
    Some(config_home.join("opencode"))
}

fn global_config_files() -> Vec<PathBuf> {
    opencode_config_dir()
        .map(|dir| {
            CONFIG_FILE_NAMES
                .iter()
                .map(|name| dir.join(name))
                .collect()
        })
        .unwrap_or_default()
}

/// Forgets a move attempt, for tests that drive several in one process.
#[cfg(test)]
pub(super) fn reset_source_move_attempt() {
    SOURCE_MOVE_ATTEMPTED.store(false, Ordering::Relaxed);
}

#[cfg(test)]
#[path = "opencode_tests.rs"]
mod tests;
