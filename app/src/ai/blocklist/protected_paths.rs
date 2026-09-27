//! The files and directories an agent must never write without the user's explicit, per-action
//! confirmation — whatever the autonomy setting, run-to-completion, or the LRC tag-in override
//! says.
//!
//! # Why this list exists, and why it is not "MCP configs"
//!
//! This guard used to protect MCP config files only (`mcp_provider_from_file_path`). But an
//! agent that can silently write *any* file which later feeds instructions, permissions or
//! code execution back into an agent, a shell, or git can escalate itself: flip its own
//! execution profile to `AlwaysAllow` via `settings.toml` or the profile store, add a hook to
//! `.claude/settings.json`, drop a key into `~/.ssh/authorized_keys`, append to `~/.bashrc`,
//! install a `.git/hooks/pre-commit`, or plant a skill or prompt template the next turn loads
//! as instructions. Every entry below is on the list for that reason; each carries its
//! justification next to it (#682).
//!
//! **This is the one place the protected set is defined.** `permissions.rs` consults it
//! through [`is_protected_write_path`]; nothing else should grow a second list.
//!
//! # How matching works
//!
//! The path is first normalised lexically — `~` expanded, `.` and `..` folded — never with
//! `fs::canonicalize`, which blocks and fails on a file that does not exist yet (the normal
//! case for a write). Then:
//!
//! - **Rules are matched anywhere in the path, not only under `$HOME`.** The guard is given
//!   both the model's raw spelling and the session-resolved absolute path (see
//!   `file_edit_guard_paths`), but the raw spelling may be relative with no cwd to resolve it
//!   against, and a project-level `.claude/settings.json` or `.git/hooks/` is exactly as
//!   dangerous as the home-level one. This over-matches by design — a `.codex/` directory
//!   inside a project also needs confirmation — and the failure mode is a confirmation
//!   prompt, not a silent write.
//! - **The app's own directories are matched by absolute prefix** ([`app_owned_dirs`]):
//!   they are platform- and channel-specific and have no stable relative spelling.
//! - **Case-insensitively on macOS and Windows**, whose default filesystems are
//!   case-insensitive: `~/.SSH/Authorized_Keys` is the same file there.
//! - **On Windows, trailing dots and spaces are stripped** from each component (Win32 drops
//!   them: `.mcp.json.` opens `.mcp.json`), and **any `:` in a path component is refused
//!   outright** — an alternate data stream such as `.mcp.json::$DATA` or `x:stream` writes
//!   the named file, and no legitimate agent edit needs a stream.
//!
//! # Residue, deliberately not closed here
//!
//! - **Symlinks.** Matching is lexical, so a symlink pointing at a protected file still
//!   evades. Following it needs blocking I/O inside a permission check, and the agent must
//!   create the link first — which is a shell command, gated separately.
//! - **`$HOME/...` and other variable spellings.** Only `~` is expanded.
//! - **Windows 8.3 short names** (`PROGRA~1`) are not expanded.
//! - **Content-level escalation through ordinary files** (a `Makefile`, `package.json`
//!   scripts, a project's own CI config) is out of scope: those are the agent's normal work
//!   product and the user reviews them as such.

use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use ai::skills::SKILL_PROVIDER_DEFINITIONS;

/// How a rule's components must appear in a normalised path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RuleKind {
    /// The path ENDS WITH these components: one specific file, wherever it lives.
    File,
    /// These components appear consecutively and at least one more follows: anything inside
    /// that directory, at any depth, wherever the directory lives.
    Tree,
}

struct ProtectedRule {
    kind: RuleKind,
    /// Relative components as written; [`MatchFlavor::key`] canonicalises both sides.
    components: Vec<String>,
    /// Why the entry is here. Not read at runtime; it is the audit trail for the list.
    #[allow(dead_code)]
    why: &'static str,
}

fn file(components: &[&str], why: &'static str) -> ProtectedRule {
    ProtectedRule {
        kind: RuleKind::File,
        components: components.iter().map(|c| (*c).to_owned()).collect(),
        why,
    }
}

fn tree(components: &[&str], why: &'static str) -> ProtectedRule {
    ProtectedRule {
        kind: RuleKind::Tree,
        components: components.iter().map(|c| (*c).to_owned()).collect(),
        why,
    }
}

/// The protected set. Keep every entry justified.
static PROTECTED_WRITE_RULES: LazyLock<Vec<ProtectedRule>> = LazyLock::new(|| {
    const MCP: &str =
        "MCP server config: adding a server grants the agent new tools and injected context";
    const AGENT_CONFIG: &str =
        "agent configuration directory: holds MCP servers, permissions, hooks or instructions";
    const HOOKS: &str =
        "agent settings: `hooks` run arbitrary shell commands, `permissions` widen autonomy";
    const SHELL_RC: &str = "shell startup file: runs on every new shell";
    const SSH: &str = "ssh: authorized_keys grants login, config can run ProxyCommand/LocalCommand";
    const GIT: &str =
        "git: hooks and config (core.hooksPath, core.fsmonitor) execute code on git commands";

    let mut rules = vec![
        // --- MCP configs (the original protected set) --------------------------------------
        file(&[".mcp.json"], MCP),
        file(&[".claude.json"], MCP),
        // `.codex/config.toml`, `.agents/.mcp.json` and `.warp/.mcp.json` are covered by the
        // directory rules below.

        // --- Other agents' configuration ---------------------------------------------------
        tree(&[".codex"], AGENT_CONFIG),
        tree(&[".agents"], AGENT_CONFIG),
        file(&[".claude", "settings.json"], HOOKS),
        file(&[".claude", "settings.local.json"], HOOKS),
        // --- SSH ---------------------------------------------------------------------------
        tree(&[".ssh"], SSH),
        // --- Shell startup files -----------------------------------------------------------
        file(&[".profile"], SHELL_RC),
        file(&[".bashrc"], SHELL_RC),
        file(&[".bash_profile"], SHELL_RC),
        file(&[".bash_login"], SHELL_RC),
        file(&[".bash_logout"], SHELL_RC),
        file(&[".zshrc"], SHELL_RC),
        file(&[".zshenv"], SHELL_RC),
        file(&[".zprofile"], SHELL_RC),
        file(&[".zlogin"], SHELL_RC),
        file(&[".zlogout"], SHELL_RC),
        file(&[".kshrc"], SHELL_RC),
        file(&[".mkshrc"], SHELL_RC),
        file(&[".cshrc"], SHELL_RC),
        file(&[".tcshrc"], SHELL_RC),
        file(&[".login"], SHELL_RC),
        file(&[".xonshrc"], SHELL_RC),
        tree(&[".config", "fish"], SHELL_RC),
        tree(&[".config", "nushell"], SHELL_RC),
        tree(&[".config", "xonsh"], SHELL_RC),
        tree(&[".config", "powershell"], SHELL_RC),
        tree(&["Documents", "PowerShell"], SHELL_RC),
        tree(&["Documents", "WindowsPowerShell"], SHELL_RC),
        file(&["Microsoft.PowerShell_profile.ps1"], SHELL_RC),
        // --- Things that run code on the user's behalf -------------------------------------
        tree(&[".git", "hooks"], GIT),
        file(&[".git", "config"], GIT),
        file(
            &[".envrc"],
            "direnv: executed by the shell on `cd` once allowed",
        ),
        file(
            &[".vscode", "tasks.json"],
            "VS Code tasks: `runOn: folderOpen` executes on opening the folder",
        ),
    ];

    // --- Skills: loaded as agent instructions, for every provider this app reads -----------
    // Taken from the skill loader's own table so a provider added there is protected here.
    for definition in SKILL_PROVIDER_DEFINITIONS.iter() {
        let components: Vec<String> = definition
            .skills_path
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        rules.push(ProtectedRule {
            kind: RuleKind::Tree,
            components,
            why: "skills directory: skills are loaded as agent instructions",
        });
    }

    rules
});

/// This app's own home-level config directory names: `.warp` and `.phosphor`, and their
/// channel / data-profile variants (`.warp-dev`, `.warp-local`, `.phosphor-<profile>`, ...;
/// see `warp_core::paths::warp_home_config_dir_name`). They hold `settings.toml` on macOS,
/// the Phosphor MCP config, skills (`skills/`) and prompt templates (`prompts/`), so the
/// whole tree is protected — matched as a directory component anywhere, like the rules
/// above, which also covers a project's `.warp/` (skills, `.mcp.json`, rules).
fn is_app_config_dir_component(component: &str) -> bool {
    ["warp", "phosphor"].iter().any(|name| {
        component
            .strip_prefix('.')
            .and_then(|rest| rest.strip_prefix(name))
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('-'))
    })
}

/// Absolute directories this app owns, resolved for the current platform, channel and data
/// profile. Writing any of them can change the agent's own permissions:
///
/// - `config_local_dir` holds `settings.toml` and `user_preferences.json` — autonomy settings;
/// - `state_dir` / `secure_state_dir` hold `warp.sqlite`, where execution profiles live;
/// - `data_dir` holds workflows and other user data the app loads;
/// - the active prompt-template override directory (`ZAP_PROMPT_DIR` or the settings panel)
///   is rendered into the system prompt.
fn app_owned_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        warp_core::paths::config_local_dir(),
        warp_core::paths::data_dir(),
        warp_core::paths::state_dir(),
    ];
    dirs.extend(warp_core::paths::secure_state_dir());
    dirs.extend(warp_core::paths::warp_home_config_dir());
    dirs.extend(crate::ai::agent_providers::prompt_renderer::active_prompt_dir());
    // `unwrap_or_default` in `warp_core::paths` yields an EMPTY path when the home directory
    // is unknown, and every path starts with the empty path. An entry that is not absolute
    // cannot be matched by prefix meaningfully, so drop it rather than protect everything.
    dirs.retain(|dir| dir.is_absolute());
    dirs
}

/// How path components compare on a platform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MatchFlavor {
    /// The default filesystem is case-insensitive (macOS APFS/HFS+, Windows NTFS).
    pub case_insensitive: bool,
    /// Win32 path rules: trailing dots/spaces dropped, `:` introduces a data stream.
    pub windows: bool,
}

impl MatchFlavor {
    pub const HOST: Self = Self {
        case_insensitive: cfg!(any(target_os = "macos", target_os = "windows")),
        windows: cfg!(target_os = "windows"),
    };

    /// The canonical form of one path component for comparison.
    fn key(self, component: &str) -> String {
        let mut key = component;
        if self.windows {
            let trimmed = key.trim_end_matches(['.', ' ']);
            // `.`/`..` are folded before this point; an all-dots-and-spaces name is left as
            // is rather than collapsing to the empty string.
            if !trimmed.is_empty() {
                key = trimmed;
            }
        }
        if self.case_insensitive {
            key.to_lowercase()
        } else {
            key.to_owned()
        }
    }
}

/// Whether `path` is protected from agent writes without explicit confirmation.
pub(super) fn is_protected_write_path(path: &Path) -> bool {
    is_protected_write_path_with(path, MatchFlavor::HOST, &app_owned_dirs())
}

/// [`is_protected_write_path`] with the platform flavor and app directories supplied, so both
/// can be exercised from any host.
pub(super) fn is_protected_write_path_with(
    path: &Path,
    flavor: MatchFlavor,
    app_dirs: &[PathBuf],
) -> bool {
    let normalized = normalize_for_protected_path_check(path);

    // Named components only, in comparison form. Root and prefix are handled by the
    // absolute-prefix check below.
    let mut names = Vec::new();
    for component in normalized.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy();
                // An alternate data stream (`file::$DATA`, `file:stream`) writes the named
                // file. Refuse any stream syntax rather than try to parse it.
                if flavor.windows && name.contains(':') {
                    return true;
                }
                names.push(flavor.key(&name));
            }
            Component::ParentDir => names.push("..".to_owned()),
            Component::RootDir | Component::Prefix(_) | Component::CurDir => {}
        }
    }

    let rule_matches = |rule: &ProtectedRule| {
        let wanted: Vec<String> = rule.components.iter().map(|c| flavor.key(c)).collect();
        match rule.kind {
            RuleKind::File => names.ends_with(&wanted),
            RuleKind::Tree => names
                .windows(wanted.len())
                .enumerate()
                .any(|(start, window)| {
                    window == wanted.as_slice() && start + wanted.len() < names.len()
                }),
        }
    };
    if PROTECTED_WRITE_RULES.iter().any(rule_matches) {
        return true;
    }

    // A directory component (never the final file name) naming this app's config dir.
    if let Some((_, dirs)) = names.split_last() {
        if dirs.iter().any(|name| is_app_config_dir_component(name)) {
            return true;
        }
    }

    let path_keys = full_keys(&normalized, flavor);
    app_dirs.iter().any(|dir| {
        let dir_keys = full_keys(&normalize_for_protected_path_check(dir), flavor);
        !dir_keys.is_empty() && path_keys.starts_with(&dir_keys)
    })
}

/// Every component of `path` — root and prefix included — in comparison form, for
/// absolute-prefix matching.
fn full_keys(path: &Path, flavor: MatchFlavor) -> Vec<String> {
    path.components()
        .map(|component| match component {
            Component::RootDir => "/".to_owned(),
            other => flavor.key(&other.as_os_str().to_string_lossy()),
        })
        .collect()
}

/// Expands a leading `~` and folds away `.` and `..` components, without touching the
/// filesystem.
///
/// Deliberately lexical: this runs inside a permission check, and `fs::canonicalize` both
/// blocks and fails outright on a path that does not exist yet — which is the normal case for
/// a file the agent is about to create.
pub(super) fn normalize_for_protected_path_check(path: &Path) -> PathBuf {
    let expanded = match path.to_str() {
        Some(path) => PathBuf::from(shellexpand::tilde(path).into_owned()),
        // Not valid UTF-8, so there is no `~` to expand; normalise it as-is.
        None => path.to_path_buf(),
    };

    let mut normalized = PathBuf::new();
    for component in expanded.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => match normalized.components().next_back() {
                // Ascend out of a named directory.
                Some(Component::Normal(_)) => {
                    normalized.pop();
                }
                // `..` cannot ascend past the root; POSIX defines `/..` as `/`.
                Some(Component::RootDir | Component::Prefix(_)) => {}
                // Nothing to ascend out of yet (`../x`, or `../..`): keep it, so the path
                // does not silently become a *different*, shorter one.
                _ => normalized.push(".."),
            },
            component => normalized.push(component.as_os_str()),
        }
    }

    if normalized.as_os_str().is_empty() {
        expanded
    } else {
        normalized
    }
}

#[cfg(test)]
#[path = "protected_paths_tests.rs"]
mod tests;
