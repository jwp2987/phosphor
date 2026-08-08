use std::path::{Path, PathBuf};
use std::str::FromStr;

use ai::skills::{get_provider_for_path, ParsedSkill, SkillProvider, SkillScope};
use warp_cli::skill::SkillSpec;

use super::filter_skills_by_spec;

// Ported from the pin's `global_skills_tests.rs` (`02b53fcd8`). Only the
// `filter_skills_by_spec` cases that don't touch `GithubRepo` (cloud, dropped —
// see `global_skills.rs`'s module doc comment) or a remote host (`RemotePath`/
// `HostId`, out of scope per issue #487's remote-arm decision) survive:
//
// - `resolve_skill_repos_*` (5 tests) — dropped with `resolve_skill_repos` itself.
// - `filter_skills_by_spec_matches_full_path_specs_for_remote_repos` and
//   `filter_skills_by_spec_scopes_simple_remote_names_to_the_repo_host` — dropped,
//   remote-host scoping.

#[test]
fn filter_skills_by_spec_only_loads_requested_simple_names() {
    let repo_path = std::env::temp_dir().join("work").join("warp-internal");
    let requested_skill_path = skill_path(&repo_path, ".agents", "read-google-doc");
    let other_skill_path = skill_path(&repo_path, ".agents", "deploy");
    let skills = vec![
        parsed_skill(requested_skill_path.clone(), "read-google-doc"),
        parsed_skill(other_skill_path, "deploy"),
    ];
    let specs = global_specs(&["warpdotdev/warp-internal:read-google-doc".to_string()]);

    let filtered = filter_skills_by_spec(&repo_path, skills, &specs);

    assert_eq!(skill_paths(filtered), vec![requested_skill_path]);
}

#[test]
fn filter_skills_by_spec_matches_simple_names_by_parsed_skill_name() {
    let repo_path = std::env::temp_dir().join("work").join("warp-internal");
    let requested_skill_path = skill_path(&repo_path, ".agents", "google-doc");
    let directory_name_match_path = skill_path(&repo_path, ".agents", "read-google-doc");
    let skills = vec![
        parsed_skill(requested_skill_path.clone(), "read-google-doc"),
        parsed_skill(directory_name_match_path, "unrelated-skill"),
    ];
    let specs = global_specs(&["warpdotdev/warp-internal:read-google-doc".to_string()]);
    let filtered = filter_skills_by_spec(&repo_path, skills, &specs);

    assert_eq!(skill_paths(filtered), vec![requested_skill_path]);
}

#[test]
fn filter_skills_by_spec_uses_provider_precedence_for_simple_names() {
    let repo_path = std::env::temp_dir().join("work").join("warp-internal");
    let agents_skill_path = skill_path(&repo_path, ".agents", "deploy");
    let claude_skill_path = skill_path(&repo_path, ".claude", "deploy");
    let skills = vec![
        parsed_skill(claude_skill_path, "deploy"),
        parsed_skill(agents_skill_path.clone(), "deploy"),
    ];
    let specs = global_specs(&["warpdotdev/warp-internal:deploy".to_string()]);
    let filtered = filter_skills_by_spec(&repo_path, skills, &specs);

    assert_eq!(skill_paths(filtered), vec![agents_skill_path]);
}

#[test]
fn filter_skills_by_spec_matches_full_path_specs() {
    let repo_path = std::env::temp_dir().join("work").join("warp-internal");
    let requested_relative_path = PathBuf::from(".claude")
        .join("skills")
        .join("deploy")
        .join("SKILL.md");
    let requested_skill_path = repo_path.join(&requested_relative_path);
    let other_skill_path = skill_path(&repo_path, ".agents", "deploy");
    let skills = vec![
        parsed_skill(other_skill_path, "deploy"),
        parsed_skill(requested_skill_path.clone(), "deploy-from-full-path"),
    ];
    let specs = global_specs(&[format!(
        "warpdotdev/warp-internal:{}",
        requested_relative_path.display()
    )]);
    let filtered = filter_skills_by_spec(&repo_path, skills, &specs);

    assert_eq!(skill_paths(filtered), vec![requested_skill_path]);
}

fn global_specs(raw_specs: &[String]) -> Vec<SkillSpec> {
    raw_specs
        .iter()
        .map(|raw| SkillSpec::from_str(raw).unwrap())
        .collect()
}

fn skill_path(repo_path: &Path, provider_dir: &str, skill_name: &str) -> PathBuf {
    repo_path
        .join(provider_dir)
        .join("skills")
        .join(skill_name)
        .join("SKILL.md")
}

fn parsed_skill(path: PathBuf, name: &str) -> ParsedSkill {
    let provider = get_provider_for_path(&path).unwrap_or(SkillProvider::Agents);
    ParsedSkill {
        path,
        name: name.to_string(),
        description: String::new(),
        content: String::new(),
        line_range: None,
        provider,
        scope: SkillScope::Project,
    }
}

fn skill_paths(skills: Vec<ParsedSkill>) -> Vec<PathBuf> {
    skills.into_iter().map(|skill| skill.path).collect()
}
