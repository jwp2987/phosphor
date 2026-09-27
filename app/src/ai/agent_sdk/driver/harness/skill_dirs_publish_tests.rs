use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::*;

/// Write a minimal skill folder named `name` under `dir` and return its path.
fn write_skill(dir: &Path, name: &str) -> PathBuf {
    let skill_dir = dir.join(name);
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        format!("---\nname: {name}\ndescription: test skill\n---\nBody"),
    )
    .unwrap();
    skill_dir
}

#[test]
fn publish_skill_creates_symlink() {
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");

    let target = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();

    assert_eq!(target, skill_root.path().join("github"));
    let metadata = fs::symlink_metadata(&target).unwrap();
    assert!(metadata.file_type().is_symlink());
    assert_eq!(fs::read_link(&target).unwrap(), skill_dir);
    // The symlink resolves through to the real skill content.
    assert!(target.join("SKILL.md").is_file());
}

#[test]
fn publish_skill_uses_the_real_skill_name() {
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "linear");

    publish_skill(skill_root.path(), "linear", &skill_dir)
        .unwrap()
        .unwrap();

    // Published under the skill's own name, not some namespaced alias, so an
    // agent prompt or another skill can still reference it by name.
    assert!(skill_root.path().join("linear").exists());
}

#[test]
fn publish_skill_is_a_noop_when_the_target_already_points_at_our_source() {
    // A repeat publish pass into a working directory that already has the
    // correct symlink (e.g. a dormant harness session waking for a
    // follow-up and re-publishing into the same working directory) must
    // recognize the target as already ours by comparing where it actually
    // points, not merely by the fact that something is a symlink there.
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");

    publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();
    let published_again = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();

    let target = skill_root.path().join("github");
    assert_eq!(published_again, target);
    assert_eq!(fs::read_link(&target).unwrap(), skill_dir);
    // No alternate name was ever created for a clean no-op.
    assert!(!skill_root.path().join("warp-github").exists());
}

#[test]
fn publish_skill_leaves_a_foreign_symlink_untouched_and_uses_an_alternate_name() {
    // A symlink at the target that points somewhere other than the source
    // we're about to publish is not ours — it's foreign, exactly like a real
    // directory would be. This fork never renames or replaces a conflicting
    // entry (see the module docs' "Never rename aside" section — a
    // Phosphor-specific divergence from upstream, which does so in a
    // detected sandbox): the original is left completely untouched and the
    // skill is published under the `warp-` alternate name instead.
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let old_skill_dir = write_skill(source_root.path(), "old-github");
    let new_skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    create_symlink(&old_skill_dir, &target).unwrap();

    let published = publish_skill(skill_root.path(), "github", &new_skill_dir)
        .unwrap()
        .unwrap();

    // The foreign symlink at the real name is completely untouched.
    assert_eq!(fs::read_link(&target).unwrap(), old_skill_dir);
    let alt_target = skill_root.path().join("warp-github");
    assert_eq!(published, alt_target);
    assert_eq!(fs::read_link(&alt_target).unwrap(), new_skill_dir);
}

#[test]
fn publish_skill_does_not_publish_when_the_alternate_name_is_a_foreign_symlink() {
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let unrelated_skill_dir = write_skill(source_root.path(), "unrelated");
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    let alt_target = skill_root.path().join("warp-github");
    create_symlink(&unrelated_skill_dir, &target).unwrap();
    create_symlink(&unrelated_skill_dir, &alt_target).unwrap();

    let published = publish_skill(skill_root.path(), "github", &skill_dir).unwrap();

    // Never fall back to replacing: nothing was published under either name,
    // and both foreign symlinks are completely untouched.
    assert_eq!(published, None);
    assert_eq!(fs::read_link(&target).unwrap(), unrelated_skill_dir);
    assert_eq!(fs::read_link(&alt_target).unwrap(), unrelated_skill_dir);
}

#[test]
fn publish_skill_is_a_noop_when_the_alternate_name_already_points_at_our_source() {
    // A second pass into the same working directory: the real name still has
    // its original conflicting entry, but the alternate name was already
    // correctly published by an earlier pass. That's a clean no-op, not a
    // fresh conflict.
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    let alt_target = skill_root.path().join("warp-github");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("real-file.txt"), "do not touch me").unwrap();
    create_symlink(&skill_dir, &alt_target).unwrap();

    let published = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();

    assert_eq!(published, alt_target);
    assert_eq!(fs::read_link(&alt_target).unwrap(), skill_dir);
    // The real conflicting directory at the real name is still untouched.
    assert_eq!(
        fs::read_to_string(target.join("real-file.txt")).unwrap(),
        "do not touch me"
    );
}

#[test]
fn publish_skill_leaves_a_conflicting_real_directory_untouched_and_uses_an_alternate_name() {
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("real-file.txt"), "do not touch me").unwrap();

    let published = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();

    // The real, pre-existing directory at the real name is completely untouched:
    // still a real directory (not a symlink), with its original content, and no
    // alternate-name entry was created anywhere but the intended one.
    assert!(
        !fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(target.join("real-file.txt")).unwrap(),
        "do not touch me"
    );
    // The skill was published under the `warp-` alternate name instead.
    let alt_target = skill_root.path().join("warp-github");
    assert_eq!(published, alt_target);
    assert_eq!(fs::read_link(&alt_target).unwrap(), skill_dir);
}

#[test]
fn publish_skill_does_not_publish_when_the_alternate_name_also_conflicts() {
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    let alt_target = skill_root.path().join("warp-github");
    fs::create_dir_all(&target).unwrap();
    fs::write(target.join("real-file.txt"), "do not touch me").unwrap();
    fs::create_dir_all(&alt_target).unwrap();
    fs::write(
        alt_target.join("other-real-file.txt"),
        "do not touch me either",
    )
    .unwrap();

    let published = publish_skill(skill_root.path(), "github", &skill_dir).unwrap();

    // Never fall back to replacing: nothing was published under either name.
    assert_eq!(published, None);
    // Both pre-existing real entries are completely untouched.
    assert!(
        !fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(target.join("real-file.txt")).unwrap(),
        "do not touch me"
    );
    assert!(
        !fs::symlink_metadata(&alt_target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(alt_target.join("other-real-file.txt")).unwrap(),
        "do not touch me either"
    );
}

#[test]
fn publish_skill_errors_on_missing_source() {
    let skill_root = TempDir::new().unwrap();
    let missing_source = skill_root.path().join("does-not-exist");

    let result = publish_skill(skill_root.path(), "github", &missing_source);

    assert!(result.is_err());
    assert!(!skill_root.path().join("github").exists());
}

#[test]
fn publish_skill_dirs_prefers_most_specific_directory_on_name_collision() {
    let root = TempDir::new().unwrap();
    let specific_dir = root.path().join("agents/triage/skills");
    let general_dir = root.path().join("skills");
    fs::create_dir_all(&specific_dir).unwrap();
    fs::create_dir_all(&general_dir).unwrap();
    let specific_github = write_skill(&specific_dir, "github");
    write_skill(&general_dir, "github");
    let general_linear = write_skill(&general_dir, "linear");
    let skill_root = TempDir::new().unwrap();

    let published = publish_skill_dirs(
        skill_root.path(),
        &[specific_dir, general_dir],
        skill_root.path(),
    );

    assert_eq!(published, 2);
    assert_eq!(
        fs::read_link(skill_root.path().join("github")).unwrap(),
        specific_github
    );
    assert_eq!(
        fs::read_link(skill_root.path().join("linear")).unwrap(),
        general_linear
    );
}

#[test]
fn publish_skill_dirs_leaves_a_pre_existing_environment_skill_untouched_and_uses_an_alternate_name()
{
    // This is the scenario upstream's sandbox branch used to handle by
    // renaming the pre-existing entry aside and taking over its name. This
    // fork never does that (see the module docs' "Never rename aside"
    // section) — the pre-existing skill is preserved exactly where it is,
    // untouched, and the published skill goes out under the alternate name.
    let root = TempDir::new().unwrap();
    let source_dir = root.path().join("skills");
    fs::create_dir_all(&source_dir).unwrap();
    let published_github = write_skill(&source_dir, "github");
    let skill_root = TempDir::new().unwrap();
    // Simulate a real, pre-existing "github" skill already installed in the
    // harness's environment before this publish runs.
    let existing_target = skill_root.path().join("github");
    fs::create_dir_all(&existing_target).unwrap();
    fs::write(existing_target.join("SKILL.md"), "pre-existing skill").unwrap();

    let published = publish_skill_dirs(skill_root.path(), &[source_dir], skill_root.path());

    assert_eq!(published, 1);
    // The pre-existing skill at the real name is completely untouched...
    assert!(
        !fs::symlink_metadata(&existing_target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(existing_target.join("SKILL.md")).unwrap(),
        "pre-existing skill"
    );
    // ...and the published skill went out under the alternate name.
    assert_eq!(
        fs::read_link(skill_root.path().join("warp-github")).unwrap(),
        published_github
    );
}

#[test]
fn publish_skill_dirs_skips_entries_without_skill_md() {
    let root = TempDir::new().unwrap();
    let source_dir = root.path().join("skills");
    fs::create_dir_all(source_dir.join("not-a-skill")).unwrap();
    write_skill(&source_dir, "github");
    let skill_root = TempDir::new().unwrap();

    let published = publish_skill_dirs(skill_root.path(), &[source_dir], skill_root.path());

    assert_eq!(published, 1);
    assert!(skill_root.path().join("github").exists());
    assert!(!skill_root.path().join("not-a-skill").exists());
}

#[test]
fn publish_skill_dirs_is_a_noop_for_empty_source_dirs() {
    let outer = TempDir::new().unwrap();
    let skill_root = outer.path().join("skills");

    assert_eq!(publish_skill_dirs(&skill_root, &[], outer.path()), 0);
    // Doesn't even create the skill root when there's nothing to publish.
    assert!(!skill_root.exists());
}

#[test]
fn publish_skill_dirs_recovers_from_missing_source_directory() {
    let root = TempDir::new().unwrap();
    let missing_dir = root.path().join("does-not-exist/skills");
    let present_dir = root.path().join("skills");
    fs::create_dir_all(&present_dir).unwrap();
    write_skill(&present_dir, "github");
    let skill_root = TempDir::new().unwrap();

    let published = publish_skill_dirs(
        skill_root.path(),
        &[missing_dir, present_dir],
        skill_root.path(),
    );

    assert_eq!(published, 1);
    assert!(skill_root.path().join("github").exists());
}

// --- Phosphor-specific: symlink-safety cases on top of the ported suite ---
//
// These target the specific properties `AGENTS.md` requires we not regress:
// the identity check must be robust to path representation (not just exact
// string equality), a dangling symlink must never be silently treated as
// "already ours", and a foreign symlink is never touched — reclaiming its
// name the way upstream's sandbox branch used to is exactly the divergence
// `DECLINED.md`'s `IMPROVED` entry records.

#[test]
fn publish_skill_dangling_symlink_target_is_treated_as_foreign() {
    // A symlink whose destination no longer exists must not be mistaken for
    // "ours" just because canonicalizing our own source also fails to match
    // it — `points_at_our_source` fails closed (returns `false`, i.e.
    // foreign) whenever either side can't be canonicalized, so a dangling
    // link is always a genuine conflict, never a silent no-op. It is left
    // completely alone, and the skill is published under the alternate name
    // instead.
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");
    let nonexistent = source_root.path().join("this-does-not-exist");
    create_symlink(&nonexistent, &target).unwrap();

    let published = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();
    assert!(
        fs::symlink_metadata(&target)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(fs::read_link(&target).unwrap(), nonexistent);
    let alt_target = skill_root.path().join("warp-github");
    assert_eq!(published, alt_target);
    assert_eq!(fs::read_link(&alt_target).unwrap(), skill_dir);
}

#[test]
fn publish_skill_is_a_noop_when_target_symlink_uses_relative_dotdot_path_to_our_source() {
    // The ownership check must compare canonicalized paths, not raw link
    // text, so a functionally-identical symlink written with a different
    // (but resolving-to-the-same-place) path representation is still
    // recognized as ours and left alone — not treated as a foreign conflict
    // that gets shadowed under an alternate name.
    let source_root = TempDir::new().unwrap();
    let skill_root = TempDir::new().unwrap();
    let skill_dir = write_skill(source_root.path(), "github");
    let target = skill_root.path().join("github");

    // Build a relative path from skill_root back down to skill_dir via `..`,
    // rather than pointing at it directly, and confirm it still canonicalizes
    // to the same place before relying on that in the assertion below.
    let relative_via_dotdot = Path::new("..")
        .join(source_root.path().file_name().unwrap())
        .join("github");
    create_symlink(&relative_via_dotdot, &target).unwrap();
    assert_eq!(
        target.canonicalize().unwrap(),
        skill_dir.canonicalize().unwrap()
    );

    let published = publish_skill(skill_root.path(), "github", &skill_dir)
        .unwrap()
        .unwrap();

    assert_eq!(published, target);
    // Untouched: still the original relative-dotdot link, not replaced with
    // our own absolute one, and no alternate name was created.
    assert_eq!(fs::read_link(&target).unwrap(), relative_via_dotdot);
    assert!(!skill_root.path().join("warp-github").exists());
}

#[test]
fn publish_skill_never_publishes_a_source_directory_missing_skill_md_even_via_symlinked_source() {
    // `publish_skill` bails on a source that has no `SKILL.md` at the
    // checked (post-canonicalization-irrelevant, pre-symlink) path — this
    // guards the "symlinks we create must point only at directories this app
    // itself resolved as skill directories" property: a caller cannot get a
    // symlink created into an arbitrary directory just because that
    // directory happens to be reachable through another symlink, if it does
    // not look like a skill directory.
    let root = TempDir::new().unwrap();
    let not_a_skill_dir = root.path().join("not-a-skill");
    fs::create_dir_all(&not_a_skill_dir).unwrap();
    fs::write(not_a_skill_dir.join("secret.txt"), "not a skill").unwrap();
    let skill_root = TempDir::new().unwrap();

    let result = publish_skill(skill_root.path(), "github", &not_a_skill_dir);

    assert!(result.is_err());
    assert!(!skill_root.path().join("github").exists());
}

// --- Phosphor-specific: symlinked-parent and working-directory safety (#705) ---
//
// `publish_skill_dirs` refuses to touch the filesystem at all when the path
// leading to `skill_root` isn't provably inside `working_dir`, or when
// `working_dir` itself is unreasonably broad. See
// `skill_root_is_safe_to_publish_into` and the module docs' "Safety note".

#[test]
fn publish_skill_dirs_refuses_when_a_skill_root_ancestor_is_a_symlink_to_outside_working_dir() {
    let working_dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");

    // `.claude` itself is a symlink pointing outside `working_dir`.
    create_symlink(outside.path(), &working_dir.path().join(".claude")).unwrap();
    let skill_root = working_dir.path().join(".claude").join("skills");

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        working_dir.path(),
    );

    assert_eq!(published, 0);
    // Nothing was ever written on the far side of the symlink.
    assert!(!outside.path().join("skills").exists());
    // The symlink itself is untouched.
    assert_eq!(
        fs::read_link(working_dir.path().join(".claude")).unwrap(),
        outside.path()
    );
}

#[test]
fn publish_skill_dirs_refuses_when_skill_root_itself_is_a_symlink_to_outside_working_dir() {
    let working_dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");

    fs::create_dir_all(working_dir.path().join(".claude")).unwrap();
    let skill_root = working_dir.path().join(".claude").join("skills");
    create_symlink(outside.path(), &skill_root).unwrap();

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        working_dir.path(),
    );

    assert_eq!(published, 0);
    assert!(!outside.path().join("github").exists());
    // The symlink itself is untouched.
    assert_eq!(fs::read_link(&skill_root).unwrap(), outside.path());
}

#[test]
fn publish_skill_dirs_publishes_normally_when_no_ancestor_is_symlinked() {
    let working_dir = TempDir::new().unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = working_dir.path().join(".claude").join("skills");

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        working_dir.path(),
    );

    assert_eq!(published, 1);
    assert!(skill_root.join("github").exists());
}

#[test]
fn publish_skill_dirs_refuses_when_working_dir_is_the_filesystem_root() {
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    // This never gets far enough to touch the filesystem under `skill_root`:
    // the filesystem-root check happens before any ancestor is inspected or
    // created.
    let skill_root = Path::new("/this-warp-skill-dirs-test-path-must-not-exist/.claude/skills");

    let published = publish_skill_dirs(
        skill_root,
        &[source_root.path().to_path_buf()],
        Path::new("/"),
    );

    assert_eq!(published, 0);
    assert!(!skill_root.exists());
}

#[test]
#[serial_test::serial]
fn publish_skill_dirs_refuses_when_working_dir_is_the_home_directory() {
    let home_dir = TempDir::new().unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let old_home = std::env::var_os("HOME");
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::set_var("HOME", home_dir.path()) };

    let skill_root = home_dir.path().join(".claude").join("skills");
    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        home_dir.path(),
    );

    match old_home {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(home) => unsafe { std::env::set_var("HOME", home) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var("HOME") },
    }

    assert_eq!(published, 0);
    assert!(!skill_root.exists());
}

// --- Phosphor-specific: `.git/info/exclude` git-status hygiene (#705) ---

#[test]
fn publish_skill_dirs_adds_published_links_to_git_info_exclude() {
    let repo_root = TempDir::new().unwrap();
    fs::create_dir_all(repo_root.path().join(".git")).unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = repo_root.path().join(".claude").join("skills");

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        repo_root.path(),
    );

    assert_eq!(published, 1);
    let exclude =
        fs::read_to_string(repo_root.path().join(".git").join("info").join("exclude")).unwrap();
    assert!(exclude.lines().any(|line| line == "/.claude/skills/github"));
}

#[test]
fn publish_skill_dirs_does_not_duplicate_git_info_exclude_entries_on_repeat_publish() {
    let repo_root = TempDir::new().unwrap();
    fs::create_dir_all(repo_root.path().join(".git")).unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = repo_root.path().join(".claude").join("skills");
    let source_dirs = [source_root.path().to_path_buf()];

    publish_skill_dirs(&skill_root, &source_dirs, repo_root.path());
    publish_skill_dirs(&skill_root, &source_dirs, repo_root.path());

    let exclude =
        fs::read_to_string(repo_root.path().join(".git").join("info").join("exclude")).unwrap();
    let occurrences = exclude
        .lines()
        .filter(|line| *line == "/.claude/skills/github")
        .count();
    assert_eq!(occurrences, 1);
}

#[test]
fn publish_skill_dirs_never_touches_gitignore_or_tracked_files() {
    let repo_root = TempDir::new().unwrap();
    fs::create_dir_all(repo_root.path().join(".git")).unwrap();
    fs::write(repo_root.path().join(".gitignore"), "target/\n").unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = repo_root.path().join(".claude").join("skills");

    publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        repo_root.path(),
    );

    assert_eq!(
        fs::read_to_string(repo_root.path().join(".gitignore")).unwrap(),
        "target/\n"
    );
}

#[test]
fn publish_skill_dirs_does_not_write_git_info_exclude_for_a_worktree_git_file() {
    // A `.git` *file* (rather than a directory) marks a linked worktree
    // checkout — its content is a one-line pointer to the real gitdir
    // elsewhere. This is deliberately not resolved (see the module docs);
    // the publish still succeeds, it just doesn't get the git-status
    // convenience.
    let repo_root = TempDir::new().unwrap();
    fs::write(
        repo_root.path().join(".git"),
        "gitdir: /elsewhere/.git/worktrees/example\n",
    )
    .unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = repo_root.path().join(".claude").join("skills");

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        repo_root.path(),
    );

    assert_eq!(published, 1);
    assert!(skill_root.join("github").exists());
    // `.git` is still the same one-line file; nothing was created under it.
    assert_eq!(
        fs::read_to_string(repo_root.path().join(".git")).unwrap(),
        "gitdir: /elsewhere/.git/worktrees/example\n"
    );
}

#[test]
fn publish_skill_dirs_publishes_normally_outside_any_git_repo() {
    let working_dir = TempDir::new().unwrap();
    let source_root = TempDir::new().unwrap();
    write_skill(source_root.path(), "github");
    let skill_root = working_dir.path().join(".claude").join("skills");

    let published = publish_skill_dirs(
        &skill_root,
        &[source_root.path().to_path_buf()],
        working_dir.path(),
    );

    assert_eq!(published, 1);
    assert!(skill_root.join("github").exists());
    assert!(!working_dir.path().join(".git").exists());
}

// --- `warp_skill_source_dirs` (#705) ---

#[test]
#[serial_test::serial]
fn warp_skill_source_dirs_resolves_relative_entries_against_working_dir() {
    let working_dir = TempDir::new().unwrap();
    let old = std::env::var_os(ai::skills::WARP_SKILL_DIRS_ENV);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe {
        std::env::set_var(
            ai::skills::WARP_SKILL_DIRS_ENV,
            "relative-skills,/abs/skills",
        )
    };

    let dirs = warp_skill_source_dirs(working_dir.path());

    match old {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(v) => unsafe { std::env::set_var(ai::skills::WARP_SKILL_DIRS_ENV, v) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var(ai::skills::WARP_SKILL_DIRS_ENV) },
    }

    assert_eq!(
        dirs,
        vec![
            working_dir.path().join("relative-skills"),
            PathBuf::from("/abs/skills"),
        ]
    );
}

#[test]
#[serial_test::serial]
fn warp_skill_source_dirs_is_empty_when_env_var_is_unset() {
    let working_dir = TempDir::new().unwrap();
    let old = std::env::var_os(ai::skills::WARP_SKILL_DIRS_ENV);
    // TODO: Audit that the environment access only happens in single-threaded code.
    unsafe { std::env::remove_var(ai::skills::WARP_SKILL_DIRS_ENV) };

    let dirs = warp_skill_source_dirs(working_dir.path());

    match old {
        // TODO: Audit that the environment access only happens in single-threaded code.
        Some(v) => unsafe { std::env::set_var(ai::skills::WARP_SKILL_DIRS_ENV, v) },
        // TODO: Audit that the environment access only happens in single-threaded code.
        None => unsafe { std::env::remove_var(ai::skills::WARP_SKILL_DIRS_ENV) },
    }

    assert!(dirs.is_empty());
}
