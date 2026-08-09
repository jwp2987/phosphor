use super::*;
use std::path::PathBuf;
use warp_util::host_id::HostId;

// Measured against the pinned oracle (`02b53fcd8`, release `2026.07.29.09.05`
// stable — see `ORACLE.md`), whose same-path `model_tests.rs` has 29 `#[test]`s.
// 16 of those are now present above/below (the `test_find_applicable_rules_*`
// group, plus the 5 global-rule tests ported below as part of #575); the
// remaining 13 break down as:
//
//   7 portable — `test_no_rules_returns_none` (needs only
//     `ProjectContextModel::default()` and `find_applicable_rules`, both
//     already present) and the 6 `test_merge_*` tests, both ported at the
//     bottom of this file. Issue #150 lists all 18 as blocked; these 7 are not.
//
//     `test_merge_*` needed `RulesDelta::merge`, ported alongside them in
//     `model.rs`. At the pin `merge` is `#[cfg(test)]`-only — `model.rs` never
//     calls it outside `model_tests.rs`
//     (`git grep '\.merge(' 02b53fcd8 -- crates/ai/src/project_context/`
//     matches only the pin's own test file) — so this fork's port keeps the
//     same `#[cfg(test)]` gate rather than inventing a production call site
//     that doesn't exist upstream either. This is a different situation from
//     `ApiKeys::provider_key_count` in `crates/ai/src/api_keys_tests.rs`
//     (#207): that method is dead in *every* build, test included, whereas
//     `merge` is real, exercised pinned test code, just never called from
//     non-test code at the pin.
//
//   5 UNBLOCKED by #575 — `test_global_rule_alone_no_project_rules`,
//     `test_global_rule_layered_with_project_warp`,
//     `test_global_rule_root_path_falls_back_to_parent`,
//     `test_in_dir_warp_shadows_agents_with_global`,
//     `test_multiple_global_rules_all_contribute`, ported below. #575 added
//     `ProjectContextModel::global_rules` (a `GlobalRules`, fed by the new
//     `project_context/global_rules.rs` watcher module) plus a *separate*
//     `find_applicable_rules_with_globals` layering entry point — kept
//     separate from the existing `find_applicable_rules` rather than folding
//     globals into it, because `find_applicable_rules` has a project-only
//     caller (`code_review_view.rs`'s "already initialized" hint) that must
//     not regress when a stray `~/.agents/AGENTS.md` exists. Adapted from the
//     pin's `model.find_applicable_rules(&local_path(...))` calls to this
//     fork's `model.find_applicable_rules_with_globals(&PathBuf::from(...))`
//     accordingly — same layering behavior, different (fork-specific) entry
//     point name. The pin's 6th global-rule test,
//     `test_remote_global_rules_only_layer_for_matching_remote_host`, is now
//     split: its global-rules-layering-and-per-host-isolation half is adapted
//     below as `test_remote_project_rules_layers_local_global_ahead_of_remote_global`
//     (added once `ProjectContextModel::remote_project_rules` gave
//     `remote_global_rules` an actual consumer — see that method's doc comment
//     in `model.rs`); its remote *project*-rule half stays blocked alongside
//     `test_remote_project_rules_require_matching_host` below, for the same
//     reason.
//
//   2 blocked on `ProjectContextModel::reconcile_project_rules` —
//     `test_missing_rule_content_preserves_cached_content_while_path_is_standing`
//     and `test_rule_missing_from_standing_results_is_removed_from_cached_content`.
//     Absent here, along with `ProjectRules::rule_paths`. Refs #150 item 2, #201.
//
//   3 blocked on remote (`LocalOrRemotePath`) *project* rules (as opposed to
//     *global* rules, which now have a lookup — see above) —
//     `test_remote_project_rules_require_matching_host`,
//     `test_remote_standing_results_preserve_host_qualified_rule_paths`,
//     `test_reconcile_project_rules_hydrates_local_and_remote_paths`. This
//     fork's `ProjectRule::path`, `ProjectRulePath` and `path_to_rules` are all
//     keyed by `PathBuf`; `find_applicable_project_rules` explicitly returns
//     `None` for every remote path, and porting these would mean giving
//     `path_to_rules` a `HostId` dimension it doesn't have — a materially
//     larger restructuring than adding the global-rules-only lookup above.
//     Refs #150 item 2, #170.

/// Scans, then drops directory stamps for ancestors outside `root`.
///
/// `scan_fast_path` walks up to `MAX_WALK_DEPTH` ancestors and records each
/// directory's mtime, which for a `tempfile` tempdir includes the shared
/// system temp dir (e.g. `/tmp`). A concurrent test creating or dropping its
/// own tempdir bumps that mtime, so an unscoped "still valid" check run in
/// parallel flips nondeterministically. Restricting the recorded directory
/// stamps to the test's own subtree removes that shared-ancestor dependency;
/// rule detection is unaffected, since rules come from the file stamps.
#[cfg(feature = "local_fs")]
fn scan_fast_path_isolated(root: &Path) -> FastPathEntry {
    let mut entry = ProjectContextModel::scan_fast_path(root);
    entry.walked_dir_stamps.retain(|(dir, _)| dir.starts_with(root));
    entry
}

#[test]
fn test_find_applicable_rules_empty_rules() {
    let rules = ProjectRules { rules: vec![] };
    let path = PathBuf::from("/a/b/c/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert!(result.is_empty());
}

#[test]
fn test_find_applicable_rules_no_matching_rules() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/x/y/WARP.md"), "content1".to_string());
    rules.upsert_rule(Path::new("/z/AGENTS.md"), "content2".to_string());

    let path = PathBuf::from("/a/b/c/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert!(result.is_empty());
}

#[test]
fn test_find_applicable_rules_single_matching_rule() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/WARP.md"), "content1".to_string());
    rules.upsert_rule(Path::new("/x/AGENTS.md"), "content2".to_string());

    let path = PathBuf::from("/a/b/c/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/WARP.md"));
}

#[test]
fn test_find_applicable_rules_includes_all_ancestor_rules() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/WARP.md"), "root_warp".to_string());
    rules.upsert_rule(Path::new("/a/b/WARP.md"), "nested_warp".to_string());
    rules.upsert_rule(Path::new("/a/b/c/WARP.md"), "deep_warp".to_string());

    let path = PathBuf::from("/a/b/c/d/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 3);

    // All should be WARP.md files (same priority), order is not specified by depth
    // Just verify all expected rules are present
    let paths: Vec<PathBuf> = result.iter().map(|r| r.path.clone()).collect();
    assert!(paths.contains(&PathBuf::from("/a/WARP.md")));
    assert!(paths.contains(&PathBuf::from("/a/b/WARP.md")));
    assert!(paths.contains(&PathBuf::from("/a/b/c/WARP.md")));
}

#[test]
fn test_find_applicable_rules_multiple_patterns() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/b/AGENTS.md"), "agents_content".to_string());
    rules.upsert_rule(Path::new("/a/WARP.md"), "warp_content".to_string());

    let path = PathBuf::from("/a/b/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 2);

    assert_eq!(result[0].path, PathBuf::from("/a/b/AGENTS.md"));
    assert_eq!(result[0].content, "agents_content");
    assert_eq!(result[1].path, PathBuf::from("/a/WARP.md"));
    assert_eq!(result[1].content, "warp_content");
}

#[test]
fn test_find_applicable_rules_exact_path_match() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/b/WARP.md"), "exact_match".to_string());

    let path = PathBuf::from("/a/b/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/b/WARP.md"));
    assert_eq!(result[0].content, "exact_match");
}

#[test]
fn test_find_applicable_rules_ignores_deeper_paths() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/WARP.md"), "applicable".to_string());
    rules.upsert_rule(Path::new("/a/b/c/d/e/WARP.md"), "too_deep".to_string()); // Path doesn't contain /a/b

    let path = PathBuf::from("/a/b/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/WARP.md"));
    assert_eq!(result[0].content, "applicable");
}

#[test]
fn test_find_applicable_rules_handles_root_path() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/WARP.md"), "root_rule".to_string());

    let path = PathBuf::from("/a/b/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/WARP.md"));
    assert_eq!(result[0].content, "root_rule");
}

#[test]
fn test_find_applicable_rules_complex_scenario() {
    // This test covers the example from the original request:
    // For path /a/b/c/file.rs with rules:
    // - /a/WARP.md
    // - /a/AGENTS.md
    // - /a/b/WARP.md
    // - /a/b/AGENTS.md
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/WARP.md"), "a_warp".to_string());
    rules.upsert_rule(Path::new("/a/AGENTS.md"), "a_agents".to_string());
    rules.upsert_rule(Path::new("/a/b/WARP.md"), "ab_warp".to_string());
    rules.upsert_rule(Path::new("/a/b/AGENTS.md"), "ab_agents".to_string());
    rules.upsert_rule(Path::new("/x/WARP.md"), "irrelevant".to_string()); // Should be ignored

    let path = PathBuf::from("/a/b/c/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 2);

    // Expect only WARP.md files to be included as they have higher priority.
    assert_eq!(result[0].path, PathBuf::from("/a/WARP.md"));
    assert_eq!(result[0].content, "a_warp");
    assert_eq!(result[1].path, PathBuf::from("/a/b/WARP.md"));
    assert_eq!(result[1].content, "ab_warp");
}

#[test]
fn test_find_applicable_rules_handles_unknown_file_patterns() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("/a/WARP.md"), "known_pattern".to_string());
    rules.upsert_rule(Path::new("/a/UNKNOWN.md"), "unknown_pattern".to_string());
    let path = PathBuf::from("/a/file.rs");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);

    assert_eq!(result[0].path, PathBuf::from("/a/WARP.md"));
    assert_eq!(result[0].content, "known_pattern");
}

#[test]
fn test_find_applicable_rules_with_relative_paths() {
    let mut rules = ProjectRules::default();

    rules.upsert_rule(Path::new("src/WARP.md"), "src_warp".to_string());
    rules.upsert_rule(
        Path::new("src/components/WARP.md"),
        "components_warp".to_string(),
    );

    let path = PathBuf::from("src/components/Button.tsx");

    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 2);

    // Both are WARP.md files (same priority), order within same priority is not guaranteed
    // Just verify both rules are present
    let paths: Vec<PathBuf> = result.iter().map(|r| r.path.clone()).collect();
    assert!(paths.contains(&PathBuf::from("src/WARP.md")));
    assert!(paths.contains(&PathBuf::from("src/components/WARP.md")));
}

// ---------------------------------------------------------------------------
// Fast-path tests (for ProjectContextModel::scan_fast_path + fast_path_entry_still_valid)
// ---------------------------------------------------------------------------
//
// These tests go through the real fs (temp directories), not depending on
// ModelContext. Coverage:
//   - cwd itself has AGENTS.md → hit
//   - WARP.md takes priority over AGENTS.md (same directory)
//   - Ancestor directory rule can be found via findUp
//   - No rule → returns None
//   - Invalidation check: modifying file mtime → still_valid returns false
//   - Invalidation check: adding a rule file in a walked directory → still_valid returns false

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_finds_agents_md_in_cwd() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "hello agents").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert_eq!(entry.rules.len(), 1, "expected to hit 1 rule");
    assert_eq!(entry.rules[0].content, "hello agents");
    assert_eq!(entry.rules[0].path, cwd.join("AGENTS.md"));
    assert_eq!(entry.root_path, cwd);
    assert_eq!(entry.stamps.len(), 1);
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_warp_md_takes_priority_over_agents_md() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("WARP.md"), "warp wins").unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "agents loses").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert_eq!(
        entry.rules.len(),
        1,
        "only 1 of 2 rule files in the same directory should be taken (aligned with RuleAtPath::respected_rule)"
    );
    assert_eq!(entry.rules[0].content, "warp wins");
    assert_eq!(entry.rules[0].path, cwd.join("WARP.md"));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_finds_rule_in_ancestor_directory() {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    let sub = root.join("a").join("b").join("c");
    std::fs::create_dir_all(&sub).unwrap();
    std::fs::write(root.join("AGENTS.md"), "ancestor rule").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&sub);
    assert_eq!(entry.rules.len(), 1);
    assert_eq!(entry.rules[0].content, "ancestor rule");
    assert_eq!(entry.root_path, root);
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_returns_empty_when_no_rules_anywhere() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert!(entry.rules.is_empty());
    // root_path falls back to cwd (semantically aligned with find_applicable_rules returning None)
    assert_eq!(entry.root_path, cwd);
    // walked_dir_stamps is not empty (at least cwd itself was walked, so the negative cache can take effect)
    assert!(!entry.walked_dir_stamps.is_empty());
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_still_valid_when_nothing_changed() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "stable").unwrap();

    let entry = scan_fast_path_isolated(&cwd);
    assert!(ProjectContextModel::fast_path_entry_still_valid(&entry));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_invalidated_when_rule_file_mtime_changes() {
    use filetime::{set_file_mtime, FileTime};

    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    let rule = cwd.join("AGENTS.md");
    std::fs::write(&rule, "v1").unwrap();

    let entry = scan_fast_path_isolated(&cwd);
    assert!(ProjectContextModel::fast_path_entry_still_valid(&entry));

    // Push mtime forward by 10s → the cache should be detected as invalidated
    let stamp = entry.stamps[0].1;
    let new_mtime = FileTime::from_system_time(stamp + std::time::Duration::from_secs(10));
    set_file_mtime(&rule, new_mtime).unwrap();
    assert!(!ProjectContextModel::fast_path_entry_still_valid(&entry));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_invalidated_when_new_rule_file_appears_in_walked_dir() {
    use filetime::{set_file_mtime, FileTime};

    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();

    // First scan: hits no rules at all (negative cache)
    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert!(entry.rules.is_empty());

    // Record the original directory mtime, then manually bump it later to trigger invalidation detection.
    // The file is only created here — but on some filesystems, creating a file doesn't immediately update the directory mtime.
    // For test stability, explicitly call set_file_mtime after creating the file to ensure the directory mtime differs from the stamp.
    std::fs::write(cwd.join("AGENTS.md"), "new!").unwrap();
    let original_dir_mtime = entry.walked_dir_stamps[0].1;
    let bumped =
        FileTime::from_system_time(original_dir_mtime + std::time::Duration::from_secs(10));
    set_file_mtime(&cwd, bumped).unwrap();

    assert!(!ProjectContextModel::fast_path_entry_still_valid(&entry));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_walk_depth_bounded() {
    // Verify MAX_WALK_DEPTH takes effect: a directory beyond the depth limit won't stat the top-level rule file.
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().canonicalize().unwrap();
    // Build a ≥7-level subdirectory (MAX_WALK_DEPTH = 6)
    let mut deep = root.clone();
    for seg in ["a", "b", "c", "d", "e", "f", "g"] {
        deep.push(seg);
    }
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(root.join("AGENTS.md"), "top").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&deep);
    // Can't reach the top level, so no rule is found
    assert!(entry.rules.is_empty(), "should not stat the top-level rule file once the depth limit is exceeded");
    // walked_dir_stamps does not exceed MAX_WALK_DEPTH
    assert!(entry.walked_dir_stamps.len() <= 6);
}

// ---------------------------------------------------------------------------
// CLAUDE.md default-recognition dedicated tests
// ---------------------------------------------------------------------------

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_finds_claude_md() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "claude rules").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert_eq!(entry.rules.len(), 1, "CLAUDE.md should be recognized by default");
    assert_eq!(entry.rules[0].content, "claude rules");
    assert_eq!(entry.rules[0].path, cwd.join("CLAUDE.md"));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_warp_md_priority_over_claude_md() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("WARP.md"), "warp wins").unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "claude loses").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert_eq!(entry.rules.len(), 1);
    assert_eq!(entry.rules[0].content, "warp wins");
    assert_eq!(entry.rules[0].path, cwd.join("WARP.md"));
}

#[cfg(feature = "local_fs")]
#[test]
fn fast_path_agents_md_priority_over_claude_md() {
    let tmp = tempfile::tempdir().unwrap();
    let cwd = tmp.path().canonicalize().unwrap();
    std::fs::write(cwd.join("AGENTS.md"), "agents wins").unwrap();
    std::fs::write(cwd.join("CLAUDE.md"), "claude loses").unwrap();

    let entry = ProjectContextModel::scan_fast_path(&cwd);
    assert_eq!(entry.rules.len(), 1);
    assert_eq!(entry.rules[0].content, "agents wins");
    assert_eq!(entry.rules[0].path, cwd.join("AGENTS.md"));
}

#[test]
fn upsert_rule_recognizes_claude_md() {
    // Pure in-memory path (no fs) verifying that ProjectRules::upsert_rule recognizes CLAUDE.md
    let mut rules = ProjectRules::default();
    rules.upsert_rule(Path::new("/a/CLAUDE.md"), "claude in /a".to_string());

    let path = PathBuf::from("/a/sub/file.rs");
    let result = rules.find_active_or_applicable_rules(&path).active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/CLAUDE.md"));
    assert_eq!(result[0].content, "claude in /a");
}

#[test]
fn upsert_rule_priority_three_way() {
    // WARP / AGENTS / CLAUDE coexist in the same directory → only the highest-priority WARP is taken
    let mut rules = ProjectRules::default();
    rules.upsert_rule(Path::new("/a/WARP.md"), "warp".to_string());
    rules.upsert_rule(Path::new("/a/AGENTS.md"), "agents".to_string());
    rules.upsert_rule(Path::new("/a/CLAUDE.md"), "claude".to_string());

    let result = rules
        .find_active_or_applicable_rules(&PathBuf::from("/a/x.rs"))
        .active_rules;
    assert_eq!(result.len(), 1, "only the highest-priority rule file is taken when multiple exist in the same directory");
    assert_eq!(result[0].path, PathBuf::from("/a/WARP.md"));
}

#[test]
fn upsert_rule_priority_agents_beats_claude() {
    // AGENTS + CLAUDE in the same directory → AGENTS is taken
    let mut rules = ProjectRules::default();
    rules.upsert_rule(Path::new("/a/AGENTS.md"), "agents".to_string());
    rules.upsert_rule(Path::new("/a/CLAUDE.md"), "claude".to_string());

    let result = rules
        .find_active_or_applicable_rules(&PathBuf::from("/a/x.rs"))
        .active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/AGENTS.md"));
}

#[test]
fn remove_rule_recognizes_claude_md() {
    let mut rules = ProjectRules::default();
    rules.upsert_rule(Path::new("/a/CLAUDE.md"), "x".to_string());
    rules.upsert_rule(Path::new("/a/AGENTS.md"), "y".to_string());

    let removed = rules.remove_rule(Path::new("/a/CLAUDE.md"));
    assert!(removed.is_some(), "should be able to remove CLAUDE.md");

    // After removing CLAUDE, AGENTS remains as the effective rule for this directory
    let result = rules
        .find_active_or_applicable_rules(&PathBuf::from("/a/x.rs"))
        .active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/AGENTS.md"));
}

#[test]
fn upsert_rule_case_insensitive_filename() {
    // Case-insensitive: claude.md / Agents.MD are also recognized
    let mut rules = ProjectRules::default();
    rules.upsert_rule(Path::new("/a/claude.md"), "lower".to_string());

    let result = rules
        .find_active_or_applicable_rules(&PathBuf::from("/a/x.rs"))
        .active_rules;
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].path, PathBuf::from("/a/claude.md"));
}

#[test]
fn test_no_rules_returns_none() {
    let model = ProjectContextModel::default();
    let result = model.find_applicable_rules(&PathBuf::from("/some/path/file.rs"));
    assert!(result.is_none());
}

// ---------------------------------------------------------------------------
// Global-rules layering tests (#575), ported from the pinned oracle's
// `model_tests.rs` (`02b53fcd8`). The pin exercises this via
// `model.find_applicable_rules(&LocalOrRemotePath)`; this fork keeps
// `find_applicable_rules` project-only (see the file-level comment above) and
// exposes the layered lookup as `find_applicable_rules_with_globals` instead,
// so calls below are adapted to that name and to this fork's local-only
// `PathBuf` paths. Assertions are otherwise unchanged from the pin.
// ---------------------------------------------------------------------------

/// Helper for global-rules tests: inserts a synthetic global rule directly
/// into the model, bypassing the watcher infrastructure (which requires the
/// warpui runtime) so we can exercise the layering logic directly.
///
/// `GlobalRules::rules` only exists on the real (`local_fs`) implementation —
/// the `!local_fs` build swaps in a fieldless dummy (`dummy_global_rules.rs`)
/// — so this helper and everything that calls it is gated accordingly,
/// mirroring the existing `fast_path_*` test gating above.
#[cfg(feature = "local_fs")]
fn insert_global_rule(model: &mut ProjectContextModel, path: &Path, content: &str) {
    model.global_rules.rules.insert(
        path.to_path_buf(),
        ProjectRule {
            path: path.to_path_buf(),
            content: content.to_string(),
        },
    );
}

#[cfg(feature = "local_fs")]
fn insert_project_rule(
    model: &mut ProjectContextModel,
    project_root: &Path,
    rule_path: &Path,
    content: &str,
) {
    let rules = model
        .path_to_rules
        .entry(project_root.to_path_buf())
        .or_default();
    rules.upsert_rule(rule_path, content.to_string());
}

#[test]
#[cfg(feature = "local_fs")]
fn test_global_rule_alone_no_project_rules() {
    let mut model = ProjectContextModel::default();
    insert_global_rule(
        &mut model,
        Path::new("/home/u/.agents/AGENTS.md"),
        "global_content",
    );

    let result = model
        .find_applicable_rules_with_globals(Path::new("/some/project/file.rs"))
        .expect("global rule should produce a result");

    assert_eq!(result.active_rules.len(), 1);
    assert_eq!(
        result.active_rules[0].path,
        PathBuf::from("/home/u/.agents/AGENTS.md")
    );
    assert_eq!(result.active_rules[0].content, "global_content");
    assert!(result.additional_rule_paths.is_empty());
}

#[test]
#[cfg(feature = "local_fs")]
fn test_global_rule_layered_with_project_warp() {
    let mut model = ProjectContextModel::default();
    insert_global_rule(&mut model, Path::new("/home/u/.agents/AGENTS.md"), "global");
    insert_project_rule(
        &mut model,
        Path::new("/repo"),
        Path::new("/repo/WARP.md"),
        "project_warp",
    );

    let result = model
        .find_applicable_rules_with_globals(Path::new("/repo/src/main.rs"))
        .expect("layered rules should produce a result");

    // Layered precedence: global first, then project rules.
    assert_eq!(result.active_rules.len(), 2);
    assert_eq!(result.active_rules[0].content, "global");
    assert_eq!(result.active_rules[1].content, "project_warp");
    assert_eq!(result.root_path, PathBuf::from("/repo"));
}

#[test]
#[cfg(feature = "local_fs")]
fn test_in_dir_warp_shadows_agents_with_global() {
    let mut model = ProjectContextModel::default();
    insert_global_rule(&mut model, Path::new("/home/u/.agents/AGENTS.md"), "global");
    // Both WARP.md and AGENTS.md in the same project directory: WARP.md should
    // shadow AGENTS.md (existing in-directory behavior preserved).
    insert_project_rule(
        &mut model,
        Path::new("/repo"),
        Path::new("/repo/WARP.md"),
        "project_warp",
    );
    insert_project_rule(
        &mut model,
        Path::new("/repo"),
        Path::new("/repo/AGENTS.md"),
        "project_agents",
    );

    let result = model
        .find_applicable_rules_with_globals(Path::new("/repo/src/main.rs"))
        .expect("layered rules should produce a result");

    // Expect: [global, project WARP.md]. project AGENTS.md is shadowed.
    assert_eq!(result.active_rules.len(), 2);
    assert_eq!(result.active_rules[0].content, "global");
    assert_eq!(result.active_rules[1].content, "project_warp");
}

#[test]
#[cfg(feature = "local_fs")]
fn test_global_rule_root_path_falls_back_to_parent() {
    let mut model = ProjectContextModel::default();
    insert_global_rule(&mut model, Path::new("/home/u/.agents/AGENTS.md"), "global");

    let result = model
        .find_applicable_rules_with_globals(Path::new("/some/file.rs"))
        .expect("global rule should produce a result");

    // No project root indexed; root_path falls back to parent of the global rule.
    assert_eq!(result.root_path, PathBuf::from("/home/u/.agents"));
}

#[test]
#[cfg(feature = "local_fs")]
fn test_multiple_global_rules_all_contribute() {
    let mut model = ProjectContextModel::default();
    insert_global_rule(
        &mut model,
        Path::new("/home/u/.agents/AGENTS.md"),
        "agents_global",
    );
    insert_global_rule(
        &mut model,
        Path::new("/home/u/.warp/WARP.md"),
        "warp_global",
    );

    let result = model
        .find_applicable_rules_with_globals(Path::new("/repo/src/main.rs"))
        .expect("globals should produce a result");

    assert_eq!(result.active_rules.len(), 2);
    let contents: Vec<&str> = result
        .active_rules
        .iter()
        .map(|r| r.content.as_str())
        .collect();
    assert!(contents.contains(&"agents_global"));
    assert!(contents.contains(&"warp_global"));
}

#[test]
#[cfg(feature = "local_fs")]
fn test_find_rules_with_fast_path_layers_globals() {
    // find_rules_with_fast_path (the agent-context entry point) must also
    // layer globals, not just find_applicable_rules_with_globals directly.
    let mut model = ProjectContextModel::default();
    insert_global_rule(&mut model, Path::new("/home/u/.agents/AGENTS.md"), "global");
    insert_project_rule(
        &mut model,
        Path::new("/repo"),
        Path::new("/repo/WARP.md"),
        "project_warp",
    );

    let result = model
        .find_rules_with_fast_path(Path::new("/repo/src/main.rs"))
        .expect("layered rules should produce a result");

    assert_eq!(result.active_rules.len(), 2);
    assert_eq!(result.active_rules[0].content, "global");
    assert_eq!(result.active_rules[1].content, "project_warp");
}

#[test]
fn test_set_and_remove_remote_global_rules() {
    // Per-host storage round-trip (#575): asserts the getter/setter/remover
    // themselves. `remote_project_rules` (exercised by the tests below) is the
    // lookup that now consumes this storage.
    let mut model = ProjectContextModel::default();
    let host_a = HostId::new("host-a".to_string());

    model.set_remote_global_rules(
        host_a.clone(),
        vec![ProjectRule {
            path: PathBuf::from("/home/remote/.agents/AGENTS.md"),
            content: "remote_global".to_string(),
        }],
    );
    assert_eq!(model.remote_global_rules(&host_a).len(), 1);
    assert_eq!(
        model.remote_global_rules(&host_a)[0].content,
        "remote_global"
    );

    model.remove_remote_global_rules(&host_a);
    assert!(model.remote_global_rules(&host_a).is_empty());
}

#[test]
fn test_remote_project_rules_none_when_nothing_stored() {
    let model = ProjectContextModel::default();
    let host = HostId::new("host-a".to_string());
    assert!(model.remote_project_rules(&host).is_none());
}

#[test]
fn test_remote_project_rules_isolates_by_host() {
    // No local global rules in this one (deliberately not `#[cfg(feature =
    // "local_fs")]`-gated, unlike the richer layering test below, which needs
    // `insert_global_rule`): `remote_project_rules` must work standalone from
    // `remote_global_rules` storage alone.
    let mut model = ProjectContextModel::default();
    let host_a = HostId::new("host-a".to_string());
    let host_b = HostId::new("host-b".to_string());

    model.set_remote_global_rules(
        host_a.clone(),
        vec![ProjectRule {
            path: PathBuf::from("/home/remote/.agents/AGENTS.md"),
            content: "remote_global_a".to_string(),
        }],
    );
    model.set_remote_global_rules(
        host_b.clone(),
        vec![ProjectRule {
            path: PathBuf::from("/home/remote/.agents/AGENTS.md"),
            content: "remote_global_b".to_string(),
        }],
    );

    let result_a = model
        .remote_project_rules(&host_a)
        .expect("host-a rules should produce a result");
    assert_eq!(result_a.active_rules.len(), 1);
    assert_eq!(result_a.active_rules[0].content, "remote_global_a");
    assert_eq!(result_a.root_path, PathBuf::from("/home/remote/.agents"));

    let result_b = model
        .remote_project_rules(&host_b)
        .expect("host-b rules should produce a result");
    assert_eq!(result_b.active_rules.len(), 1);
    assert_eq!(result_b.active_rules[0].content, "remote_global_b");

    // Neither host's result may leak the other's content.
    assert!(
        !result_a
            .active_rules
            .iter()
            .any(|rule| rule.content == "remote_global_b")
    );
    assert!(
        !result_b
            .active_rules
            .iter()
            .any(|rule| rule.content == "remote_global_a")
    );

    let host_c = HostId::new("host-c".to_string());
    assert!(
        model.remote_project_rules(&host_c).is_none(),
        "a host with nothing stored and no local global rules should produce no result"
    );
}

#[test]
#[cfg(feature = "local_fs")]
fn test_remote_project_rules_layers_local_global_ahead_of_remote_global() {
    // Adapted from the pin's `test_remote_global_rules_only_layer_for_matching_remote_host`
    // (`02b53fcd8`): that test also layers a remote *project* rule, which this fork can't
    // produce (`path_to_rules` has no per-host dimension — see `remote_global_rules`'s doc
    // comment in `model.rs`), so this covers the part that IS portable: this client's own
    // global rules apply ahead of every host's own remote global rules, and a host's
    // `remote_project_rules` never leaks another host's content.
    let mut model = ProjectContextModel::default();
    insert_global_rule(
        &mut model,
        Path::new("/home/local/.agents/AGENTS.md"),
        "local_global",
    );

    let host_a = HostId::new("host-a".to_string());
    let host_b = HostId::new("host-b".to_string());
    model.set_remote_global_rules(
        host_a.clone(),
        vec![ProjectRule {
            path: PathBuf::from("/home/remote/.agents/AGENTS.md"),
            content: "remote_global_a".to_string(),
        }],
    );
    model.set_remote_global_rules(
        host_b.clone(),
        vec![ProjectRule {
            path: PathBuf::from("/home/remote/.agents/AGENTS.md"),
            content: "remote_global_b".to_string(),
        }],
    );

    let result_a = model
        .remote_project_rules(&host_a)
        .expect("host-a should produce a result");
    assert_eq!(
        result_a
            .active_rules
            .iter()
            .map(|rule| rule.content.as_str())
            .collect::<Vec<_>>(),
        ["local_global", "remote_global_a"]
    );

    let result_b = model
        .remote_project_rules(&host_b)
        .expect("host-b should produce a result");
    assert_eq!(
        result_b
            .active_rules
            .iter()
            .map(|rule| rule.content.as_str())
            .collect::<Vec<_>>(),
        ["local_global", "remote_global_b"]
    );

    // A host with nothing stored still sees the local global rule (it applies
    // everywhere), but never another host's remote content.
    let host_c = HostId::new("host-c".to_string());
    let result_c = model
        .remote_project_rules(&host_c)
        .expect("local global rule alone should still produce a result");
    assert_eq!(
        result_c
            .active_rules
            .iter()
            .map(|rule| rule.content.as_str())
            .collect::<Vec<_>>(),
        ["local_global"]
    );
}

// Ported unchanged from the pinned oracle's `model_tests.rs`
// (`02b53fcd8`, release `2026.07.29.09.05` stable). `RulesDelta` and
// `ProjectRulePath` are field-identical between fork and pin, so these are a
// pure port of `RulesDelta::merge`'s pinned test coverage. See #150 item 2.
fn make_rule_path(path: &str) -> ProjectRulePath {
    ProjectRulePath {
        path: PathBuf::from(path),
        project_root: PathBuf::from("/project"),
    }
}

#[test]
fn test_merge_independent_deltas() {
    let mut delta = RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    };
    delta.merge(RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/b/WARP.md")],
    });

    assert_eq!(delta.discovered_rules.len(), 1);
    assert_eq!(delta.discovered_rules[0].path, PathBuf::from("/a/WARP.md"));
    assert_eq!(delta.deleted_rules, vec![PathBuf::from("/b/WARP.md")]);
}

#[test]
fn test_merge_add_then_delete_yields_delete() {
    let mut delta = RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    };
    delta.merge(RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/a/WARP.md")],
    });

    assert!(delta.discovered_rules.is_empty());
    assert_eq!(delta.deleted_rules, vec![PathBuf::from("/a/WARP.md")]);
}

#[test]
fn test_merge_delete_then_add_yields_add() {
    let mut delta = RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/a/WARP.md")],
    };
    delta.merge(RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    });

    assert_eq!(delta.discovered_rules.len(), 1);
    assert_eq!(delta.discovered_rules[0].path, PathBuf::from("/a/WARP.md"));
    assert!(delta.deleted_rules.is_empty());
}

#[test]
fn test_merge_add_delete_add_yields_add() {
    let mut delta = RulesDelta::default();
    delta.merge(RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    });
    delta.merge(RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/a/WARP.md")],
    });
    delta.merge(RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    });

    assert_eq!(delta.discovered_rules.len(), 1);
    assert_eq!(delta.discovered_rules[0].path, PathBuf::from("/a/WARP.md"));
    assert!(delta.deleted_rules.is_empty());
}

#[test]
fn test_merge_delete_add_delete_yields_delete() {
    let mut delta = RulesDelta::default();
    delta.merge(RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/a/WARP.md")],
    });
    delta.merge(RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    });
    delta.merge(RulesDelta {
        discovered_rules: vec![],
        deleted_rules: vec![PathBuf::from("/a/WARP.md")],
    });

    assert!(delta.discovered_rules.is_empty());
    assert_eq!(delta.deleted_rules, vec![PathBuf::from("/a/WARP.md")]);
}

#[test]
fn test_merge_rediscovery_keeps_latest() {
    let mut delta = RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    };
    // A second discovery of the same path (content update) should deduplicate.
    delta.merge(RulesDelta {
        discovered_rules: vec![make_rule_path("/a/WARP.md")],
        deleted_rules: vec![],
    });

    assert_eq!(delta.discovered_rules.len(), 1);
    assert!(delta.deleted_rules.is_empty());
}
