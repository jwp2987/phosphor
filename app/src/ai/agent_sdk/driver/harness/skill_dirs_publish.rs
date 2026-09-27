//! Makes the skills listed in `WARP_SKILL_DIRS` available to third-party
//! harnesses (Claude Code, Codex), by symlinking them into a skill root each
//! harness already searches on its own.
//!
//! Oz reads `WARP_SKILL_DIRS` directly (see
//! `crate::ai::agent_sdk::driver::AgentDriver::load_skills_dirs`). Third-party
//! harnesses discover skills from their own skill roots instead, so this
//! module reads the same `WARP_SKILL_DIRS` directories and symlinks each
//! skill folder into the harness's skill root, under the skill's own name.
//! The published name must match the real skill name (rather than some
//! namespaced alias) because an agent prompt, or another skill, may
//! reference a skill by that name. Skill frontmatter is never rewritten.
//!
//! A publish target already counts as ours only when it is a symlink whose
//! canonical destination is this exact source directory — publishing is then
//! a no-op. A publish target's working directory is not guaranteed to be
//! fresh: a dormant harness session can wake for a follow-up and re-publish
//! into the same working directory it used before, so "is a symlink" alone
//! does not imply "is ours" (a foreign symlink can exist too, and repeated
//! runs need a real identity check, not an assumption). Anything else at the
//! target — a real file or directory, or a symlink pointing elsewhere or
//! nowhere — is a genuine conflict with something that predates this publish
//! pass: we never modify it (see "Never rename aside" below). The skill is
//! instead published under a `warp-<name>` alias (subject to the same
//! ownership check), unless that alias also conflicts, in which case the
//! skill is not published at all.
//!
//! Every conflict is logged (see `logging-and-error-reporting`) with enough
//! detail to debug later — there is no user-facing surface for this today,
//! so the log is for us, not the user.
//!
//! A symlink — never a copy — keeps a skill's relative paths (for example a
//! helper script the skill invokes) pointing at the real, versioned skill
//! tree.
//!
//! ## Never rename aside (Phosphor-specific, IMPROVED over upstream — see `DECLINED.md`)
//!
//! Upstream renames a conflicting entry to `<name>.backup` and takes over its
//! name whenever `warp_isolation_platform::detect()` reports a sandbox, on the
//! premise that "in a detected sandbox we own the whole filesystem, so
//! nothing is lost by moving an entry aside". That premise does not hold in
//! this fork: `detect()` fires on the presence of `/.dockerenv`
//! (`crates/isolation_platform/src/docker.rs`), `KUBERNETES_SERVICE_HOST`, an
//! NSC token, or `WARP_ISOLATION_PLATFORM` — i.e. devcontainers, Codespaces,
//! and containerized CI, which are exactly the environments where
//! `working_dir` is the user's real, bind-mounted repo checkout rather than a
//! disposable container filesystem. Renaming an entry aside there renames the
//! user's own file, in their own repo, just because a container marker
//! happened to be present. So this fork never takes that branch, regardless
//! of what `detect()` reports: a conflicting entry is always left exactly as
//! it is, and the skill is published under the `warp-<name>` alias instead
//! (or not published at all, if the alias also conflicts). See `DECLINED.md`'s
//! `IMPROVED` section for the full writeup and the evidence.
//!
//! ## Scope: these symlinks become ordinary project skills (intended)
//!
//! Once published, `<working_dir>/.claude/skills/<name>` and
//! `<working_dir>/.agents/skills/<name>` are indistinguishable from any other
//! project skill: this fork's own scanner
//! (`crates/ai/src/skills/skill_provider.rs`) registers exactly those two
//! paths as project skill roots, and `app/src/ai/skills/file_watchers/utils.rs`
//! follows the symlink when walking them. The practical effect is that a
//! skill published here becomes visible to *every* conversation opened
//! against that repo, and shows up in the Skill Manager UI, for as long as
//! the symlink exists — not only to the harness run that published it. This
//! is an intended consequence, not a gap to close by teaching the scanner
//! about these links: the skills came from the operator's own
//! `WARP_SKILL_DIRS`, so surfacing them as ordinary project skills the
//! scanner already watches is the whole point of publishing them there. See
//! `DECLINED.md` and `TODO.md`'s `dff0d13fe` entry.
//!
//! ## Safety note (Phosphor-specific, not upstream)
//!
//! This fork previously fixed an arbitrary-file-read
//! (`tools/skill.rs`/`SKILL_FILE_PATTERN`, see `TODO.md` "FIXED 2026-08-21")
//! where a planted symlink at a *read* path let `parse_skill` open a file the
//! lexical validation never saw. This module is not that code path — it never
//! opens or reads through a symlink's target, only classifies filesystem
//! entries with `fs::symlink_metadata` (never dereferencing) and compares
//! canonicalized paths for identity. But because it *creates* filesystem
//! entries, the equivalent property to preserve here is: never overwrite, and
//! never treat as "ours" (and thus subject to no-op logic), a pre-existing
//! non-symlink or a symlink this pass did not itself create. See
//! `inspect_target`/`points_at_our_source` below, and
//! `skill_dirs_publish_tests.rs` for the tests guarding this specifically
//! (`publish_skill_dangling_symlink_target_is_treated_as_foreign`,
//! `publish_skill_is_a_noop_when_target_symlink_uses_relative_dotdot_path_to_our_source`).
//!
//! A second symlink hazard applies to the *parents* of the publish target,
//! not just the target itself: if `.claude`, `.claude/skills`, `.agents`, or
//! `.agents/skills` is itself a symlink planted by something else before this
//! pass ever ran, naively calling `fs::create_dir_all` and creating our
//! symlink underneath it would write wherever that symlink points — possibly
//! well outside `working_dir`. [`skill_root_is_safe_to_publish_into`] guards
//! against this by canonicalizing the *deepest existing ancestor* of
//! `skill_root` and refusing (logging, and skipping only that provider root)
//! unless the result is still inside `working_dir`. It also refuses when
//! `working_dir` itself is unreasonably broad — the filesystem root, or the
//! user's home directory — since publishing there would spray symlinks across
//! every unrelated repo and application that happens to share it.
//!
//! ## Keeping `git status` quiet
//!
//! In Warp's own use, `working_dir` is an ephemeral per-task container torn
//! down with the task, so the published symlinks never persist. In this fork
//! `working_dir` is typically the user's own repo checkout, so a
//! `WARP_SKILL_DIRS`-configured run leaves real symlinks under
//! `.claude/skills/` and `.agents/skills/` that would otherwise show up in
//! `git status` indefinitely. [`exclude_from_git_status`] best-effort adds the
//! paths this pass actually published to `.git/info/exclude` — never
//! `.gitignore`, never a tracked file, and never at the cost of failing the
//! publish itself.

use std::collections::HashSet;
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use ai::skills::{parse_skills_dirs_env, resolve_skills_dirs};
use anyhow::{Context, Result};
use warp_core::safe_warn;

/// Prefix used to publish a skill under an alternate name when a real,
/// non-symlink (or foreign-symlink) entry already occupies its real name.
const ALTERNATE_NAME_PREFIX: &str = "warp-";

/// Resolve the `WARP_SKILL_DIRS` source directories, most specific first —
/// the same directories and precedence order Oz uses (see
/// `ai::skills::read_skills_for_skills_dirs`).
pub(super) fn warp_skill_source_dirs(working_dir: &Path) -> Vec<PathBuf> {
    resolve_skills_dirs(working_dir, parse_skills_dirs_env())
}

/// Publish every skill found under `source_dirs` into `skill_root` as a
/// symlink under the skill's own name, pointing at the real skill folder.
/// Returns the number of skills published. See [`publish_skill`] for the
/// conflict-resolution behavior.
///
/// `working_dir` is the harness's own task working directory (`skill_root` is
/// always somewhere underneath it — `.claude/skills` or `.agents/skills`); it
/// is used only for the safety checks in
/// [`skill_root_is_safe_to_publish_into`] and to compute the paths recorded
/// in `.git/info/exclude` (see [`exclude_from_git_status`]). Nothing is
/// published (not even creating `skill_root`) when those checks refuse, or
/// when `source_dirs` is empty.
///
/// `source_dirs` is most-specific-first: when two directories contain a skill
/// folder with the same name, only the one from the first (most specific)
/// directory is published under that name — the same precedence Oz applies
/// when it reads these directories directly. This precedence choice among our
/// own source directories is not logged as a conflict; only a conflict with
/// an entry that did not come from this pass is (see [`publish_skill`]).
///
/// A failure to publish one skill (an unreadable directory, a missing
/// `SKILL.md`, a filesystem error) is logged and does not stop the rest of
/// the skills from publishing.
pub(super) fn publish_skill_dirs(
    skill_root: &Path,
    source_dirs: &[PathBuf],
    working_dir: &Path,
) -> usize {
    if source_dirs.is_empty() {
        return 0;
    }
    if !skill_root_is_safe_to_publish_into(skill_root, working_dir) {
        return 0;
    }

    let mut published_names = HashSet::new();
    let mut published_targets = Vec::new();
    let mut published = 0usize;
    for source_dir in source_dirs {
        let entries = match fs::read_dir(source_dir) {
            Ok(entries) => entries,
            Err(err) => {
                safe_warn!(
                    safe: ("WARP_SKILL_DIRS publish: skipping an unreadable source directory"),
                    full: ("WARP_SKILL_DIRS publish: skipping '{}' — {err}", source_dir.display())
                );
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    safe_warn!(
                        safe: ("WARP_SKILL_DIRS publish: failed to read a directory entry"),
                        full: (
                            "WARP_SKILL_DIRS publish: failed to read an entry in '{}': {err}",
                            source_dir.display()
                        )
                    );
                    continue;
                }
            };
            let source_path = entry.path();
            if !source_path.is_dir() || !source_path.join("SKILL.md").is_file() {
                // Not a skill folder.
                continue;
            }
            let Some(name) = source_path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if !published_names.insert(name.to_owned()) {
                // A more specific directory already published a skill with this name.
                continue;
            }
            match publish_skill(skill_root, name, &source_path) {
                Ok(Some(target)) => {
                    published += 1;
                    published_targets.push(target);
                }
                Ok(None) => {
                    // Deliberately skipped (a conflict whose alternate name also
                    // conflicted) — already logged by publish_skill.
                }
                Err(err) => {
                    safe_warn!(
                        safe: ("WARP_SKILL_DIRS publish: failed to publish a skill"),
                        full: ("WARP_SKILL_DIRS publish: failed to publish '{name}': {err:#}")
                    );
                }
            }
        }
    }
    exclude_from_git_status(working_dir, &published_targets);
    published
}

/// Whether it is safe to `create_dir_all(skill_root)` and create symlinks
/// under it, given that `skill_root` is meant to sit somewhere under
/// `working_dir`.
///
/// Refuses (after logging why) when:
/// - `working_dir` cannot be canonicalized, so nothing here can be reasoned
///   about safely;
/// - `working_dir` canonicalizes to the filesystem root, or to the user's
///   home directory — either one makes "publish under `working_dir`"
///   dramatically broader than "publish into this one repo checkout", which
///   is the only case this module is meant to handle;
/// - or the deepest *existing* ancestor of `skill_root` (starting at
///   `skill_root` itself and walking up) is a symlink whose canonicalized
///   destination lies outside `working_dir` — meaning one of `.claude`,
///   `.claude/skills`, `.agents`, or `.agents/skills` was already a symlink
///   planted by something else, and creating our own entries underneath it
///   would write through to wherever it points.
fn skill_root_is_safe_to_publish_into(skill_root: &Path, working_dir: &Path) -> bool {
    let Ok(canonical_working_dir) = working_dir.canonicalize() else {
        safe_warn!(
            safe: ("WARP_SKILL_DIRS publish: skipping a skill root because its working directory could not be resolved"),
            full: (
                "WARP_SKILL_DIRS publish: skipping {} — working directory {} could not be canonicalized",
                skill_root.display(), working_dir.display()
            )
        );
        return false;
    };
    if canonical_working_dir.parent().is_none() {
        safe_warn!(
            safe: ("WARP_SKILL_DIRS publish: refusing to publish into the filesystem root"),
            full: (
                "WARP_SKILL_DIRS publish: refusing to publish under {} — working directory {} is the filesystem root",
                skill_root.display(), canonical_working_dir.display()
            )
        );
        return false;
    }
    if dirs::home_dir()
        .and_then(|home| home.canonicalize().ok())
        .is_some_and(|home| home == canonical_working_dir)
    {
        safe_warn!(
            safe: ("WARP_SKILL_DIRS publish: refusing to publish into the user's home directory"),
            full: (
                "WARP_SKILL_DIRS publish: refusing to publish under {} — working directory {} is the user's home directory",
                skill_root.display(), canonical_working_dir.display()
            )
        );
        return false;
    }

    // Walk up from `skill_root` to the first entry that actually exists —
    // `skill_root` itself when it (or a symlink there) already exists, or
    // otherwise the nearest existing ancestor (`.claude`, then working_dir,
    // ...). `working_dir` is guaranteed to exist (it was just canonicalized
    // above), so this always finds something before running out of
    // ancestors.
    let Some(existing_ancestor) = skill_root
        .ancestors()
        .find(|ancestor| fs::symlink_metadata(ancestor).is_ok())
    else {
        return false;
    };
    let Ok(canonical_ancestor) = existing_ancestor.canonicalize() else {
        return false;
    };
    if !canonical_ancestor.starts_with(&canonical_working_dir) {
        safe_warn!(
            safe: ("WARP_SKILL_DIRS publish: refusing to publish through a symlinked parent directory"),
            full: (
                "WARP_SKILL_DIRS publish: refusing to publish under {} — its existing ancestor {} resolves to {}, outside working directory {}",
                skill_root.display(), existing_ancestor.display(), canonical_ancestor.display(), canonical_working_dir.display()
            )
        );
        return false;
    }
    true
}

/// Whether a publish target is already ours, missing, or occupied by
/// something foreign to us.
enum TargetOutcome {
    Missing,
    /// A symlink whose canonical destination is exactly `source_dir` — this
    /// target is already correctly published; nothing to do.
    Ours,
    /// A real file or directory, or a symlink pointing somewhere else (or
    /// nowhere, if broken) — a genuine conflict with something that
    /// predates this publish pass.
    Foreign,
}

/// Classify the filesystem entry, if any, at `target` relative to `source_dir`.
///
/// A working directory is not guaranteed to be fresh for every publish pass —
/// a dormant harness session can wake for a follow-up and re-publish into a
/// working directory it already used — so a repeat publish must recognize its
/// own earlier symlink by where it actually points, not merely by the fact
/// that *something* is a symlink there. Canonicalizing both sides makes the
/// comparison robust to a relative link, a `..` segment, or a symlinked
/// parent directory. A symlink whose destination no longer resolves (broken)
/// is treated as foreign, not ours.
///
/// Uses [`fs::symlink_metadata`] (`lstat`), never [`fs::metadata`], so a
/// symlink is classified by its own type without following it — only
/// [`points_at_our_source`]'s `canonicalize` call resolves through the link,
/// and only to compare a path, never to open or read the target's content.
fn inspect_target(target: &Path, source_dir: &Path) -> Result<TargetOutcome> {
    match fs::symlink_metadata(target) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            if points_at_our_source(target, source_dir) {
                Ok(TargetOutcome::Ours)
            } else {
                Ok(TargetOutcome::Foreign)
            }
        }
        Ok(_) => Ok(TargetOutcome::Foreign),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(TargetOutcome::Missing),
        Err(err) => {
            Err(anyhow::Error::from(err).context(format!("failed to inspect {}", target.display())))
        }
    }
}

/// Whether the symlink at `existing_symlink` resolves to the exact same real
/// path as `source_dir`. Returns `false` (not ours) for a broken symlink, or
/// if either path fails to canonicalize.
fn points_at_our_source(existing_symlink: &Path, source_dir: &Path) -> bool {
    let Ok(canonical_existing) = existing_symlink.canonicalize() else {
        return false;
    };
    let Ok(canonical_source) = source_dir.canonicalize() else {
        return false;
    };
    canonical_existing == canonical_source
}

fn create_symlink_at(source_dir: &Path, target: &Path) -> Result<Option<PathBuf>> {
    create_symlink(source_dir, target).with_context(|| {
        format!(
            "failed to symlink {} -> {}",
            target.display(),
            source_dir.display()
        )
    })?;
    Ok(Some(target.to_path_buf()))
}

/// Publish a single skill folder as `<skill_root>/<skill_name>`, symlinked to
/// `source_dir`. Returns the published symlink path, or `None` when the skill
/// was deliberately not published (its alternate name also conflicted) —
/// that is not an error.
///
/// A target that is already a symlink resolving to `source_dir` is left
/// exactly as it is — a harmless no-op, correct whether this is the first
/// publish or a repeat pass into a reused working directory (see the module
/// docs). Anything else at the target is a genuine conflict: this fork never
/// modifies an entry that predates it (see "Never rename aside" in the module
/// docs) — the conflicting entry is left completely untouched, and the skill
/// is instead published as `<skill_root>/warp-<skill_name>`, subject to the
/// same ownership check. If that alternate name is itself occupied by
/// something foreign, the skill is not published under either name.
///
/// Every conflict is logged via `safe_warn!` for later debugging — there is
/// no user-facing channel for this today.
pub(super) fn publish_skill(
    skill_root: &Path,
    skill_name: &str,
    source_dir: &Path,
) -> Result<Option<PathBuf>> {
    if !source_dir.join("SKILL.md").is_file() {
        anyhow::bail!(
            "source skill directory {} has no SKILL.md",
            source_dir.display()
        );
    }
    fs::create_dir_all(skill_root)
        .with_context(|| format!("failed to create skill root {}", skill_root.display()))?;
    let target = skill_root.join(skill_name);

    match inspect_target(&target, source_dir)? {
        TargetOutcome::Missing => return create_symlink_at(source_dir, &target),
        TargetOutcome::Ours => return Ok(Some(target)),
        TargetOutcome::Foreign => {}
    }

    // A foreign entry occupies `skill_name` — something that predates this
    // publish pass and isn't already our own symlink to this source. Never
    // modify it; try the `warp-<name>` alternate name instead.
    let alt_name = format!("{ALTERNATE_NAME_PREFIX}{skill_name}");
    let alt_target = skill_root.join(&alt_name);
    match inspect_target(&alt_target, source_dir)? {
        TargetOutcome::Foreign => {
            safe_warn!(
                safe: ("WARP_SKILL_DIRS publish: a skill conflict also collided under its alternate name; the skill was not published"),
                full: (
                    "WARP_SKILL_DIRS publish: skill '{skill_name}' conflicts with an existing entry at {} (left untouched); the alternate name {} is also occupied, so the skill from {} was not published under either name",
                    target.display(), alt_target.display(), source_dir.display()
                )
            );
            Ok(None)
        }
        TargetOutcome::Ours => {
            // Already correctly published as the alternate name from an
            // earlier pass into this same working directory — a no-op.
            Ok(Some(alt_target))
        }
        TargetOutcome::Missing => {
            safe_warn!(
                safe: ("WARP_SKILL_DIRS publish: a skill conflicted with an existing entry; the original was left as-is and the skill was published under an alternate name"),
                full: (
                    "WARP_SKILL_DIRS publish: skill '{skill_name}' conflicts with an existing entry at {} (left untouched); published {} as {} instead",
                    target.display(), source_dir.display(), alt_target.display()
                )
            );
            create_symlink_at(source_dir, &alt_target)
        }
    }
}

/// Add `published_targets` (each an absolute path this pass just published or
/// confirmed as already ours, e.g. `<working_dir>/.claude/skills/<name>`) to
/// the repository's `.git/info/exclude`, so a `WARP_SKILL_DIRS`-configured run
/// does not leave harness-owned symlinks showing up in `git status`.
///
/// Best-effort: any failure (no repository found, the exclude file not
/// writable, ...) is logged and swallowed rather than propagated, so it never
/// fails an otherwise-successful skill publish. See
/// [`try_exclude_from_git_status`] for the actual logic.
fn exclude_from_git_status(working_dir: &Path, published_targets: &[PathBuf]) {
    if published_targets.is_empty() {
        return;
    }
    if let Err(err) = try_exclude_from_git_status(working_dir, published_targets) {
        safe_warn!(
            safe: ("WARP_SKILL_DIRS publish: failed to keep published skill links out of git status"),
            full: (
                "WARP_SKILL_DIRS publish: failed to add published skill link(s) to .git/info/exclude for {}: {err:#}",
                working_dir.display()
            )
        );
    }
}

/// Only when `working_dir` sits inside a non-bare git repository whose `.git`
/// is a real directory: idempotently append each of `published_targets`,
/// expressed relative to the repository's top level and anchored with a
/// leading `/`, to `.git/info/exclude`. Never touches `.gitignore` or any
/// tracked file.
///
/// A `.git` that is a *file* rather than a directory marks a linked worktree
/// checkout (the file is a one-line pointer to the real gitdir elsewhere);
/// this function deliberately does not resolve that pointer and treats it the
/// same as "no repository found" — skip, touching nothing. See the module
/// docs.
fn try_exclude_from_git_status(working_dir: &Path, published_targets: &[PathBuf]) -> Result<()> {
    let Some(repo_root) = find_git_worktree_root(working_dir)? else {
        return Ok(());
    };
    let mut patterns = Vec::with_capacity(published_targets.len());
    for target in published_targets {
        let Ok(relative) = target.strip_prefix(&repo_root) else {
            continue;
        };
        let relative = relative.to_string_lossy().replace('\\', "/");
        patterns.push(format!("/{relative}"));
    }
    if patterns.is_empty() {
        return Ok(());
    }

    let exclude_path = repo_root.join(".git").join("info").join("exclude");
    let existing = fs::read_to_string(&exclude_path).unwrap_or_default();
    let existing_lines: HashSet<&str> = existing.lines().collect();
    let mut to_append = String::new();
    for pattern in &patterns {
        if !existing_lines.contains(pattern.as_str()) {
            to_append.push_str(pattern);
            to_append.push('\n');
        }
    }
    if to_append.is_empty() {
        return Ok(());
    }

    if let Some(parent) = exclude_path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)
        .with_context(|| format!("failed to open {}", exclude_path.display()))?;
    if !existing.is_empty() && !existing.ends_with('\n') {
        file.write_all(b"\n")?;
    }
    file.write_all(to_append.as_bytes())?;
    Ok(())
}

/// Find the top of the git working tree containing `dir`, by walking up from
/// `dir` looking for the first `.git` entry. Returns `Ok(None)` when none is
/// found before running out of ancestors, or when the first `.git` found is
/// not a real directory (a linked worktree checkout's `.git` file, or a
/// symlink — see [`try_exclude_from_git_status`]).
fn find_git_worktree_root(dir: &Path) -> Result<Option<PathBuf>> {
    for ancestor in dir.ancestors() {
        let git_path = ancestor.join(".git");
        match fs::symlink_metadata(&git_path) {
            Ok(metadata) if metadata.is_dir() => return Ok(Some(ancestor.to_path_buf())),
            Ok(_) => return Ok(None),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => continue,
            Err(err) => {
                return Err(anyhow::Error::from(err)
                    .context(format!("failed to inspect {}", git_path.display())));
            }
        }
    }
    Ok(None)
}

#[cfg(unix)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(source, target)
}

#[cfg(windows)]
fn create_symlink(source: &Path, target: &Path) -> std::io::Result<()> {
    std::os::windows::fs::symlink_dir(source, target)
}

#[cfg(test)]
#[path = "skill_dirs_publish_tests.rs"]
mod tests;
