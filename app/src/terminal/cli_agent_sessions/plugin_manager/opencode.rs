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
    PluginInstructionStep, PluginInstructions, ShellCommandRunner, is_warp_plugin_source,
    plugin_source_reachable, source_unreachable_error,
};
use crate::terminal::shell::ShellType;

/// The `plugin` entry for Phosphor's GitLab copy of the plugin (full-history
/// mirror of `warpdotdev/opencode-warp`, MIT notices kept), pinned to a release
/// tag.
///
/// OpenCode installs every `plugin` entry through `npm-package-arg` +
/// Arborist (`packages/util/src/npm.ts`). `npm-package-arg` classifies a
/// `git+https://` URL as `type: "git"` (`fromURL`), and gitlab.com is also a
/// `hosted-git-info` host, so either route gives a git install of the tag.
/// OpenCode keys the install by `git-<slug>-<sha256(spec)[..12]>` and treats a
/// non-commit ref as mutable, re-resolving the tag. Arborist runs with
/// `ignoreScripts: true`, so the repository has to carry its built `dist/`;
/// the tag does (built from source, identical to npm's 0.2.0 `dist/`).
///
/// Keep the instruction commands below in step with this (a test pins them),
/// and keep the tag in step with the plugin's `package.json` version.
pub(crate) const PLUGIN_SPEC: &str =
    "git+https://gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0";

/// Warp's npm package, which existing installs name in their `plugin` array.
const WARP_NPM_PACKAGE: &str = "@warp-dot-dev/opencode-warp";
/// The repository name, shared by Warp's and Phosphor's copies.
const REPO_NAME: &str = "opencode-warp";
/// What the pre-flight asks `git ls-remote`: the spec without `git+` and ref.
const PREFLIGHT_URL: &str = "https://gitlab.com/winters-phosphor/opencode-warp.git";
/// Phosphor's repository path on gitlab.com.
const PHOSPHOR_REPO_PATH: &str = "winters-phosphor/opencode-warp";
/// The global config files OpenCode reads, in its own order
/// (`packages/core/src/config/discovery.ts`).
const CONFIG_FILE_NAMES: [&str; 2] = ["opencode.json", "opencode.jsonc"];

// The lowest plugin version this build needs, compared against the version the
// running plugin reports over OSC 777. Phosphor's copy is 0.2.0, so it clears
// this; the move off Warp's npm package is detected from the config instead
// (`needs_source_migration`), so raising this is not what moves anyone.
const MINIMUM_PLUGIN_VERSION: &str = "0.1.5";

static SOURCE_MIGRATION_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();
/// `is_installed` is asked from the footer's render path too.
static INSTALLED_CHECK: ConfigFileCheckCache = ConfigFileCheckCache::new();

/// Set once this process has *completed* a move attempt (the pre-flight passed
/// and the rewrite ran, whatever its outcome); after it the move chip is not
/// offered again in this process. A failed pre-flight does not set it.
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
        let files = global_config_files();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            files.iter().any(|path| file_needs_source_migration(path))
        })
    }

    /// Rewrites every Warp-sourced entry in the global OpenCode config to
    /// [`PLUGIN_SPEC`] ([`migrate_plugin_entries`]): only that entry changes,
    /// so the user's other plugins, options, comments and formatting stay as
    /// they were. A local `file://` or path entry is never touched. Each file
    /// is replaced atomically ([`write_config_atomically`]); if anything fails
    /// the original is untouched.
    ///
    /// Only once Phosphor's repository answers the pre-flight: OpenCode
    /// fetches the new entry on its next start, and with gitlab.com blocked
    /// the plugin would stop loading where Warp's cached package still works.
    async fn move_to_phosphor_source(&self) -> Result<(), PluginInstallError> {
        let _operation = OPERATION_LOCK.lock().await;
        let mut log = String::new();
        let files = global_config_files();
        if !files.iter().any(|path| file_needs_source_migration(path)) {
            // Moved meanwhile (another move held the lock), or nothing to move.
            return Ok(());
        }
        if !plugin_source_reachable(&*self.runner, PREFLIGHT_URL, &mut log).await {
            return Err(source_unreachable_error(log));
        }
        let result = rewrite_config_files(&files, &mut log);
        SOURCE_MOVE_ATTEMPTED.store(true, Ordering::Relaxed);
        result.map_err(|message| PluginInstallError { message, log })
    }

    fn move_success_message(&self) -> &'static str {
        "Notification plugin moved to Phosphor's source. Please restart OpenCode to activate."
    }

    fn install_instructions(&self) -> &'static PluginInstructions {
        &INSTALL_INSTRUCTIONS
    }

    fn update_instructions(&self) -> &'static PluginInstructions {
        &UPDATE_INSTRUCTIONS
    }
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
            command: "\"plugin\": [\"git+https://gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0\"]",
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
            command: "\"plugin\": [\"git+https://gitlab.com/winters-phosphor/opencode-warp.git#v0.2.0\"]",
            executable: false,
            link: None,
        },
    ],
    post_install_notes: vec![crate::t_static!(
        "cli-agent-plugin-opencode-restart-update-note"
    )],
});

/// Moves the Warp entries in each of `files`; `Err` carries the message for
/// the toast (the details are in `log`).
fn rewrite_config_files(files: &[PathBuf], log: &mut String) -> Result<(), String> {
    for path in files {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        let Some(updated) = migrate_plugin_entries(&text) else {
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
            "Pointed the plugin entry in {} at {PLUGIN_SPEC}\n",
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
    /// Phosphor's GitLab copy, at any ref.
    Phosphor,
    /// Warp's package: the npm name (bare, `@<version>`, `npm:`-aliased) or a
    /// git spec for `warpdotdev/opencode-warp`.
    Warp,
}

/// Classifies one `plugin` entry. Anything else -- another plugin, a local
/// `file://` or filesystem path, a similarly named repository -- is `None` and
/// is never rewritten.
fn classify_plugin_entry(spec: &str) -> Option<PluginEntryKind> {
    let spec = spec.trim().to_ascii_lowercase();
    let npm = spec.strip_prefix("npm:").unwrap_or(&spec);
    if let Some(rest) = npm.strip_prefix(WARP_NPM_PACKAGE)
        && (rest.is_empty() || rest.starts_with('@'))
    {
        return Some(PluginEntryKind::Warp);
    }
    if is_phosphor_git_spec(&spec) {
        return Some(PluginEntryKind::Phosphor);
    }
    let git = spec.strip_prefix("git+").unwrap_or(&spec);
    if is_warp_plugin_source(git, REPO_NAME) {
        return Some(PluginEntryKind::Warp);
    }
    None
}

/// `git+https://gitlab.com/winters-phosphor/opencode-warp(.git)(#ref)` and the
/// other spellings `npm-package-arg` accepts for the same repository.
fn is_phosphor_git_spec(spec: &str) -> bool {
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
    let Some(rest) = path.and_then(|path| path.strip_prefix(PHOSPHOR_REPO_PATH)) else {
        return false;
    };
    let rest = rest.strip_prefix(".git").unwrap_or(rest);
    rest.is_empty() || rest.starts_with(['#', '/'])
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

fn file_needs_source_migration(path: &Path) -> bool {
    plugin_entry_kinds_in_file(path).contains(&PluginEntryKind::Warp)
}

/// Rewrites the Warp-sourced entries of the top-level `plugin` array in
/// `text`, returning `None` when there is nothing to change or the text cannot
/// be read as JSON(C).
///
/// Exactly one entry is kept for the plugin: the first `[spec, options]`
/// tuple among the Warp and Phosphor entries (so the user's options survive),
/// else an existing Phosphor entry, else the first Warp entry. A kept Warp
/// entry has its spec string replaced by [`PLUGIN_SPEC`]. Every other plain
/// string Warp or Phosphor entry is removed so the plugin is not loaded
/// twice; another Warp *tuple* is rewritten rather than removed, so no options
/// are ever dropped. A removal deletes only the entry, its comma and the
/// indentation of its own line -- never a comment or any other text between
/// entries. Every other byte of the file is kept.
fn migrate_plugin_entries(text: &str) -> Option<String> {
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
    if !kinds.contains(&Some(PluginEntryKind::Warp)) {
        return None;
    }

    let ours = |index: &usize| kinds[*index].is_some();
    let keeper = (0..elements.len())
        .filter(ours)
        .find(|index| elements[*index].tuple)
        .or_else(|| {
            (0..elements.len()).find(|index| kinds[*index] == Some(PluginEntryKind::Phosphor))
        })
        .or_else(|| {
            (0..elements.len()).find(|index| kinds[*index] == Some(PluginEntryKind::Warp))
        })?;

    let quoted_spec = serde_json::to_string(PLUGIN_SPEC).ok()?;
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    let mut removed = vec![false; elements.len()];
    for index in (0..elements.len()).filter(ours) {
        let element = &elements[index];
        let rewrite =
            kinds[index] == Some(PluginEntryKind::Warp) && (index == keeper || element.tuple);
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
