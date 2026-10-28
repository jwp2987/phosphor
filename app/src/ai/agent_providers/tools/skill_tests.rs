//! `read_skill` name binding against the skills listed for the turn (#1068).
//!
//! Ported by content from mehmetbaykar/zap `f4ef1318b` (itself from InfiniShell `fff26aa85`):
//! the model's name binds to an exact reference from the turn's `<available_skills>` list, or
//! the call is rejected. Nothing outside the list is consulted.

use std::path::PathBuf;

use ai::skills::{
    skill_reference_from_read_skill_ref, SkillPathOrigin, SkillProvider, SkillReference,
    SkillScope,
};
use warp_multi_agent_api as api;
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;
use warp_util::remote_path::RemotePath;
use warp_util::standardized_path::StandardizedPath;

use super::*;
use crate::ai::skills::SkillDescriptor;

const LISTED_PATH: &str = "/home/user/repo-b/.agents/skills/deploy/SKILL.md";
const OTHER_REPO_PATH: &str = "/home/user/repo-a/.agents/skills/deploy/SKILL.md";
const HOME_PATH: &str = "/home/user/.agents/skills/deploy/SKILL.md";
const REMOTE_PATH: &str = "/home/remote/.agents/skills/deploy/SKILL.md";

fn descriptor(name: &str, reference: SkillReference) -> SkillDescriptor {
    SkillDescriptor {
        reference,
        name: name.to_owned(),
        description: "test skill".to_owned(),
        scope: SkillScope::Project,
        provider: SkillProvider::Agents,
        icon_override: None,
    }
}

fn local(name: &str, path: &str) -> SkillDescriptor {
    descriptor(
        name,
        SkillReference::Path(LocalOrRemotePath::Local(PathBuf::from(path))),
    )
}

fn remote(name: &str, host: &str, path: &str) -> SkillDescriptor {
    descriptor(
        name,
        SkillReference::Path(LocalOrRemotePath::Remote(RemotePath::new(
            HostId::new(host.to_owned()),
            StandardizedPath::try_new(path).expect("absolute remote path"),
        ))),
    )
}

fn bundled(name: &str, id: &str) -> SkillDescriptor {
    SkillDescriptor {
        scope: SkillScope::Bundled,
        provider: SkillProvider::Zap,
        ..descriptor(name, SkillReference::BundledSkillId(id.to_owned()))
    }
}

fn remote_origin(host: &str) -> SkillPathOrigin {
    SkillPathOrigin::Remote {
        host_id: HostId::new(host.to_owned()),
    }
}

/// What the model sent, as `from_args` parses it: the name, and no reference yet.
fn read_skill_call(name: &str) -> api::message::tool_call::ReadSkill {
    let args = serde_json::json!({ "name": name }).to_string();
    match from_args(&args).expect("from_args accepts a name") {
        api::message::tool_call::Tool::ReadSkill(read_skill) => read_skill,
        other => panic!("expected ReadSkill, got {other:?}"),
    }
}

fn bound_path(read_skill: &api::message::tool_call::ReadSkill) -> &str {
    match &read_skill.skill_reference {
        Some(ApiSkillReference::SkillPath(path)) => path,
        other => panic!("expected a bound SkillPath, got {other:?}"),
    }
}

#[test]
fn from_args_keeps_the_name_and_binds_no_reference() {
    // The old shape stuffed the name into the `SkillPath` slot, which is what let the
    // executor treat any bare name as a lookup over the whole index.
    let read_skill = read_skill_call("deploy");
    assert_eq!(read_skill.name, "deploy");
    assert_eq!(read_skill.skill_reference, None);
}

#[test]
fn listed_skill_binds_to_its_exact_path() {
    let skills = [local("deploy", LISTED_PATH), local("review", HOME_PATH)];
    let mut read_skill = read_skill_call("deploy");
    resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Local).expect("listed");
    assert_eq!(bound_path(&read_skill), LISTED_PATH);
    // The model's own argument is left as it sent it, for history replay.
    assert_eq!(read_skill.name, "deploy");
}

/// The defect: a same-named SKILL.md outside this turn's list (another repository the
/// `SkillManager` happens to have indexed) used to be read by name. With the list as the only
/// source there is nothing for it to match.
#[test]
fn same_named_skill_outside_the_list_is_not_bound() {
    let skills = [local("review", HOME_PATH)];
    let mut read_skill = read_skill_call("deploy");
    assert_eq!(
        resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Local),
        Err(SkillResolutionError::NotAvailable)
    );
    assert_eq!(read_skill.skill_reference, None);

    // Naming the unlisted copy by its full path does not get around the list either.
    let mut by_path = read_skill_call(OTHER_REPO_PATH);
    assert_eq!(
        resolve_reference(&mut by_path, &skills, &SkillPathOrigin::Local),
        Err(SkillResolutionError::NotAvailable)
    );
    assert_eq!(by_path.skill_reference, None);
}

#[test]
fn nothing_binds_when_no_skills_are_listed() {
    let mut read_skill = read_skill_call("deploy");
    assert_eq!(
        resolve_reference(&mut read_skill, &[], &SkillPathOrigin::Local),
        Err(SkillResolutionError::NotAvailable)
    );
}

#[test]
fn colliding_listed_names_are_ambiguous_until_named_by_path() {
    let skills = [local("deploy", HOME_PATH), local("deploy", LISTED_PATH)];

    let mut read_skill = read_skill_call("deploy");
    assert_eq!(
        resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Local),
        Err(SkillResolutionError::Ambiguous)
    );
    assert_eq!(read_skill.skill_reference, None);

    let mut by_path = read_skill_call(LISTED_PATH);
    resolve_reference(&mut by_path, &skills, &SkillPathOrigin::Local).expect("exact path");
    assert_eq!(bound_path(&by_path), LISTED_PATH);
}

#[test]
fn the_same_reference_listed_twice_is_not_ambiguous() {
    let skills = [local("deploy", LISTED_PATH), local("deploy", LISTED_PATH)];
    let mut read_skill = read_skill_call("deploy");
    resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Local).expect("one skill");
    assert_eq!(bound_path(&read_skill), LISTED_PATH);
}

#[test]
fn listed_bundled_skill_binds_by_name_or_bundled_id() {
    let skills = [bundled("find-skills", "find-skills-id")];
    for name in ["find-skills", "@warp-skill:find-skills-id"] {
        let mut read_skill = read_skill_call(name);
        resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Local).expect(name);
        assert_eq!(
            read_skill.skill_reference,
            Some(ApiSkillReference::BundledSkillId("find-skills-id".to_owned())),
            "{name}"
        );
    }
}

/// A remote (SSH) session lists the host's skills by remote path. The name binds to that
/// path, and the conversion the controller runs next (`convert_read_skill`, with the session's
/// origin) restores the same host-qualified reference the executor then reads from that
/// host's catalog.
#[test]
fn remote_skill_binds_for_its_own_host_and_keeps_host_identity() {
    let skills = [remote("deploy", "host-a", REMOTE_PATH)];
    let origin = remote_origin("host-a");

    let mut read_skill = read_skill_call("deploy");
    resolve_reference(&mut read_skill, &skills, &origin).expect("listed for this host");
    assert_eq!(bound_path(&read_skill), REMOTE_PATH);

    let reference = skill_reference_from_read_skill_ref(
        read_skill.skill_reference.clone().expect("bound"),
        &origin,
    )
    .expect("a bound remote path converts");
    assert_eq!(reference, skills[0].reference);
}

#[test]
fn skills_from_another_host_never_bind() {
    // A remote skill does not bind for a different host, nor in a local session.
    let remote_skills = [remote("deploy", "host-a", REMOTE_PATH)];
    for origin in [remote_origin("host-b"), SkillPathOrigin::Local] {
        let mut read_skill = read_skill_call("deploy");
        assert_eq!(
            resolve_reference(&mut read_skill, &remote_skills, &origin),
            Err(SkillResolutionError::NotAvailable),
            "{origin:?}"
        );
    }

    // And a client-local skill does not bind inside a remote session.
    let local_skills = [local("deploy", LISTED_PATH)];
    let mut read_skill = read_skill_call("deploy");
    assert_eq!(
        resolve_reference(&mut read_skill, &local_skills, &remote_origin("host-a")),
        Err(SkillResolutionError::NotAvailable)
    );
}

/// A session whose host identity is unknown (`Unavailable`, e.g. legacy SSH) cannot place a
/// skill path anywhere: `location_for_path` returns `PathOriginUnavailable`, so a bound local
/// path would only fail later in `convert_read_skill`. It is rejected up front, consistently.
/// A bundled id carries no host and still binds; the executor resolves it per origin.
#[test]
fn path_skills_do_not_bind_when_the_host_identity_is_unavailable() {
    let skills = [local("deploy", LISTED_PATH)];
    let mut read_skill = read_skill_call("deploy");
    assert_eq!(
        resolve_reference(&mut read_skill, &skills, &SkillPathOrigin::Unavailable),
        Err(SkillResolutionError::NotAvailable)
    );
    assert!(!is_listed_for_origin(&skills[0], &SkillPathOrigin::Unavailable));
    // The same path could not have been converted either — the reason for the rule.
    assert!(
        skill_reference_from_read_skill_ref(
            ApiSkillReference::SkillPath(LISTED_PATH.to_owned()),
            &SkillPathOrigin::Unavailable,
        )
        .is_err()
    );

    let bundled_skills = [bundled("find-skills", "find-skills-id")];
    let mut read_bundled = read_skill_call("find-skills");
    resolve_reference(
        &mut read_bundled,
        &bundled_skills,
        &SkillPathOrigin::Unavailable,
    )
    .expect("a bundled id has no host");
}

/// A bundled skill sharing its name with a file skill is ambiguous by name, and reachable by
/// the `@warp-skill:<id>` shown as its `skill_path` — the form `listed_skill_path` renders.
#[test]
fn bundled_skill_sharing_a_file_skills_name_is_reachable_by_its_skill_path() {
    let skills = [
        local("find-skills", LISTED_PATH),
        bundled("find-skills", "find-skills-id"),
    ];

    let mut by_name = read_skill_call("find-skills");
    assert_eq!(
        resolve_reference(&mut by_name, &skills, &SkillPathOrigin::Local),
        Err(SkillResolutionError::Ambiguous)
    );

    let shown = listed_skill_path(&skills[1].reference);
    assert_eq!(shown, "@warp-skill:find-skills-id");
    let mut by_path = read_skill_call(&shown);
    resolve_reference(&mut by_path, &skills, &SkillPathOrigin::Local).expect("bundled path");
    assert_eq!(
        by_path.skill_reference,
        Some(ApiSkillReference::BundledSkillId("find-skills-id".to_owned()))
    );

    assert_eq!(listed_skill_path(&skills[0].reference), LISTED_PATH);
}

#[test]
fn resolution_errors_carry_distinct_codes() {
    assert_eq!(SkillResolutionError::NotAvailable.code(), "skill_unavailable");
    assert_eq!(SkillResolutionError::Ambiguous.code(), "ambiguous_skill_name");
}
