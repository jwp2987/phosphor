use super::*;

const FIRST_AND_LAST: ChunkPosition = ChunkPosition {
    first: true,
    last: true,
};

#[test]
fn chunks_are_at_most_64k_and_an_empty_file_is_one_empty_chunk() {
    assert_eq!(split_chunks(b"", false).len(), 1);
    let data = vec![7u8; WRITE_CHUNK_BYTES * 2 + 1];
    let chunks = split_chunks(&data, false);
    assert_eq!(chunks.len(), 3);
    assert!(chunks.iter().all(|c| c.len() <= WRITE_CHUNK_BYTES));
    assert_eq!(chunks.concat(), data);
}

#[test]
fn a_single_chunk_write_is_dir_temp_decode_mode_rename_in_one_command() {
    let c = posix_write_command(
        "/a/b c.txt",
        "n0nce",
        b"hi",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        None,
    );
    assert!(c.starts_with("f='/a/b c.txt'\n"), "{c}");
    assert!(c.contains("tmp=\"$f.phosphor-tmp-n0nce\""));
    assert!(c.contains("mkdir -p -- \"$(dirname -- \"$f\")\""));
    assert!(c.contains("printf '%s' 'aGk=' | base64 -d > \"$tmp\""));
    assert!(c.contains("stat -c %a -- \"$f\"") && c.contains("stat -f %Lp -- \"$f\""));
    assert!(c.contains("mv -f -- \"$tmp\" \"$f\""));
    assert!(!c.contains("CONFLICT"));
}

#[test]
fn middle_chunks_append_and_only_the_last_one_renames() {
    let first = posix_write_command(
        "/x",
        "n",
        b"a",
        ChunkPosition {
            first: true,
            last: false,
        },
        Some(DecodeTool::Upper),
        None,
    );
    let mid = posix_write_command(
        "/x",
        "n",
        b"b",
        ChunkPosition {
            first: false,
            last: false,
        },
        Some(DecodeTool::Upper),
        None,
    );
    let last = posix_write_command(
        "/x",
        "n",
        b"c",
        ChunkPosition {
            first: false,
            last: true,
        },
        Some(DecodeTool::Upper),
        None,
    );
    assert!(first.contains("base64 -D > \"$tmp\"") && first.contains("mkdir"));
    assert!(mid.contains(">> \"$tmp\"") && !mid.contains("mkdir") && !mid.contains("mv -f"));
    assert!(last.contains(">> \"$tmp\"") && last.contains("mv -f"));
}

#[test]
fn a_guarded_write_checks_size_and_mtime_or_absence_before_the_rename() {
    let stamp = Expect::Stamp(FileStamp {
        size: 12,
        mtime_secs: 1700000000,
    });
    let c = posix_write_command(
        "/x",
        "n",
        b"a",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        Some(stamp),
    );
    let check = c
        .find("[ \"$cur\" = '12|1700000000' ]")
        .expect("check present");
    assert!(check < c.find("mv -f").unwrap());
    assert!(c.contains("exit 5"));
    let absent = posix_write_command(
        "/x",
        "n",
        b"a",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        Some(Expect::Absent),
    );
    assert!(absent.contains("= 'absent' ]"));
}

#[test]
fn hostile_paths_are_quoted_and_tilde_expands_on_the_host() {
    let c = posix_write_command(
        "/t/it's $x",
        "n",
        b"",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        None,
    );
    assert!(c.starts_with("f='/t/it'\\''s $x'\n"), "{c}");
    let h = posix_write_command(
        "~/n.txt",
        "n",
        b"",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        None,
    );
    assert!(h.starts_with("f=\"$HOME\"'/n.txt'\n"), "{h}");
    assert_eq!(posix_delete_command("/a b"), "f='/a b'\nrm -- \"$f\"\n");
}

#[test]
fn the_pwsh_write_never_uses_exit_inside_the_framed_block() {
    let c = pwsh_write_command(
        "C:\\it's.txt",
        "n",
        b"hi",
        FIRST_AND_LAST,
        Some(Expect::Absent),
    );
    assert!(c.starts_with("$f = 'C:\\it''s.txt'\n"), "{c}");
    assert!(c.contains("WriteAllBytes($tmp, [Convert]::FromBase64String('aGk='))"));
    assert!(c.contains("Move-Item -LiteralPath $tmp -Destination $f -Force"));
    assert!(!c.contains("exit "), "{c}");
    assert!(c.contains("$global:LASTEXITCODE = 5; return"));
    assert!(pwsh_delete_command("~/x").contains("Join-Path $HOME 'x'"));
}

#[test]
fn failure_messages_name_the_cause_and_a_conflict_says_nothing_was_overwritten() {
    assert!(
        write_failure_message("/x", EXIT_CONFLICT, "CONFLICT\n").contains("changed on the host")
    );
    assert!(write_failure_message("/x", EXIT_MKDIR, "mkdir: denied").contains("mkdir: denied"));
    assert!(write_failure_message("/x", 99, "").ends_with("failed"));
}

#[test]
fn without_a_decoder_bytes_travel_as_printf_octal_escapes_in_small_chunks() {
    assert_eq!(printf_escape(b"ab 1~"), "ab 1~");
    assert_eq!(printf_escape(b"100%'\\"), "100\\045\\047\\134");
    assert_eq!(printf_escape(b"\n\0\xc3\xa9"), "\\012\\000\\303\\251");
    let data = vec![0xffu8; PRINTF_CHUNK_BYTES * 2 + 1];
    let chunks = split_chunks(&data, true);
    assert_eq!(chunks.len(), 3);
    // Worst case, four characters per byte: still one command under 64 KiB.
    assert!(printf_escape(chunks[0]).len() <= 64 * 1024);
    let c = posix_write_command("/x", "n", b"a%\n", FIRST_AND_LAST, None, None);
    assert!(c.contains("printf 'a\\045\\012' > \"$tmp\""), "{c}");
    assert!(!c.contains("base64"));
    let mid = posix_write_command(
        "/x",
        "n",
        b"b",
        ChunkPosition {
            first: false,
            last: false,
        },
        None,
        None,
    );
    assert!(mid.contains("printf 'b' >> \"$tmp\""));
}

#[test]
fn a_hex_digest_is_exactly_sixty_four_hex_characters_lowercased() {
    let upper = "ABCDEF0123456789".repeat(4);
    let digest = HexDigest::parse(&format!("  {upper}\n")).unwrap();
    assert_eq!(digest.as_str(), upper.to_lowercase());
    assert!(HexDigest::parse("abc").is_none());
    assert!(HexDigest::parse(&"g".repeat(64)).is_none());
    assert!(HexDigest::parse(&"a".repeat(65)).is_none());
}

#[test]
fn a_hash_guarded_write_compares_the_files_sha256_before_the_rename() {
    let digest = HexDigest::parse(&"ab".repeat(32)).unwrap();
    let c = posix_write_command(
        "/x",
        "n",
        b"a",
        FIRST_AND_LAST,
        Some(DecodeTool::Lower),
        Some(Expect::Sha256(digest)),
    );
    assert!(c.contains("sha256sum") && c.contains("shasum -a 256"));
    let check = c
        .find(&format!("[ \"$cur\" = '{}' ]", "ab".repeat(32)))
        .expect("check present");
    assert!(check < c.find("mv -f").unwrap());
    assert!(c.contains("exit 5"));
    let p = pwsh_write_command(
        "C:\\x",
        "n",
        b"a",
        FIRST_AND_LAST,
        Some(Expect::Sha256(digest)),
    );
    assert!(p.contains("Get-FileHash"));
    assert!(p.find("-ne '").unwrap() < p.find("Move-Item").unwrap());
}
