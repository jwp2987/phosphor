use super::{
    get_provider_for_path, get_scope_for_path, home_skills_path, SkillProvider, SkillScope,
};

// The pinned oracle (`02b53fcd8`) has 6 tests here; 3 are covered below (its
// `warp_home_skill_path_is_home_warp_skill` is split into the provider and scope
// halves, plus `project_skill_path_is_project_scope` which the pin does not
// have). The other 3 are blocked on the `LocalOrRemotePath` migration that
// `crates/ai/src/skills` never got — `get_provider_for_path` here still takes a
// `&Path`, so a remote skill path cannot even be expressed:
//
//   - `remote_provider_path_is_classified_by_structure`
//   - `foreign_encoded_remote_provider_path_is_classified_by_structure`
//   - `foreign_encoded_remote_skills_root_resolves_provider_parent_directory`
//     (also needs `provider_parent_directory_for_skills_root`, absent here)
//
// Non-cloud, and the missing host-awareness is a latent correctness bug, not
// just missing coverage. Tracked as #205; refs #150 item 1, #170.

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
