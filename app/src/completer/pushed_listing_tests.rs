use typed_path::TypedPathBuf;
use warp_completer::completer::{EngineDirEntry, EngineFileType};

use super::{PushedListing, lookup, parse_pushed_listing, store};
use crate::terminal::model::session::SessionId;

fn entry(name: &str, file_type: EngineFileType) -> EngineDirEntry {
    EngineDirEntry {
        file_name: name.to_owned(),
        file_type,
    }
}

#[test]
fn a_complete_listing_parses_with_its_dir_flags() {
    assert_eq!(
        parse_pushed_listing("L d.git fREADME.md dsub"),
        Some(PushedListing::Complete(vec![
            entry(".git", EngineFileType::Directory),
            entry("README.md", EngineFileType::File),
            entry("sub", EngineFileType::Directory),
        ]))
    );
}

#[test]
fn an_empty_directory_is_a_complete_empty_listing() {
    assert_eq!(
        parse_pushed_listing("L"),
        Some(PushedListing::Complete(vec![]))
    );
}

#[test]
fn partial_and_too_large_markers_parse() {
    assert_eq!(
        parse_pushed_listing("P da"),
        Some(PushedListing::Partial(vec![entry(
            "a",
            EngineFileType::Directory
        )]))
    );
    assert_eq!(parse_pushed_listing("T"), Some(PushedListing::TooLarge));
    assert_eq!(
        parse_pushed_listing("T da"),
        None,
        "too-large carries no entries"
    );
}

#[test]
fn percent_encoded_names_decode_to_the_original_bytes() {
    let parsed = parse_pushed_listing("L fa%20b f%2525 f%22q%5C%0A%C3%A9").unwrap();
    let PushedListing::Complete(entries) = parsed else {
        panic!("complete listing expected")
    };
    let names: Vec<&str> = entries.iter().map(|e| e.file_name.as_str()).collect();
    assert_eq!(names, vec!["a b", "%25", "\"q\\\n\u{e9}"]);
}

#[test]
fn a_name_that_is_not_utf8_leaves_out_that_entry_only() {
    assert_eq!(
        parse_pushed_listing("L fbad%FF da"),
        Some(PushedListing::Complete(vec![entry(
            "a",
            EngineFileType::Directory
        )]))
    );
}

#[test]
fn malformed_listings_are_dropped_whole() {
    for bad in [
        "", "X", "L", "L  da", "L x", "L d", "L d%2", "L d%zz", "L da/b", "L d%2Fb", "Lda", "T ",
    ] {
        // "L" alone is valid (empty directory); everything else here is not.
        if bad == "L" {
            continue;
        }
        assert_eq!(parse_pushed_listing(bad), None, "{bad:?}");
    }
}

#[test]
fn lookup_answers_only_for_the_listed_directory_and_only_when_usable() {
    let id = SessionId::from(77_001u64);
    store(id, Some("/srv/app"), "L dsub fa.txt");
    let dir = TypedPathBuf::from("/srv/app/");
    let (entries, complete) = lookup(id, &dir).expect("the listed directory");
    assert!(complete);
    assert_eq!(entries.len(), 2);
    assert!(lookup(id, &TypedPathBuf::from("/srv/other")).is_none());
    assert!(lookup(SessionId::from(77_002u64), &dir).is_none());

    store(id, Some("/srv/app"), "P dsub");
    assert!(
        !lookup(id, &dir).unwrap().1,
        "a truncated listing is not complete"
    );

    store(id, Some("/srv/app"), "T");
    assert!(lookup(id, &dir).is_none(), "too large answers nothing");

    store(id, Some("/srv/app"), "L dsub");
    store(id, None, "L dother");
    let (entries, _) = lookup(id, &dir).unwrap();
    assert_eq!(entries.len(), 1, "a listing without a pwd is ignored");
    assert_eq!(entries[0].file_name, "sub");
}
