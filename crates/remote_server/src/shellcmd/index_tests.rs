use super::*;
use crate::shellcmd::Script;
use crate::shellcmd::ShellKind;

const A: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const B: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn the_scan_command_quotes_the_repository_and_names_every_tool() {
    let cmd = scan_page_command("/home/me/it's here", 0, SCAN_PAGE_FILES, true);
    assert!(cmd.starts_with("cd '/home/me/it'\\''s here' || exit 3"));
    for needle in [
        "sha256sum",
        "shasum -a 256",
        "sha256 -r",
        "git ls-files -z --cached --others --exclude-standard",
        "-prune",
        "xargs -0",
        "-size -3000000c",
        "-maxdepth 0",
        "sed -n '1,2000p;2000q'",
        "TOTAL:",
    ] {
        assert!(cmd.contains(needle), "missing {needle}: {cmd}");
    }
}

#[test]
fn the_scan_command_fits_a_framed_script_for_every_shell() {
    for kind in [ShellKind::Posix, ShellKind::Fish, ShellKind::Nu] {
        let script = Script::new("abc123")
            .unwrap()
            .section(
                "scan",
                scan_page_command("/r", 3, 50, false),
                crate::shellcmd::StderrMode::Discard,
            )
            .unwrap();
        assert!(script.render(kind).contains("MODE:"));
    }
}

#[test]
fn gnu_output_is_parsed_in_path_order() {
    let text = format!("MODE:git\n{B}  src/b.rs\n{A}  a.txt\n");
    let (mode, files) = parse_scan_output(&text);
    assert_eq!(mode, Some(ListMode::Git));
    assert_eq!(
        files,
        vec![
            HashedFile {
                path: "a.txt".into(),
                sha256: A.into()
            },
            HashedFile {
                path: "src/b.rs".into(),
                sha256: B.into()
            },
        ]
    );
}

#[test]
fn find_paths_lose_their_dot_slash_and_other_separators_are_read() {
    let text = format!("MODE:find\n{A}  ./x/y.txt\n{B} *bin.dat\n{A} bsd.txt\n");
    let (mode, files) = parse_scan_output(&text);
    assert_eq!(mode, Some(ListMode::Find));
    let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["bin.dat", "bsd.txt", "x/y.txt"]);
}

#[test]
fn escaped_names_stdin_markers_and_garbage_are_skipped() {
    let text = format!(
        "login banner\nMODE:git\n\\{A}  weird\\nname\n{B}  -\nnot a hash  file\n{A}  ok.txt\n\n{A}\n"
    );
    let (_, files) = parse_scan_output(&text);
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].path, "ok.txt");
}

#[test]
fn a_repeated_path_is_listed_once_and_crlf_is_tolerated() {
    let text = format!("MODE:git\r\n{A}  a.txt\r\n{A}  a.txt\r\n");
    let (mode, files) = parse_scan_output(&text);
    assert_eq!(mode, Some(ListMode::Git));
    assert_eq!(files.len(), 1);
}

#[test]
fn reading_a_file_is_capped_quoted_and_encoded() {
    let cmd = read_file_command(Base64Tool::GnuW0, "/r", "dir/we ird'.rs");
    assert_eq!(
        cmd,
        "cd '/r' && head -c 3000000 < 'dir/we ird'\\''.rs' | base64 -w0"
    );
    assert!(read_file_command(Base64Tool::Od, "/r", "a").ends_with("od -An -v -tx1"));
}

#[test]
fn pages_slice_the_listing_and_only_the_first_reports_the_total() {
    let first = scan_page_command("/r", 0, 100, true);
    assert!(first.contains("sed -n '1,100p;100q'"), "{first}");
    assert!(first.contains("echo \"TOTAL:"), "{first}");
    let third = scan_page_command("/r", 2, 100, false);
    assert!(third.contains("sed -n '201,300p;300q'"), "{third}");
    assert!(!third.contains("TOTAL:"), "{third}");
    // The slice goes to the hasher NUL separated again.
    assert!(third.contains("tr '\\n' '\\0' | xargs -0"), "{third}");
}

#[test]
fn the_total_is_read_from_the_first_page() {
    assert_eq!(parse_total("MODE:git\nTOTAL:123456\n"), Some(123_456));
    assert_eq!(parse_total("MODE:git\r\nTOTAL:7\r\n"), Some(7));
    assert_eq!(parse_total("MODE:git\n"), None);
    assert_eq!(parse_total("TOTAL:x\n"), None);
}

#[test]
fn the_pwsh_scan_page_lists_hashes_and_pages_like_the_posix_one() {
    let cmd = scan_page_command_pwsh("C:\\it's", 2, 100, true);
    assert!(
        cmd.contains("Set-Location -LiteralPath 'C:\\it''s'"),
        "{cmd}"
    );
    assert!(cmd.contains("exit 3"), "{cmd}");
    assert!(
        cmd.contains("git ls-files -z --cached --others --exclude-standard"),
        "{cmd}"
    );
    assert!(cmd.contains("Select-Object -Skip 200 -First 100"), "{cmd}");
    assert!(cmd.contains("Get-FileHash"), "{cmd}");
    assert!(cmd.contains("'TOTAL:'"), "{cmd}");
    assert!(!scan_page_command_pwsh("C:\\r", 0, 100, false).contains("'TOTAL:'"));
    assert!(read_file_command_pwsh("C:\\r", "a b.rs").contains("ToBase64String"));
}
