use std::path::{Path, PathBuf};

use super::*;

const POSIX: MatchFlavor = MatchFlavor {
    case_insensitive: false,
    windows: false,
};
const MACOS: MatchFlavor = MatchFlavor {
    case_insensitive: true,
    windows: false,
};
const WINDOWS: MatchFlavor = MatchFlavor {
    case_insensitive: true,
    windows: true,
};

fn protected(path: &str, flavor: MatchFlavor) -> bool {
    is_protected_write_path_with(Path::new(path), flavor, &[])
}

/// Every class the maintainer asked to protect (#682), spelled both home-level and, where it
/// applies, project-level. Each of these must need an explicit confirmation.
#[test]
fn escalation_targets_are_protected() {
    for path in [
        // MCP configs, the original set.
        "/home/u/.mcp.json",
        "/project/.mcp.json",
        "/home/u/.claude.json",
        "/home/u/.codex/config.toml",
        "/home/u/.agents/.mcp.json",
        "/home/u/.warp/.mcp.json",
        // This app's own config dirs, every channel/profile spelling.
        "/home/u/.warp/settings.toml",
        "/home/u/.warp-dev/settings.toml",
        "/home/u/.phosphor/skills/evil/SKILL.md",
        "/home/u/.phosphor/prompts/system.j2",
        "/home/u/.phosphor-work/prompts/system.j2",
        "/project/.warp/skills/x/SKILL.md",
        // Other agents' config and hooks.
        "/home/u/.claude/settings.json",
        "/project/.claude/settings.json",
        "/project/.claude/settings.local.json",
        "/home/u/.codex/AGENTS.md",
        "/home/u/.agents/anything",
        // Skills for every provider the loader reads.
        "/home/u/.claude/skills/x/SKILL.md",
        "/project/.cursor/skills/x/SKILL.md",
        "/project/.github/skills/x/SKILL.md",
        // SSH.
        "/home/u/.ssh/authorized_keys",
        "/home/u/.ssh/config",
        // Shell startup.
        "/home/u/.bashrc",
        "/home/u/.zshrc",
        "/home/u/.profile",
        "/home/u/.config/fish/config.fish",
        "/home/u/.config/fish/conf.d/evil.fish",
        "/home/u/Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
        // Code that runs on the user's behalf.
        "/project/.git/hooks/pre-commit",
        "/project/.git/config",
        "/project/.envrc",
        "/project/.vscode/tasks.json",
        // Relative and non-canonical spellings of the same.
        ".mcp.json",
        ".claude/settings.json",
        "sub/../.git/hooks/post-checkout",
        "./.envrc",
    ] {
        assert!(protected(path, POSIX), "{path:?} must be protected");
    }
}

/// Negative controls: the list must not swallow ordinary project work, or the guard becomes
/// "ask for everything" and users learn to click through it.
#[test]
fn ordinary_files_are_not_protected() {
    for path in [
        "/project/src/main.rs",
        "/project/README.md",
        "/project/.gitignore",
        "/project/.github/workflows/ci.yml",
        "/project/.vscode/settings.json",
        "/project/.claude/notes.md",
        "/project/docs/warp.md",
        "/project/.warpignore",
        "/project/config.toml",
        "/project/profile.rs",
        "/home/u/notes.md",
        // A directory rule protects what is INSIDE the directory; a file that merely shares
        // the name is not the directory.
        "/project/.git",
        "/project/.codex",
        // Case matters on a case-sensitive filesystem.
        "/home/u/.SSH/authorized_keys",
    ] {
        assert!(!protected(path, POSIX), "{path:?} must stay writable");
    }
}

/// macOS and Windows default to case-insensitive filesystems: `.SSH/Authorized_Keys` IS the
/// protected file there, and an exact-case match would let it through.
#[test]
fn case_insensitive_platforms_match_any_case() {
    for path in [
        "/home/u/.SSH/Authorized_Keys",
        "/project/.MCP.json",
        "/home/u/.Claude.JSON",
        "/project/.Git/Hooks/pre-commit",
        "/Users/u/.WARP/settings.toml",
        "/project/.VSCode/Tasks.json",
    ] {
        assert!(
            protected(path, MACOS),
            "{path:?} must be protected on macOS"
        );
        assert!(
            protected(path, WINDOWS),
            "{path:?} must be protected on Windows"
        );
    }
    assert!(!protected("/project/src/Main.rs", MACOS));
}

/// Win32 drops trailing dots and spaces from a name, so `.mcp.json.` opens `.mcp.json`; and
/// `name:stream` / `name::$DATA` write the named file through an alternate data stream.
#[test]
fn windows_trailing_dots_spaces_and_streams_are_refused() {
    for path in [
        "project/.mcp.json.",
        "project/.mcp.json. . ",
        "project/.git./hooks/pre-commit",
        "project/.envrc ",
        "project/.mcp.json::$DATA",
        "project/.bashrc:stream",
        // Any stream syntax is refused, even on an otherwise ordinary name.
        "project/src/main.rs:hidden",
    ] {
        assert!(
            protected(path, WINDOWS),
            "{path:?} must be protected on Windows"
        );
    }
    assert!(!protected("project/src/main.rs", WINDOWS));
    // On POSIX a trailing dot is part of the name, so these are different files.
    assert!(!protected("/project/.mcp.json.", POSIX));
}

/// The app's own settings, profile store and prompt-template dirs live at platform- and
/// channel-specific absolute paths; anything under them is protected.
#[test]
fn app_owned_dirs_are_matched_by_absolute_prefix() {
    let app_dirs = [
        PathBuf::from("/home/u/.config/phosphor"),
        PathBuf::from("/home/u/.local/state/phosphor"),
        PathBuf::from("/home/u/my-prompts"),
    ];
    for path in [
        "/home/u/.config/phosphor/settings.toml",
        "/home/u/.local/state/phosphor/warp.sqlite",
        "/home/u/my-prompts/system.j2",
        "/home/u/my-prompts/../my-prompts/system.j2",
    ] {
        assert!(
            is_protected_write_path_with(Path::new(path), POSIX, &app_dirs),
            "{path:?} is inside an app-owned dir"
        );
    }
    for path in ["/home/u/.config/phosphorus/x", "/home/u/my-prompts-old/x"] {
        assert!(
            !is_protected_write_path_with(Path::new(path), POSIX, &app_dirs),
            "{path:?} only shares a name prefix with an app-owned dir"
        );
    }
}

/// `warp_core::paths` returns an EMPTY path when the home directory is unknown, and every path
/// starts with the empty path. Such an entry must be ignored, not turn the guard into "deny
/// everything"; and the host's real app dirs must all be absolute.
#[test]
fn empty_or_relative_app_dirs_do_not_protect_everything() {
    assert!(
        app_owned_dirs().iter().all(|dir| dir.is_absolute()),
        "app_owned_dirs must filter out non-absolute entries"
    );
    assert!(!is_protected_write_path_with(
        Path::new("/project/src/main.rs"),
        POSIX,
        &[PathBuf::new()],
    ));
}
