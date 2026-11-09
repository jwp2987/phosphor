use super::*;

#[test]
fn resolve_names_the_canonical_repo_and_the_root_with_the_override() {
    let cmd = resolve_command(false, "/home/me/it's");
    assert!(
        cmd.starts_with("cd '/home/me/it'\\''s' 2>/dev/null || exit 3"),
        "{cmd}"
    );
    assert!(cmd.contains("pwd -P"), "{cmd}");
    assert!(
        cmd.contains("${PHOSPHOR_REMOTE_INDEX_DIR:-$HOME/.phosphor/index}"),
        "{cmd}"
    );
    let pwsh = resolve_command(true, "C:\\r");
    assert!(pwsh.contains("PHOSPHOR_REMOTE_INDEX_DIR"), "{pwsh}");
    assert_eq!(
        parse_resolve("banner\nREPO:/real/repo\nROOT:/home/u/.phosphor/index\n"),
        Some(("/real/repo".into(), "/home/u/.phosphor/index".into()))
    );
    assert_eq!(parse_resolve("REPO:/x\n"), None);
}

#[test]
fn stat_reports_present_files_and_skips_absent_ones() {
    let cmd = stat_command(false, "/d", &["meta.json", "vectors-ab.bin"]);
    assert!(cmd.contains("wc -c"), "{cmd}");
    assert!(cmd.contains("stat -c %Y"), "{cmd}");
    let parsed = parse_stat("meta.json 120 1700000000\nvectors-ab.bin -\nvectors-cd.bin 7 5\r\n");
    assert_eq!(
        parsed.get("meta.json"),
        Some(&FileStat {
            size: 120,
            mtime: 1_700_000_000
        })
    );
    assert_eq!(parsed.get("vectors-ab.bin"), None);
    assert_eq!(
        parsed.get("vectors-cd.bin"),
        Some(&FileStat { size: 7, mtime: 5 })
    );
}

#[test]
fn put_writes_stdin_through_a_temp_file_and_a_rename() {
    let cmd = put_command(false, "/h/.phosphor/index/ab", "meta.json", "n0nce");
    assert!(cmd.contains("cat > \"$t\""), "{cmd}");
    assert!(cmd.contains("mv -f -- \"$t\" \"$d\"/'meta.json'"), "{cmd}");
    assert!(cmd.contains(".tmp-n0nce"), "{cmd}");
    assert!(
        !cmd.contains("base64"),
        "the payload is stdin, not an argument: {cmd}"
    );
    let pwsh = put_command(true, "C:\\i\\ab", "meta.json", "n0nce");
    assert!(pwsh.contains("OpenStandardInput"), "{pwsh}");
}

#[test]
fn append_cuts_a_torn_tail_and_refuses_a_shorter_file() {
    let cmd = append_command(false, "/d", "vectors-ab.bin", 4096);
    assert!(cmd.contains("-lt 4096"), "{cmd}");
    assert!(cmd.contains("seek=4096"), "{cmd}");
    assert!(cmd.contains("cat >> \"$f\""), "{cmd}");
    assert!(append_command(true, "C:\\d", "v.bin", 9).contains("SetLength(9)"));
}

#[test]
fn stale_vector_files_are_dropped_except_the_kept_one() {
    let cmd = drop_other_vectors_command(false, "/d", "vectors-keep.bin");
    assert!(cmd.contains("vectors-*.bin"), "{cmd}");
    assert!(cmd.contains("!= 'vectors-keep.bin'"), "{cmd}");
}
