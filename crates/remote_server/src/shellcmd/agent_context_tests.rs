use super::*;

fn raw(path: &str, bytes: &[u8]) -> RawFile {
    RawFile {
        path: path.into(),
        bytes: bytes.to_vec(),
    }
}

fn section(output: &[u8]) -> ParsedSection {
    ParsedSection {
        name: "skills".into(),
        output: output.to_vec(),
        exit: 0,
    }
}

#[test]
fn records_round_trip_including_content_with_nuls_and_newlines() {
    let mut out = Vec::new();
    for (path, body) in [("/h/a b/SKILL.md", &b"x\0y\n"[..]), ("/h/c/SKILL.md", b"")] {
        out.extend_from_slice(path.as_bytes());
        out.push(0);
        out.extend_from_slice(body.len().to_string().as_bytes());
        out.push(0);
        out.extend_from_slice(body);
    }
    let files = parse_records(&section(&out));
    assert_eq!(
        files,
        vec![raw("/h/a b/SKILL.md", b"x\0y\n"), raw("/h/c/SKILL.md", b"")]
    );
}

#[test]
fn a_record_cut_short_ends_the_list_without_panicking() {
    let out = b"/h/a/SKILL.md\x005\x00abc".to_vec();
    assert!(parse_records(&section(&out)).is_empty());
    assert!(parse_records(&section(b"/h/a/SKILL.md\x00zz\x00abc")).is_empty());
    assert!(parse_records(&section(b"")).is_empty());
}

#[test]
fn assemble_sorts_drops_non_utf8_and_oversized_files_and_marks_home_skills() {
    let big = vec![b'a'; MAX_FILE_BYTES + 1];
    let snapshot = assemble(
        "/h",
        vec![
            raw("/h/.claude/skills/b/SKILL.md", b"B"),
            raw("/h/.agents/skills/a/SKILL.md", b"A"),
            raw("/h/.agents/skills/bad/SKILL.md", &[0xff, 0xfe]),
            raw("/h/.agents/skills/huge/SKILL.md", &big),
        ],
        vec![raw("/h/.agents/AGENTS.md", b"rules")],
        7,
    );
    assert_eq!(snapshot.revision, 7);
    assert_eq!(snapshot.home_dir, "/h");
    let paths: Vec<_> = snapshot.skills.iter().map(|s| s.path.as_str()).collect();
    assert_eq!(
        paths,
        vec![
            "/h/.agents/skills/a/SKILL.md",
            "/h/.claude/skills/b/SKILL.md"
        ]
    );
    assert!(
        snapshot
            .skills
            .iter()
            .all(|s| matches!(s.source, Some(remote_skill_proto::Source::Home(_))))
    );
    assert_eq!(snapshot.global_rules.len(), 1);
    assert_eq!(snapshot.global_rules[0].content, "rules");
}

#[test]
fn assemble_stops_at_the_total_byte_budget() {
    let body = vec![b'a'; MAX_FILE_BYTES];
    let files: Vec<RawFile> = (0..20)
        .map(|i| raw(&format!("/h/.agents/skills/s{i:02}/SKILL.md"), &body))
        .collect();
    let snapshot = assemble("/h", files, Vec::new(), 1);
    assert_eq!(snapshot.skills.len(), MAX_TOTAL_BYTES / MAX_FILE_BYTES);
}

#[test]
fn same_context_ignores_only_the_revision() {
    let a = assemble(
        "/h",
        vec![raw("/h/.agents/skills/a/SKILL.md", b"A")],
        vec![],
        1,
    );
    let mut b = a.clone();
    b.revision = 9;
    assert!(same_context(&a, &b));
    b.skills[0].content = "other".into();
    assert!(!same_context(&a, &b));
}

#[test]
fn relevant_changes_are_the_skill_roots_and_the_rule_file_only() {
    let h = "/home/u";
    for yes in [
        "/home/u/.agents",
        "/home/u/.claude",
        "/home/u/.claude/skills",
        "/home/u/.claude/skills/x/SKILL.md",
        "/home/u/.agents/AGENTS.md",
    ] {
        assert!(is_relevant_change(h, yes), "{yes}");
    }
    for no in [
        "/home/u/.bash_history",
        "/home/u/.claude/history.jsonl",
        "/home/u/.agents/other.md",
        "/home/uu/.agents",
        "/elsewhere/.agents",
        "/home/u",
    ] {
        assert!(!is_relevant_change(h, no), "{no}");
    }
}

#[test]
fn commands_exist_for_posix_family_shells_and_not_for_powershell() {
    for kind in [ShellKind::Posix, ShellKind::Fish, ShellKind::Nu] {
        let skills = skills_command(kind).unwrap();
        assert!(skills.contains(".agents/skills") && skills.contains("SKILL.md"));
        assert!(rules_command(kind).unwrap().contains(".agents/AGENTS.md"));
        assert!(home_command(kind).is_some());
    }
    assert!(skills_command(ShellKind::Pwsh).is_none());
    assert!(rules_command(ShellKind::Pwsh).is_none());
    assert!(home_command(ShellKind::Pwsh).is_none());
}

fn child(name: &str, is_dir: bool) -> TreeChild {
    TreeChild {
        name: name.into(),
        is_dir,
        ignored: false,
    }
}

#[test]
fn project_skills_are_skill_files_one_level_under_a_provider_skills_directory() {
    let mut dirs = BTreeMap::new();
    dirs.insert("/r".to_owned(), vec![child(".agents", true)]);
    dirs.insert(
        "/r/.agents/skills/a".to_owned(),
        vec![child("SKILL.md", false)],
    );
    dirs.insert(
        "/r/pkg/.claude/skills/b".to_owned(),
        vec![child("SKILL.md", false), child("notes.md", false)],
    );
    // Not under a provider directory, no SKILL.md, or a directory called SKILL.md.
    dirs.insert("/r/docs/c".to_owned(), vec![child("SKILL.md", false)]);
    dirs.insert(
        "/r/.agents/skills/d".to_owned(),
        vec![child("README.md", false)],
    );
    dirs.insert(
        "/r/.agents/skills/e".to_owned(),
        vec![child("SKILL.md", true)],
    );
    assert_eq!(
        project_skill_files(&dirs).into_iter().collect::<Vec<_>>(),
        vec![
            "/r/.agents/skills/a/SKILL.md".to_owned(),
            "/r/pkg/.claude/skills/b/SKILL.md".to_owned()
        ]
    );
}

#[test]
fn the_standing_delta_reports_added_and_removed_skills_and_nothing_when_equal() {
    let a: BTreeSet<String> = ["/r/.agents/skills/a/SKILL.md".to_owned()].into();
    let b: BTreeSet<String> = ["/r/.agents/skills/b/SKILL.md".to_owned()].into();
    assert!(standing_delta(&a, &a).is_none());
    assert!(standing_snapshot(&BTreeSet::new()).is_none());
    let delta = standing_delta(&a, &b).unwrap();
    assert_eq!(
        delta.upserted_project_skills[0].path,
        "/r/.agents/skills/b/SKILL.md"
    );
    assert_eq!(
        delta.removed_project_skills[0].path,
        "/r/.agents/skills/a/SKILL.md"
    );
    assert_eq!(
        standing_snapshot(&b).unwrap().upserted_project_skills.len(),
        1
    );
}
