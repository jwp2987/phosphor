//! Single source of truth for the project rule-file names (WARP.md / AGENTS.md /
//! CLAUDE.md) that both the local rules pipeline (`crates/ai`) and the
//! repo-metadata standing-query indexer (`crates/repo_metadata`) must agree on.
//!
//! Before this module existed, `crates/ai::project_context::model::RULES_FILE_PATTERN`
//! recognized `CLAUDE.md` but `crates/repo_metadata::standing_queries::StandingQueryDefinitions`
//! had its own, separate `["WARP.md", "AGENTS.md"]` list, so a `CLAUDE.md` in a
//! repository never showed up in that indexer's standing-query results, even though
//! the same file worked locally via the `ai` crate's own fast-path scan.
//!
//! **This is indexing-list prep, not a working remote-rules pipeline.** This fork
//! has no production code path that turns a `repo_metadata` standing-query rule-file
//! result into content the agent's context actually receives for a *remote* (SSH)
//! session: `ai::project_context::model::standing_project_rule_paths` and
//! `ProjectContextModel::reconcile_project_rules` — the functions that would bridge
//! `repo_metadata`'s results into `ProjectContextModel`'s `remote_path_to_rules` — have
//! no non-test caller (see their doc comments), and `remote_path_to_rules` itself has
//! no production writer at all. Remote project-rule *discovery and content-reading*
//! (the pin's `app/src/ai/metadata_project_rules.rs`) does not exist on this fork; see
//! `app/src/ai/remote_context_files.rs`'s module doc comment for the adjacent,
//! already-wired case (remote project *skills*) this does not share. So today,
//! `CLAUDE.md` (like `WARP.md`/`AGENTS.md`) reaches the agent only for a local
//! session; single-sourcing this list just keeps the indexer's rule-file recognition
//! from drifting further from the local pipeline's while that remote pipeline is
//! built. Tracked in `TODO.md`.
//!
//! Order = priority (earlier wins) when multiple rule files coexist in the same
//! directory.

/// Default list of rule files, in priority order (earlier wins).
///
/// - WARP.md   the project's native convention.
/// - AGENTS.md community-wide convention (recognized by opencode / Cursor / Cline etc.).
/// - CLAUDE.md Claude Code's native convention, so projects migrated from Claude Code work out of the box.
pub const RULES_FILE_PATTERN: &[&str] = &["WARP.md", "AGENTS.md", "CLAUDE.md"];
