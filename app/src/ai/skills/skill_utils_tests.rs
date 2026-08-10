use super::*;
use ai::skills::{ParsedSkill, SkillProvider, SkillScope};
use std::path::PathBuf;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;

fn local(path: PathBuf) -> LocalOrRemotePath {
    LocalOrRemotePath::Local(path)
}

fn remote_location(path: &str) -> LocalOrRemotePath {
    LocalOrRemotePath::Remote(RemotePath::new(
        HostId::new("remote-host".to_string()),
        StandardizedPath::try_new(path).unwrap(),
    ))
}

#[test]
fn test_skill_path_from_file_path_skill_md() {
    let skill = PathBuf::from("/home/user/.claude/skills/my-skill/SKILL.md");
    let result = skill_path_from_file_path(&skill);
    assert_eq!(
        result,
        Some(PathBuf::from("/home/user/.claude/skills/my-skill/SKILL.md"))
    );
}

#[test]
fn test_skill_path_from_file_path_warp_home_skill() {
    let Some(warp_home_skills_dir) = warp_core::paths::warp_home_skills_dir() else {
        eprintln!("Skipping test: Zap home skills directory not available");
        return;
    };
    let warp_home_skill = warp_home_skills_dir
        .join("my-skill")
        .join("assets")
        .join("image.png");
    let result = skill_path_from_file_path(&warp_home_skill);
    assert_eq!(
        result,
        Some(warp_home_skills_dir.join("my-skill").join("SKILL.md"))
    );
}

#[test]
fn test_skill_path_from_file_path_nested_file() {
    let skill_nested = PathBuf::from("/home/user/.agents/skills/my-skill/assets/image.png");
    let result = skill_path_from_file_path(&skill_nested);
    assert_eq!(
        result,
        Some(PathBuf::from("/home/user/.agents/skills/my-skill/SKILL.md"))
    );
}

#[test]
fn test_skill_path_from_file_path_non_skill() {
    let non_skill = PathBuf::from("/home/user/Documents/file.txt");
    let result = skill_path_from_file_path(&non_skill);
    assert_eq!(result, None);

    let similar_path = PathBuf::from("/home/user/.claude/other/file.txt");
    let result = skill_path_from_file_path(&similar_path);
    assert_eq!(result, None);

    let empty_path = PathBuf::from("");
    let result = skill_path_from_file_path(&empty_path);
    assert_eq!(result, None);
}

#[test]
fn test_unique_skills_dedupes_identical_skills_same_dir() {
    let shared_skill_dir = PathBuf::from("/home/user");
    let skill_path1 = shared_skill_dir.join(".agents/skills/my-skill/SKILL.md");
    let skill_path2 = shared_skill_dir.join(".claude/skills/my-skill/SKILL.md");

    let content = "---\nname: test-skill\ndescription: A test skill\n---\nContent here";
    let skill = ParsedSkill {
        path: local(skill_path1.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let skill2 = ParsedSkill {
        path: local(skill_path2.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Claude,
        scope: SkillScope::Project,
    };

    let mut skills_by_path = HashMap::new();
    skills_by_path.insert(local(skill_path1.clone()), skill);
    skills_by_path.insert(local(skill_path2.clone()), skill2);

    let skill_paths = vec![
        (local(shared_skill_dir.clone()), local(skill_path1)),
        (local(shared_skill_dir), local(skill_path2)),
    ];

    let result = unique_skills(&skill_paths, &skills_by_path);
    assert_eq!(result.len(), 1);
    // Agents has higher priority (index 0) than Claude, so it should be preferred
    assert_eq!(result[0].provider, SkillProvider::Agents);
}

#[test]
fn test_unique_skills_keeps_same_provider_skills_from_different_dirs() {
    let home_dir = PathBuf::from("/home/user");
    let project_dir = PathBuf::from("/home/user/projects/repo");
    let home_path = home_dir.join(".agents/skills/my-skill/SKILL.md");
    let project_path = project_dir.join(".agents/skills/my-skill/SKILL.md");

    let content = "---\nname: test-skill\ndescription: A test skill\n---\nContent here";
    let home_skill = ParsedSkill {
        path: local(home_path.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let project_skill = ParsedSkill {
        path: local(project_path.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let mut skills_by_path = HashMap::new();
    skills_by_path.insert(local(home_path.clone()), home_skill);
    skills_by_path.insert(local(project_path.clone()), project_skill);

    let skill_paths = vec![
        (local(home_dir), local(home_path.clone())),
        (local(project_dir), local(project_path)),
    ];

    let result = unique_skills(&skill_paths, &skills_by_path);
    assert_eq!(result.len(), 2, "Same name + same provider across different directories should each be kept");
    assert!(
        result
            .iter()
            .any(|skill| skill.reference.to_string().contains("/home/user/.agents")),
        "Should keep the same-name skill in the home directory, actual={result:?}"
    );
    assert!(
        result.iter().any(|skill| skill
            .reference
            .to_string()
            .contains("/home/user/projects/repo/.agents")),
        "Should keep the same-name skill in the project directory, actual={result:?}"
    );
}

#[test]
fn test_unique_skills_name_dedup_same_name_different_providers() {
    let shared_skill_dir = PathBuf::from("/home/user");
    let skill_path1 = shared_skill_dir.join(".agents/skills/my-skill/SKILL.md");
    let skill_path2 = shared_skill_dir.join(".claude/skills/my-skill/SKILL.md");

    let content1 = "---\nname: test-skill\ndescription: A test skill\n---\nContent here";
    let content2 = "---\nname: test-skill\ndescription: A test skill\n---\nDifferent content";

    let skill1 = ParsedSkill {
        path: local(skill_path1.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content1.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Agents,
        scope: SkillScope::Project,
    };

    let skill2 = ParsedSkill {
        path: local(skill_path2.clone()),
        name: "test-skill".to_string(),
        description: "A test skill".to_string(),
        content: content2.to_string(),
        line_range: Some(8..18),
        provider: SkillProvider::Claude,
        scope: SkillScope::Project,
    };

    let mut skills_by_path = HashMap::new();
    skills_by_path.insert(local(skill_path1.clone()), skill1);
    skills_by_path.insert(local(skill_path2.clone()), skill2);

    let skill_paths = vec![
        (local(shared_skill_dir.clone()), local(skill_path1)),
        (local(shared_skill_dir), local(skill_path2)),
    ];

    let result = unique_skills(&skill_paths, &skills_by_path);
    assert_eq!(
        result.len(),
        1,
        "Same name, different content, different provider should be name-deduped, keeping only the highest-priority provider"
    );
    assert_eq!(
        result[0].provider,
        SkillProvider::Agents,
        "name-dedup should keep the higher-priority provider (Agents > Claude)"
    );
}

// ── Ported from the pinned oracle (02b53fcd8),
// `app/src/ai/skills/skill_utils_tests.rs` ──
//
// `skill_path_from_location` walks ancestors to find the SKILL.md that owns an
// arbitrary file inside a skill directory; it is what turns a tool call touching
// `.../skills/deploy/scripts/run.sh` into a clickable skill button. Only the
// local `PathBuf` form was covered here. These two pin the remote form, which is
// the one that can silently break: the walk must keep the location's host and
// respect the path's own encoding rather than the build host's.

#[test]
fn skill_path_from_unix_encoded_remote_location() {
    let location = remote_location("/repo/.agents/skills/deploy/scripts/run.sh");

    assert_eq!(
        skill_path_from_location(&location),
        Some(remote_location("/repo/.agents/skills/deploy/SKILL.md"))
    );
}

#[test]
fn skill_path_from_windows_encoded_remote_location() {
    let location = remote_location(r"C:\repo\.agents\skills\deploy\scripts\run.ps1");

    assert_eq!(
        skill_path_from_location(&location),
        Some(remote_location(r"C:\repo\.agents\skills\deploy\SKILL.md"))
    );
}
