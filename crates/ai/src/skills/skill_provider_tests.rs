use super::{
    get_provider_for_path, get_scope_for_path, home_skills_path, SkillProvider, SkillScope,
};

#[test]
fn warp_home_skills_path_uses_warp_home_path() {
    assert_eq!(
        home_skills_path(SkillProvider::Zap),
        warp_core::paths::warp_home_skills_dir()
    );
}

#[test]
fn warp_home_skill_path_uses_warp_provider() {
    let Some(warp_home_skills_dir) = warp_core::paths::warp_home_skills_dir() else {
        eprintln!("Skipping test: home directory not available");
        return;
    };
    let path = warp_home_skills_dir.join("my-skill").join("SKILL.md");

    assert_eq!(get_provider_for_path(&path), Some(SkillProvider::Zap));
}

#[test]
fn home_skill_path_is_home_scope() {
    let Some(warp_home_skills_dir) = warp_core::paths::warp_home_skills_dir() else {
        eprintln!("Skipping test: home directory not available");
        return;
    };
    let path = warp_home_skills_dir.join("my-skill").join("SKILL.md");

    // Regression guard: home-directory skills must be Home scope, not
    // Project (which would render a misleading "Project Skill" badge).
    assert_eq!(get_scope_for_path(&path), SkillScope::Home);
}

#[test]
fn project_skill_path_is_project_scope() {
    let path = std::env::temp_dir()
        .join("repo")
        .join(".claude")
        .join("skills")
        .join("my-skill")
        .join("SKILL.md");

    assert_eq!(get_scope_for_path(&path), SkillScope::Project);
}

#[test]
fn local_project_provider_path_is_classified_by_structure() {
    let path = std::env::temp_dir()
        .join("repo")
        .join(".claude")
        .join("skills")
        .join("my-skill")
        .join("SKILL.md");

    assert_eq!(get_provider_for_path(&path), Some(SkillProvider::Claude));
}
