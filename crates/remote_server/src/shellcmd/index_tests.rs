use super::*;
use crate::shellcmd::Script;
use crate::shellcmd::ShellKind;

const A: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
const B: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

#[test]
fn the_scan_command_quotes_the_repository_and_names_every_tool() {
    let cmd = scan_command("/home/me/it's here");
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
                scan_command("/r"),
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
