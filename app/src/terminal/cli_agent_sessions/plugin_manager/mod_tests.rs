use std::cmp::Ordering;

use std::cell::Cell;
use std::fs;

use super::{ConfigFileCheckCache, compare_versions, is_warp_plugin_source, plugin_manager_for};
use crate::terminal::CLIAgent;

#[test]
fn returns_manager_for_claude() {
    assert!(plugin_manager_for(CLIAgent::Claude).is_some());
}

#[test]
fn returns_manager_for_opencode() {
    let _oc_guard = crate::features::FeatureFlag::OpenCodeNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::OpenCode).is_some());
}

#[test]
fn returns_manager_for_codex() {
    let _codex_guard = crate::features::FeatureFlag::CodexNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::Codex).is_some());
}

#[test]
fn returns_manager_for_gemini() {
    let _gemini_guard = crate::features::FeatureFlag::GeminiNotifications.override_enabled(true);
    let _hoa_guard = crate::features::FeatureFlag::HOANotifications.override_enabled(true);
    assert!(plugin_manager_for(CLIAgent::Gemini).is_some());
}

#[test]
fn returns_none_for_unsupported_agents() {
    assert!(plugin_manager_for(CLIAgent::Amp).is_none());
    assert!(plugin_manager_for(CLIAgent::Droid).is_none());
    assert!(plugin_manager_for(CLIAgent::Copilot).is_none());
    assert!(plugin_manager_for(CLIAgent::Unknown).is_none());
}

#[test]
fn compare_versions_equal() {
    assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
}

#[test]
fn compare_versions_less_than_major() {
    assert_eq!(compare_versions("1.0.0", "2.0.0"), Ordering::Less);
}

#[test]
fn compare_versions_less_than_minor() {
    assert_eq!(compare_versions("1.1.0", "1.2.0"), Ordering::Less);
}

#[test]
fn compare_versions_less_than_patch() {
    assert_eq!(compare_versions("1.1.0", "1.1.1"), Ordering::Less);
}

#[test]
fn compare_versions_greater_than() {
    assert_eq!(compare_versions("3.0.0", "2.0.0"), Ordering::Greater);
}

#[test]
fn compare_versions_unparseable_treated_as_zero() {
    assert_eq!(compare_versions("abc", "0.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("abc", "1.0.0"), Ordering::Less);
}

#[test]
fn compare_versions_partial_version_string() {
    assert_eq!(compare_versions("2", "2.0.0"), Ordering::Equal);
    assert_eq!(compare_versions("2.1", "2.1.0"), Ordering::Equal);
}

#[test]
fn compare_versions_empty_string() {
    assert_eq!(compare_versions("", "2.0.0"), Ordering::Less);
}

#[test]
fn warp_plugin_source_matches_every_recorded_spelling() {
    for source in [
        "warpdotdev/claude-code-warp",
        "WarpDotDev/Claude-Code-Warp",
        "warpdotdev/claude-code-warp#main",
        "warpdotdev/claude-code-warp@v2",
        "github:warpdotdev/claude-code-warp",
        "https://github.com/warpdotdev/claude-code-warp",
        "https://github.com/warpdotdev/claude-code-warp/",
        "https://github.com/warpdotdev/claude-code-warp.git",
        "https://github.com/warpdotdev/claude-code-warp.git#stable",
        "https://www.github.com/warpdotdev/claude-code-warp",
        "git@github.com:warpdotdev/claude-code-warp.git",
        "ssh://git@github.com/warpdotdev/claude-code-warp.git",
        "  https://github.com/warpdotdev/claude-code-warp  ",
    ] {
        assert!(
            is_warp_plugin_source(source, "claude-code-warp"),
            "{source}"
        );
    }
}

#[test]
fn warp_plugin_source_rejects_other_sources() {
    for source in [
        "https://gitlab.com/winters-phosphor/claude-code-warp.git",
        "winters-phosphor/claude-code-warp",
        "warpdotdev/claude-code-warp-internal",
        "https://github.com/warpdotdev/claude-code-warp-internal.git",
        "https://github.com/warpdotdev/codex-warp.git",
        "https://notgithub.com/warpdotdev/claude-code-warp",
        "https://gitlab.com/warpdotdev/claude-code-warp.git",
        "/home/dev/claude-code-warp",
        "",
    ] {
        assert!(
            !is_warp_plugin_source(source, "claude-code-warp"),
            "{source}"
        );
    }
}

#[test]
fn config_file_check_cache_recomputes_only_when_a_file_changes() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("config.json");
    fs::write(&file, "{}").unwrap();
    let files = [file.clone(), dir.path().join("missing.json")];
    let cache = ConfigFileCheckCache::new();
    let runs = Cell::new(0);
    let check = |answer: bool| {
        cache.get_or_compute(&files, || {
            runs.set(runs.get() + 1);
            answer
        })
    };

    assert!(check(true));
    // Unchanged files: the memoised answer, without running the check.
    assert!(check(false));
    assert_eq!(runs.get(), 1);

    // A different length changes the key even within one mtime tick.
    fs::write(&file, "{\"moved\": true}").unwrap();
    assert!(!check(false));
    assert_eq!(runs.get(), 2);
}
