use super::*;

/// #1278: a ranged or truncated read must say which lines it returned. Old code dropped
/// `line_range`, so a truncated file looked complete.
#[test]
fn read_result_carries_line_range_when_present() {
    let result = api::message::tool_call_result::Result::ReadFiles(api::ReadFilesResult {
        result: Some(api::read_files_result::Result::TextFilesSuccess(
            api::read_files_result::TextFilesSuccess {
                files: vec![
                    api::FileContent {
                        file_path: "a.rs".into(),
                        content: "x".into(),
                        line_range: Some(api::FileContentLineRange { start: 5, end: 9 }),
                    },
                    api::FileContent {
                        file_path: "b.rs".into(),
                        content: "y".into(),
                        line_range: None,
                    },
                ],
                ..Default::default()
            },
        )),
    });
    let json = (READ_FILES.result_to_json)(&result).expect("claims its own result");
    assert_eq!(json["files"][0]["line_range"]["start"], 5);
    assert_eq!(json["files"][0]["line_range"]["end"], 9);
    assert!(json["files"][1].get("line_range").is_none());
}
