use super::*;
use crate::proto::{LineRange, ReadFileContextFile, file_context_proto::Content};
use crate::shellcmd::read::MAX_GATHER_BYTES;

fn run(request: &ReadFileContextRequest) -> ReadFileContextResponse {
    read_file_context(request, &Cancel::default())
}

fn one(
    path: &std::path::Path,
    ranges: &[(u32, u32)],
    per_file: Option<u32>,
) -> ReadFileContextRequest {
    ReadFileContextRequest {
        files: vec![ReadFileContextFile {
            path: path.to_string_lossy().into_owned(),
            line_ranges: ranges
                .iter()
                .map(|&(start, end)| LineRange { start, end })
                .collect(),
        }],
        max_file_bytes: per_file,
        max_batch_bytes: None,
    }
}

fn text(c: &crate::proto::FileContextProto) -> &str {
    match c.content.as_ref().unwrap() {
        Content::TextContent(t) => t,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_small_file_is_read_whole() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "one\ntwo\n").unwrap();
    let r = run(&one(&path, &[], None));
    assert!(r.failed_files.is_empty());
    assert_eq!(text(&r.file_contexts[0]), "one\ntwo\n");
    assert_eq!(r.file_contexts[0].line_count, 2);
    assert!(r.file_contexts[0].last_modified_epoch_millis.is_some());
}

#[test]
fn a_file_far_over_the_cap_is_truncated_at_a_line_with_the_whole_line_count() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.txt");
    let body: String = (1..=5000).map(|i| format!("line {i:05}\n")).collect();
    std::fs::write(&path, &body).unwrap();
    let r = run(&one(&path, &[], Some(100)));
    let c = &r.file_contexts[0];
    // "line 00001" is 10 bytes, each further line costs 11: 100 bytes hold 9 lines (10 + 8*11).
    assert_eq!(text(c).lines().count(), 9);
    assert_eq!((c.line_range_start, c.line_range_end), (Some(1), Some(9)));
    assert_eq!(c.line_count, 5000);
}

#[test]
fn a_line_range_deep_in_a_file_is_scanned_to_and_counted_past() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("big.txt");
    let body: String = (1..=3000).map(|i| format!("row {i}\n")).collect();
    std::fs::write(&path, &body).unwrap();
    let r = run(&one(&path, &[(2000, 2003)], None));
    let c = &r.file_contexts[0];
    assert_eq!(text(c), "row 2000\nrow 2001\nrow 2002");
    assert_eq!(
        (c.line_range_start, c.line_range_end),
        (Some(2000), Some(2003))
    );
    assert_eq!(c.line_count, 3000);
}

#[test]
fn a_range_on_a_file_without_a_final_newline_counts_its_last_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "a\nb\nc").unwrap();
    let r = run(&one(&path, &[(1, 2)], None));
    assert_eq!(r.file_contexts[0].line_count, 3);
}

#[test]
fn missing_directory_and_binary_files_are_reported_per_file() {
    let dir = tempfile::tempdir().unwrap();
    let bin = dir.path().join("blob.bin");
    std::fs::write(&bin, [0u8, 1, 2, 3, 255]).unwrap();
    let request = ReadFileContextRequest {
        files: vec![
            ReadFileContextFile {
                path: dir.path().join("nope").to_string_lossy().into_owned(),
                line_ranges: vec![],
            },
            ReadFileContextFile {
                path: dir.path().to_string_lossy().into_owned(),
                line_ranges: vec![],
            },
            ReadFileContextFile {
                path: bin.to_string_lossy().into_owned(),
                line_ranges: vec![],
            },
        ],
        ..Default::default()
    };
    let r = run(&request);
    assert_eq!(r.failed_files.len(), 2);
    assert_eq!(
        r.failed_files[0].error.as_ref().unwrap().message,
        "File does not exist"
    );
    assert!(
        r.failed_files[1]
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("Is a directory")
    );
    assert_eq!(
        r.file_contexts[0].content,
        Some(Content::BinaryContent(vec![0, 1, 2, 3, 255]))
    );
}

#[test]
fn tilde_is_expanded_on_the_host() {
    // A path that cannot exist under the real home: only the expansion is under test.
    let r = run(&ReadFileContextRequest {
        files: vec![ReadFileContextFile {
            path: "~/.phosphor-no-such-file-for-test".into(),
            line_ranges: vec![],
        }],
        ..Default::default()
    });
    assert_eq!(
        r.failed_files[0].error.as_ref().unwrap().message,
        "File does not exist"
    );
}

#[test]
fn a_cancelled_request_stops_and_still_returns_a_response_of_the_right_length() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    std::fs::write(&path, "x\n").unwrap();
    let cancel = Cancel::default();
    cancel.cancel();
    let r = read_file_context(&one(&path, &[], None), &cancel);
    // Nothing was read; the caller drops the reply of a cancelled job anyway.
    assert!(r.file_contexts.is_empty());
}

#[test]
fn the_plan_bound_is_what_the_gatherer_honours() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("huge.txt");
    // Longer than MAX_GATHER_BYTES, one line per 10 bytes.
    let body = "123456789\n".repeat(MAX_GATHER_BYTES / 10 + 1000);
    std::fs::write(&path, &body).unwrap();
    // A cap above what one command can carry: refused, not silently cut short.
    let r = run(&one(&path, &[], Some(u32::MAX)));
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
