use super::*;

fn script() -> Script {
    Script::new("abc123")
        .unwrap()
        .section("head", "git rev-parse HEAD", StderrMode::Discard)
        .unwrap()
        .section("list", "ls -1", StderrMode::Merge)
        .unwrap()
}

#[test]
fn posix_rendering_is_golden() {
    let expected = "\
printf '%s\\n' '@@PHOSPHOR-abc123 BEGIN head'
(
git rev-parse HEAD
) 2>/dev/null
__phosphor_e=$?
printf '\\n%s exit=%s\\n' '@@PHOSPHOR-abc123 END head' \"$__phosphor_e\"
printf '%s\\n' '@@PHOSPHOR-abc123 BEGIN list'
(
ls -1
) 2>&1
__phosphor_e=$?
printf '\\n%s exit=%s\\n' '@@PHOSPHOR-abc123 END list' \"$__phosphor_e\"
";
    assert_eq!(script().render_posix(), expected);
    assert_eq!(script().render(ShellKind::Posix), expected);
}

#[test]
fn non_posix_login_shells_get_sh_c() {
    let fish = script().render(ShellKind::Fish);
    assert!(fish.starts_with("sh -c '") && fish.ends_with('\''));
    let nu = script().render(ShellKind::Nu);
    assert!(nu.starts_with("^sh -c r#'") && nu.ends_with("'#"));
    let pwsh = script().render(ShellKind::Pwsh);
    assert!(pwsh.starts_with("pwsh -NoProfile -NonInteractive -EncodedCommand "));
}

#[test]
fn pwsh_rendering_marks_each_section() {
    let p = script().render_pwsh();
    assert!(p.contains("Write-Output '@@PHOSPHOR-abc123 BEGIN head'"));
    assert!(p.contains("Write-Output ('@@PHOSPHOR-abc123 END list exit=' + $__e)"));
    assert!(p.contains("}2>$null") || p.contains("} 2>$null") || p.contains("}\u{20}2>$null"));
}

#[test]
fn invalid_names_and_nonces_are_rejected() {
    assert!(Script::new("").is_err());
    assert!(Script::new("a b").is_err());
    assert!(Script::new("a-b").is_err());
    let s = Script::new("n").unwrap();
    assert!(s.clone().section("", "x", StderrMode::Merge).is_err());
    assert!(s.clone().section("a b", "x", StderrMode::Merge).is_err());
    assert!(s.clone().section("a'b", "x", StderrMode::Merge).is_err());
    assert!(s.section("ok-name.1_x", "x", StderrMode::Merge).is_ok());
}

fn envelope(nonce: &str, name: &str, body: &str, exit: i32) -> String {
    format!("@@PHOSPHOR-{nonce} BEGIN {name}\n{body}\n@@PHOSPHOR-{nonce} END {name} exit={exit}\n")
}

#[test]
fn parses_two_sections_and_their_exit_codes() {
    let out = format!(
        "motd banner\n{}{}trailing\n",
        envelope("n1", "a", "line1\nline2", 0),
        envelope("n1", "b", "", 3)
    );
    let f = Frame::parse(out.as_bytes(), "n1").unwrap();
    assert_eq!(f.sections.len(), 2);
    let a = f.section("a").unwrap();
    assert_eq!(a.output, b"line1\nline2");
    assert!(a.success());
    let b = f.section("b").unwrap();
    assert_eq!(b.output, b"");
    assert_eq!(b.exit, 3);
    assert!(!b.success());
}

#[test]
fn output_without_a_trailing_newline_is_exact() {
    // The script prints a newline before END, so "x" with no newline arrives as "x\n<END>" and
    // "x\n" arrives as "x\n\n<END>"; the parser removes exactly one.
    let f = Frame::parse(envelope("n", "a", "x\n", 0).as_bytes(), "n").unwrap();
    assert_eq!(f.section("a").unwrap().output, b"x\n");
}

#[test]
fn crlf_line_ends_are_tolerated() {
    let out = "@@PHOSPHOR-n BEGIN a\r\nhello\r\n@@PHOSPHOR-n END a exit=0\r\n";
    let f = Frame::parse(out.as_bytes(), "n").unwrap();
    assert_eq!(f.section("a").unwrap().output, b"hello");
}

#[test]
fn a_marker_with_the_wrong_nonce_is_plain_output() {
    let body = "@@PHOSPHOR-other END a exit=9";
    let f = Frame::parse(envelope("n", "a", body, 0).as_bytes(), "n").unwrap();
    assert_eq!(f.section("a").unwrap().output, body.as_bytes());
}

#[test]
fn marker_text_in_the_middle_of_a_line_is_plain_output() {
    let body = "x @@PHOSPHOR-n END a exit=9";
    let f = Frame::parse(envelope("n", "a", body, 0).as_bytes(), "n").unwrap();
    assert_eq!(f.section("a").unwrap().output, body.as_bytes());
}

#[test]
fn binary_payload_survives() {
    let mut out = b"@@PHOSPHOR-n BEGIN a\n".to_vec();
    out.extend_from_slice(&[0, 255, 1, 2, b'\r', 0xC3]);
    out.extend_from_slice(b"\n@@PHOSPHOR-n END a exit=0\n");
    let f = Frame::parse(&out, "n").unwrap();
    assert_eq!(
        f.section("a").unwrap().output,
        vec![0, 255, 1, 2, b'\r', 0xC3]
    );
}

#[test]
fn failures_are_distinct_errors() {
    assert_eq!(
        Frame::parse(b"ssh: connect refused\n", "n"),
        Err(FrameError::NoFrame)
    );
    assert_eq!(Frame::parse(b"", "n"), Err(FrameError::NoFrame));
    assert_eq!(
        Frame::parse(b"@@PHOSPHOR-n BEGIN a\npartial", "n"),
        Err(FrameError::Truncated("a".into()))
    );
    assert!(matches!(
        Frame::parse(b"@@PHOSPHOR-n END a exit=0\n", "n"),
        Err(FrameError::Malformed(_))
    ));
    assert!(matches!(
        Frame::parse(b"@@PHOSPHOR-n BEGIN a\n@@PHOSPHOR-n BEGIN b\n", "n"),
        Err(FrameError::Malformed(_))
    ));
    assert!(matches!(
        Frame::parse(b"@@PHOSPHOR-n BEGIN a\n\n@@PHOSPHOR-n END b exit=0\n", "n"),
        Err(FrameError::Malformed(_))
    ));
    assert!(matches!(
        Frame::parse(b"@@PHOSPHOR-n BEGIN a\n\n@@PHOSPHOR-n END a exit=zz\n", "n"),
        Err(FrameError::Malformed(_))
    ));
}

#[test]
fn a_section_over_the_cap_is_an_error_not_a_truncation() {
    let body = "x".repeat(11);
    let out = envelope("n", "a", &body, 0);
    assert_eq!(
        Frame::parse_with_cap(out.as_bytes(), "n", 10),
        Err(FrameError::TooLarge {
            section: "a".into(),
            bytes: 11,
            max: 10
        })
    );
    assert!(Frame::parse_with_cap(out.as_bytes(), "n", 11).is_ok());
}
