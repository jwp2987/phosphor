use std::env;
use std::fs;
use std::ops::Range;
use std::path::{Path, PathBuf};
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
    /// user does from the instructions; Phosphor does not add it unasked.
    fn can_auto_install(&self) -> bool {
        false
    }

    /// The update chip rewrites a Warp-sourced entry in place, which the user
    /// asks for by clicking it. Background launch paths key off
    /// `can_auto_install` and stay declined.
    fn can_auto_update(&self) -> bool {
        true
    }

    fn is_installed(&self) -> bool {
        global_config_files()
            .iter()
            .any(|path| !plugin_entry_kinds_in_file(path).is_empty())
    }

    fn needs_update(&self) -> bool {
        // No on-disk version to compare: OpenCode's install cache is keyed by a
        // hash of the spec. The reported version covers the version case.
        self.needs_source_migration()
    }

    fn needs_source_migration(&self) -> bool {
        let files = global_config_files();
        SOURCE_MIGRATION_CHECK.get_or_compute(&files, || {
            files.iter().any(|path| file_needs_source_migration(path))
        })
    }

    /// Rewrites every Warp-sourced entry in the global OpenCode config to
    /// [`PLUGIN_SPEC`], in place: only that string literal changes, so the
    /// user's other plugins, options, comments and formatting stay as they
    /// were. A local `file://` or path entry is never touched.
    ///
    /// Only once Phosphor's repository answers the pre-flight: OpenCode
    /// fetches the new entry on its next start, and with gitlab.com blocked
    /// the plugin would stop loading where Warp's cached package still works.
    async fn update(&self) -> Result<(), PluginInstallError> {
        let mut log = String::new();
        if !plugin_source_reachable(&*self.runner, PREFLIGHT_URL, &mut log).await {
            return Err(source_unreachable_error(log));
        }
        let mut changed = false;
        for path in global_config_files() {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Some(updated) = migrate_plugin_entries(&text) else {
                continue;
            };
            // Written in place rather than renamed over, so a symlinked
            // (dotfile-managed) config stays a symlink.
            fs::write(&path, updated).map_err(|err| PluginInstallError {
                message: format!("Failed to write {}", path.display()),
                log: format!("{log}write {}: {err}\n", path.display()),
            })?;
            log.push_str(&format!(
                "Pointed the plugin entry in {} at {PLUGIN_SPEC}\n",
                path.display()
            ));
            changed = true;
        }
        if !changed {
            log.push_str(
                "No @warp-dot-dev/opencode-warp entry found in the global OpenCode config\n",
            );
            return Err(PluginInstallError {
                message: "No plugin entry to update in the global OpenCode config".to_owned(),
                log,
            });
        }
        Ok(())
    }

    fn update_success_message(&self) -> &'static str {
        "Notification plugin updated. Please restart OpenCode to activate."
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
/// The first Warp entry's string literal becomes [`PLUGIN_SPEC`] (inside a
/// `[spec, options]` tuple the options are kept); any further Warp entry, or
/// every Warp entry when Phosphor's spec is already listed, is removed so the
/// plugin is not loaded twice. Every other byte of the file is kept.
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

    let mut have_phosphor = kinds.contains(&Some(PluginEntryKind::Phosphor));
    let mut replace: Option<Range<usize>> = None;
    let mut removed = vec![false; elements.len()];
    for (index, kind) in kinds.iter().enumerate() {
        if *kind != Some(PluginEntryKind::Warp) {
            continue;
        }
        if have_phosphor {
            removed[index] = true;
        } else {
            replace = elements[index].spec.as_ref().map(|(span, _)| span.clone());
            have_phosphor = true;
        }
    }

    // One edit per maximal run of removed elements, taking the separator on
    // the side that still has a kept element, so no comma is left dangling.
    let mut edits: Vec<(Range<usize>, String)> = Vec::new();
    if let Some(span) = replace {
        edits.push((span, serde_json::to_string(PLUGIN_SPEC).ok()?));
    }
    let mut index = 0;
    while index < elements.len() {
        if !removed[index] {
            index += 1;
            continue;
        }
        let first = index;
        while index < elements.len() && removed[index] {
            index += 1;
        }
        let last = index - 1;
        let range = if let Some(next) = elements.get(index) {
            elements[first].span.start..next.span.start
        } else if first > 0 {
            elements[first - 1].span.end..elements[last].span.end
        } else {
            elements[first].span.start..elements[last].span.end
        };
        edits.push((range, String::new()));
    }

    edits.sort_by_key(|(range, _)| std::cmp::Reverse(range.start));
    let mut updated = text.to_owned();
    for (range, replacement) in edits {
        updated.replace_range(range, &replacement);
    }
    Some(updated)
}

/// One element of the top-level `plugin` array.
struct PluginArrayElement {
    /// The whole element, from its first to its last byte.
    span: Range<usize>,
    /// The spec string literal (quotes included) and its decoded value: the
    /// element itself, or the first item of a `[spec, options]` tuple.
    spec: Option<(Range<usize>, String)>,
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

    let mut elements = Vec::new();
    let mut index = open + 1;
    while index < close {
        let token = &tokens[index];
        match token.kind {
            TokenKind::Comma => {
                index += 1;
            }
            TokenKind::String => {
                let spec = decode_string(text, token).map(|spec| (token.span.clone(), spec));
                elements.push(PluginArrayElement {
                    span: token.span.clone(),
                    spec,
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
                });
                index = end + 1;
            }
            TokenKind::Other | TokenKind::Colon | TokenKind::Close => {
                elements.push(PluginArrayElement {
                    span: token.span.clone(),
                    spec: None,
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

#[cfg(test)]
#[path = "opencode_tests.rs"]
mod tests;
