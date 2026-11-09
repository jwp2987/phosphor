use super::*;

#[test]
fn stamps_and_digests_parse_what_the_commands_print() {
    assert_eq!(
        parse_stamp("12|1700000000\n"),
        Some(FileStamp {
            size: 12,
            mtime_secs: 1700000000
        })
    );
    assert_eq!(parse_stamp("absent"), None);
    assert_eq!(parse_stamp("12|"), None);
    assert_eq!(parse_stamp(""), None);
    assert!(parse_sha(&"0f".repeat(32)).is_some());
    assert_eq!(parse_sha(""), None);
    assert_eq!(parse_sha("not a hash"), None);
}

#[test]
fn the_commands_use_the_tools_the_guarded_write_uses() {
    let stamp = stamp_command(false, "/a b/it's");
    assert!(stamp.contains("stat -c %Y") && stamp.contains("stat -f %m"));
    assert!(stamp.contains("'/a b/it'\\''s'"), "{stamp}");
    let sha = sha_command(false, "~/x");
    assert!(sha.contains("sha256sum") && sha.contains("shasum -a 256"));
    assert!(sha.contains("$HOME"));
    assert!(stamp_command(true, "C:\\x").contains("ToUnixTimeSeconds"));
    assert!(sha_command(true, "C:\\x").contains("Get-FileHash"));
}
