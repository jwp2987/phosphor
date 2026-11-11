use super::*;
use crate::proto::{LineRange, file_context_proto::Content};

fn file(path: &str, ranges: &[(u32, u32)]) -> ReadFileContextFile {
    ReadFileContextFile {
        path: path.to_string(),
        line_ranges: ranges
            .iter()
            .map(|&(start, end)| LineRange { start, end })
            .collect(),
    }
}

fn request(
    files: Vec<ReadFileContextFile>,
    per_file: Option<u32>,
    batch: Option<u32>,
) -> ReadFileContextRequest {
    ReadFileContextRequest {
        files,
        max_file_bytes: per_file,
        max_batch_bytes: batch,
    }
}

fn loaded(bytes: &[u8]) -> FileProbe {
    FileProbe::Loaded(LoadedFile {
        size: bytes.len() as u64,
        mtime_ms: Some(5000),
        bytes: bytes.to_vec(),
        complete: true,
        total_lines: None,
    })
}

fn text_of(c: &FileContextProto) -> &str {
    match c.content.as_ref().unwrap() {
        Content::TextContent(t) => t,
        other => panic!("expected text, got {other:?}"),
    }
}

fn assemble_one(
    path: &str,
    ranges: &[(u32, u32)],
    per_file: Option<u32>,
    probe: FileProbe,
) -> ReadFileContextResponse {
    assemble(
        &request(vec![file(path, ranges)], per_file, None),
        vec![probe],
    )
}

#[test]
fn a_small_text_file_is_returned_whole_with_its_final_newline_and_no_range() {
    let r = assemble_one("/a.txt", &[], None, loaded(b"one\ntwo\n"));
    assert!(r.failed_files.is_empty());
    let c = &r.file_contexts[0];
    assert_eq!(text_of(c), "one\ntwo\n");
    assert_eq!((c.line_range_start, c.line_range_end), (None, None));
    assert_eq!(c.line_count, 2);
    assert_eq!(c.last_modified_epoch_millis, Some(5000));
    assert_eq!(c.file_name, "/a.txt");
}

#[test]
fn crlf_is_normalised_and_a_missing_final_newline_stays_missing() {
    let r = assemble_one("/a.txt", &[], None, loaded(b"one\r\ntwo"));
    assert_eq!(text_of(&r.file_contexts[0]), "one\ntwo");
}

#[test]
fn an_empty_file_is_an_empty_text_file_not_a_failure() {
    let r = assemble_one("/a.txt", &[], None, loaded(b""));
    assert!(r.failed_files.is_empty());
    assert_eq!(text_of(&r.file_contexts[0]), "");
    assert_eq!(r.file_contexts[0].line_count, 0);
}

#[test]
fn the_byte_cap_cuts_at_a_line_boundary_and_marks_the_range_apply_diff_relies_on() {
    // Budget 9: "aaaa" (4) + "\n" + "bbbb" (4) = 9 fits, "cccc" does not.
    let r = assemble_one("/a.txt", &[], Some(9), loaded(b"aaaa\nbbbb\ncccc\ndddd\n"));
    let c = &r.file_contexts[0];
    assert_eq!(text_of(c), "aaaa\nbbbb");
    assert_eq!((c.line_range_start, c.line_range_end), (Some(1), Some(2)));
    assert_eq!(c.line_count, 4);
}

#[test]
fn a_first_line_longer_than_the_cap_is_a_named_failure_not_an_empty_file() {
    let r = assemble_one("/a.txt", &[], Some(3), loaded(b"abcdefgh\nij\n"));
    assert!(r.file_contexts.is_empty());
    let message = &r.failed_files[0].error.as_ref().unwrap().message;
    assert!(
        message.contains("Line 1 is longer than the 3 bytes"),
        "{message}"
    );
}

#[test]
fn line_ranges_return_one_segment_each_with_the_requested_range() {
    let r = assemble_one("/a.txt", &[(2, 4)], None, loaded(b"l1\nl2\nl3\nl4\nl5\n"));
    assert_eq!(r.file_contexts.len(), 1);
    let c = &r.file_contexts[0];
    assert_eq!(text_of(c), "l2\nl3");
    assert_eq!((c.line_range_start, c.line_range_end), (Some(2), Some(4)));
    assert_eq!(c.line_count, 5);
}

#[test]
fn a_range_past_the_end_still_names_its_range_with_empty_content() {
    let r = assemble_one("/a.txt", &[(10, 12)], None, loaded(b"l1\nl2\n"));
    let c = &r.file_contexts[0];
    assert_eq!(text_of(c), "");
    assert_eq!((c.line_range_start, c.line_range_end), (Some(10), Some(12)));
}

#[test]
fn the_batch_budget_is_shared_and_files_past_it_are_named_failures() {
    let req = request(
        vec![file("/a", &[]), file("/b", &[]), file("/c", &[])],
        None,
        Some(9),
    );
    let r = assemble(
        &req,
        vec![
            loaded(b"12345678\n"),
            loaded(b"abcdefgh\n"),
            loaded(b"zz\n"),
        ],
    );
    // The first spends all 9 (8 bytes and the final newline); the rest are named, not dropped.
    assert_eq!(text_of(&r.file_contexts[0]), "12345678\n");
    let failed: Vec<_> = r.failed_files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(failed, vec!["/b", "/c"]);
    for f in &r.failed_files {
        assert!(
            f.error
                .as_ref()
                .unwrap()
                .message
                .contains("total size limit")
        );
    }
}

#[test]
fn a_batch_budget_smaller_than_a_line_cuts_that_file_with_the_size_limit_message() {
    let req = request(vec![file("/a", &[]), file("/b", &[])], None, Some(10));
    let r = assemble(&req, vec![loaded(b"12345678\n"), loaded(b"abcdefgh\n")]);
    // /a spends 9 of 10; /b has 1 byte left, which cuts its only line.
    assert_eq!(r.file_contexts.len(), 1);
    assert!(
        r.failed_files[0]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("Line 1 is longer than the 1 bytes")
    );
}

#[test]
fn missing_and_unreadable_files_fail_individually_and_the_rest_are_read() {
    let req = request(
        vec![file("/gone", &[]), file("/denied", &[]), file("/ok", &[])],
        None,
        None,
    );
    let r = assemble(
        &req,
        vec![
            FileProbe::NotFound,
            FileProbe::Failed("Permission denied (os error 13)".into()),
            loaded(b"x\n"),
        ],
    );
    assert_eq!(r.file_contexts.len(), 1);
    let msgs: Vec<_> = r
        .failed_files
        .iter()
        .map(|f| (f.path.as_str(), f.error.as_ref().unwrap().message.as_str()))
        .collect();
    assert_eq!(
        msgs,
        vec![
            ("/gone", "File does not exist"),
            (
                "/denied",
                "Could not read file: Permission denied (os error 13)"
            ),
        ]
    );
}

/// #2151: a missing file carries the typed `not_found` flag, every other failure does not, so the
/// client never classifies by text.
#[test]
fn only_a_missing_file_carries_the_typed_not_found_flag() {
    let req = request(vec![file("/gone", &[]), file("/denied", &[])], None, None);
    let r = assemble(
        &req,
        vec![
            FileProbe::NotFound,
            FileProbe::Failed("Permission denied (os error 13)".into()),
        ],
    );
    let flags: Vec<_> = r
        .failed_files
        .iter()
        .map(|f| (f.path.as_str(), f.not_found))
        .collect();
    assert_eq!(flags, vec![("/gone", true), ("/denied", false)]);
}

#[test]
fn invalid_utf8_and_binary_extensions_come_back_as_binary() {
    let r = assemble_one("/a.txt", &[], None, loaded(&[0xff, 0xfe, 0x41, 0x80]));
    assert!(matches!(
        r.file_contexts[0].content,
        Some(Content::BinaryContent(_))
    ));
    let png = [0x89, b'P', b'N', b'G', 0, 1, 2];
    let r = assemble_one("/pic.PNG", &[], None, loaded(&png));
    assert_eq!(
        r.file_contexts[0].content,
        Some(Content::BinaryContent(png.to_vec()))
    );
    assert_eq!(r.file_contexts[0].line_count, 0);
}

#[test]
fn an_extensionless_file_is_decided_by_its_first_bytes() {
    assert!(should_read_as_binary("/bin/tool", b"\x7fELF\0\0\0"));
    assert!(!should_read_as_binary("/bin/run", b"#!/bin/sh\necho hi\n"));
    // A text extension never demotes to binary on its own.
    assert!(!should_read_as_binary("/x/main.rs", b"\0\0\0"));
}

#[test]
fn a_binary_file_over_the_cap_is_too_large_with_both_sizes_named() {
    let probe = FileProbe::Loaded(LoadedFile {
        size: 3_000_000,
        mtime_ms: None,
        bytes: vec![0; 10],
        complete: false,
        total_lines: None,
    });
    let r = assemble_one("/pic.png", &[], None, probe);
    let message = &r.failed_files[0].error.as_ref().unwrap().message;
    assert!(message.contains("3.0 MB > 1.0 MB limit"), "{message}");
}

#[test]
fn a_prefix_of_a_big_file_is_truncated_text_with_the_whole_files_line_count() {
    // Cap 9: the third line crosses it. The prefix ends mid-line and the file has 1000 lines.
    let probe = FileProbe::Loaded(LoadedFile {
        size: 6000,
        mtime_ms: None,
        bytes: b"aaaa\nbbbb\ncccc\nddd".to_vec(),
        complete: false,
        total_lines: Some(1000),
    });
    let r = assemble_one("/big.txt", &[], Some(9), probe);
    let c = &r.file_contexts[0];
    assert_eq!(text_of(c), "aaaa\nbbbb");
    assert_eq!((c.line_range_start, c.line_range_end), (Some(1), Some(2)));
    assert_eq!(c.line_count, 1000);
}

#[test]
fn a_prefix_that_ends_inside_a_multibyte_character_is_not_binary() {
    let mut bytes = "αβγ\nδεζ\n".as_bytes().to_vec();
    bytes.pop(); // the final \n
    bytes.pop(); // half of ζ
    let probe = FileProbe::Loaded(LoadedFile {
        size: 100,
        mtime_ms: None,
        bytes,
        complete: false,
        total_lines: Some(50),
    });
    let r = assemble_one("/u.txt", &[], Some(6), probe);
    assert!(matches!(
        r.file_contexts[0].content,
        Some(Content::TextContent(_))
    ));
}

#[test]
fn a_prefix_the_cap_never_cut_is_refused_rather_than_returned_as_if_whole() {
    let probe = FileProbe::Loaded(LoadedFile {
        size: 10_000_000,
        mtime_ms: None,
        bytes: b"short\n".to_vec(),
        complete: false,
        total_lines: Some(5),
    });
    let r = assemble_one("/huge.txt", &[], Some(5_000_000), probe);
    assert!(r.file_contexts.is_empty());
    assert!(
        r.failed_files[0]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("too large to read in this mode")
    );
}

#[test]
fn plans_ask_for_twice_the_cap_plus_slack_and_ranges_scan_to_their_last_line() {
    let req = request(
        vec![file("/a", &[]), file("/b", &[(5, 20), (1, 3)])],
        Some(100),
        Some(60),
    );
    assert_eq!(
        plan(&req.files[0], &req),
        GatherPlan {
            prefix_bytes: 124,
            through_line: None
        }
    );
    assert_eq!(
        plan(&req.files[1], &req),
        GatherPlan {
            prefix_bytes: RANGE_SCAN_CAP_BYTES,
            through_line: Some(20)
        }
    );
    // A huge cap is bounded by what one command can return.
    let big = request(vec![file("/a", &[])], Some(u32::MAX), None);
    assert_eq!(plan(&big.files[0], &big).prefix_bytes, MAX_GATHER_BYTES);
    // A line range means nothing for a known binary type.
    let png = request(vec![file("/p.png", &[(1, 2)])], None, None);
    assert_eq!(plan(&png.files[0], &png).through_line, None);
}

#[test]
fn the_whole_request_error_is_one_failure_with_an_empty_path() {
    let r = whole_request_error("boom");
    assert!(r.file_contexts.is_empty());
    assert_eq!(r.failed_files.len(), 1);
    assert_eq!(r.failed_files[0].path, "");
    assert_eq!(r.failed_files[0].error.as_ref().unwrap().message, "boom");
}

fn section(output: &str) -> ParsedSection {
    ParsedSection {
        name: "f0".into(),
        output: output.as_bytes().to_vec(),
        exit: 0,
    }
}

#[test]
fn a_section_parses_into_a_probe() {
    let plan = GatherPlan {
        prefix_bytes: 100,
        through_line: None,
    };
    let payload = super::super::base64::encode(b"hi\nthere\n");
    let probe = parse_file_section(
        &section(&format!("OK\n9\n1700000000\n-\n{payload}")),
        &plan,
        Some(Base64Tool::GnuW0),
    );
    assert_eq!(
        probe,
        FileProbe::Loaded(LoadedFile {
            size: 9,
            mtime_ms: Some(1_700_000_000_000),
            bytes: b"hi\nthere\n".to_vec(),
            complete: true,
            total_lines: None,
        })
    );
    assert_eq!(
        parse_file_section(&section("NOTFOUND\n"), &plan, None),
        FileProbe::NotFound
    );
    assert!(matches!(
        parse_file_section(&section("DIR\n"), &plan, None),
        FileProbe::Failed(m) if m.contains("directory")
    ));
    assert!(matches!(
        parse_file_section(&section("sh: boom\n"), &plan, None),
        FileProbe::Failed(m) if m == "sh: boom"
    ));
}

#[test]
fn wrapped_base64_payloads_and_a_cut_file_parse_with_their_line_count() {
    let plan = GatherPlan {
        prefix_bytes: 6,
        through_line: None,
    };
    let payload = super::super::base64::encode(b"abc\nde");
    let (a, b) = payload.split_at(4);
    let probe = parse_file_section(
        &section(&format!("OK\n5000\n-\n77\n{a}\n{b}\n")),
        &plan,
        Some(Base64Tool::Plain),
    );
    let FileProbe::Loaded(file) = probe else {
        panic!()
    };
    assert_eq!(file.bytes, b"abc\nde");
    assert!(!file.complete);
    assert_eq!(file.total_lines, Some(77));
    assert_eq!(file.mtime_ms, None);
}

#[test]
fn a_ranged_section_is_complete_when_the_file_has_no_more_lines_than_were_scanned() {
    let plan = GatherPlan {
        prefix_bytes: 1000,
        through_line: Some(10),
    };
    let payload = super::super::base64::encode(b"a\nb\n");
    let FileProbe::Loaded(file) = parse_file_section(
        &section(&format!("OK\n4\n1\n2\n{payload}")),
        &plan,
        Some(Base64Tool::GnuW0),
    ) else {
        panic!()
    };
    assert!(file.complete);
    let FileProbe::Loaded(file) = parse_file_section(
        &section(&format!("OK\n400\n1\n99\n{payload}")),
        &plan,
        Some(Base64Tool::GnuW0),
    ) else {
        panic!()
    };
    assert!(!file.complete);
    assert_eq!(file.total_lines, Some(99));
}

#[test]
fn the_posix_builder_quotes_the_path_and_expands_tilde_on_the_host() {
    let plan = GatherPlan {
        prefix_bytes: 50,
        through_line: None,
    };
    let hostile = posix_file_section("/tmp/it's $x `y`", &plan, "base64 -w0");
    assert!(
        hostile.starts_with("f='/tmp/it'\\''s $x `y`'\n"),
        "{hostile}"
    );
    let home = posix_file_section("~/notes/a b.txt", &plan, "base64 -w0");
    assert!(home.starts_with("f=\"$HOME\"'/notes/a b.txt'\n"), "{home}");
    assert!(posix_file_section("~", &plan, "base64").starts_with("f=\"$HOME\"\n"));
    assert!(home.contains("head -c 50 < \"$f\" | base64 -w0"));
    assert!(home.contains("[ \"$size\" -gt 50 ]"));
    let ranged = GatherPlan {
        prefix_bytes: 80,
        through_line: Some(7),
    };
    let s = posix_file_section("/x", &ranged, "base64");
    assert!(
        s.contains("sed -n '1,7p;7q' < \"$f\" | head -c 80 | base64"),
        "{s}"
    );
    assert!(s.contains("if true; then grep -c ''"));
}

#[test]
fn the_pwsh_builder_quotes_the_path_and_expands_tilde_on_the_host() {
    let plan = GatherPlan {
        prefix_bytes: 50,
        through_line: None,
    };
    let s = pwsh_file_section("C:\\it's\\a.txt", &plan);
    assert!(s.starts_with("$f = 'C:\\it''s\\a.txt'\n"), "{s}");
    assert!(pwsh_file_section("~/a.txt", &plan).starts_with("$f = Join-Path $HOME 'a.txt'\n"));
    assert!(s.contains("ToBase64String($b, 0, $n)"));
}

#[test]
fn every_base64_tool_has_an_encoder_filter() {
    assert_eq!(Base64Tool::GnuW0.encoder_filter(), "base64 -w0");
    assert_eq!(Base64Tool::Plain.encoder_filter(), "base64");
    assert_eq!(Base64Tool::Openssl.encoder_filter(), "openssl base64");
    assert_eq!(Base64Tool::Od.encoder_filter(), "od -An -v -tx1");
}

#[test]
fn a_range_section_skips_whole_blocks_and_reads_back() {
    let command = posix_range_section("/a b/f", 7 * RANGE_BLOCK_BYTES, RANGE_CHUNK_BYTES, "base64");
    assert!(command.contains("f='/a b/f'"), "{command}");
    assert!(command.contains("bs=1048576 skip=7 count=4"), "{command}");
    assert!(command.contains("head -c 4194304 | base64"), "{command}");
    let payload = super::super::base64::encode(b"tail bytes");
    let bytes =
        parse_range_section(&section(&format!("OK\n{payload}")), Some(Base64Tool::GnuW0)).unwrap();
    assert_eq!(bytes, b"tail bytes");
    assert_eq!(
        parse_range_section(&section("NOTFOUND"), None).unwrap_err(),
        "File does not exist"
    );
    assert!(parse_range_section(&section("garbage"), None).is_err());
}

#[test]
fn the_pwsh_range_section_seeks_and_the_ranged_read_never_re_encodes_text() {
    let range = pwsh_range_section("C:\\it's", 8_388_608, 4_194_304);
    assert!(range.contains("$f = 'C:\\it''s'"), "{range}");
    assert!(range.contains("Seek(8388608"), "{range}");
    let ranged = pwsh_file_section(
        "C:\\f.txt",
        &GatherPlan {
            prefix_bytes: 1000,
            through_line: Some(5),
        },
    );
    assert!(ranged.contains("IndexOf($buf, [byte]10"), "{ranged}");
    assert!(!ranged.contains("UTF8"), "{ranged}");
    assert!(!ranged.contains("-join"), "{ranged}");
}
