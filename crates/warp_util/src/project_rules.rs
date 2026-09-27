//! Single source of truth for the project rule-file names (WARP.md / AGENTS.md /
//! CLAUDE.md) that both the local rules pipeline (`crates/ai`) and the
//! repo-metadata standing-query indexer (`crates/repo_metadata`, the only rule
//! source over SSH) must agree on.
//!
//! Before this module existed, `crates/ai::project_context::model::RULES_FILE_PATTERN`
//! recognized `CLAUDE.md` but `crates/repo_metadata::standing_queries::StandingQueryDefinitions`
//! had its own, separate `["WARP.md", "AGENTS.md"]` list — so a remote repo's
//! CLAUDE.md was never indexed and never reached the agent over SSH, even though
//! the same file worked locally via the `ai` crate's fast-path scan.
//!
//! Order = priority (earlier wins) when multiple rule files coexist in the same
//! directory.

/// Default list of rule files, in priority order (earlier wins).
///
/// - WARP.md   the project's native convention.
/// - AGENTS.md community-wide convention (recognized by opencode / Cursor / Cline etc.).
/// - CLAUDE.md Claude Code's native convention, so projects migrated from Claude Code work out of the box.
pub const RULES_FILE_PATTERN: &[&str] = &["WARP.md", "AGENTS.md", "CLAUDE.md"];
