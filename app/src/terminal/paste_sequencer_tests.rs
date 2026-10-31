use super::*;

#[test]
fn keys_typed_during_a_paste_are_released_behind_it_in_order() {
    let mut seq = PasteSequencer::default();
    assert_eq!(seq.on_key(b"a", false), KeyDisposition::PassThrough);
    let id = seq.begin();
    assert_eq!(seq.on_key(b"x", false), KeyDisposition::Held);
    assert_eq!(seq.on_key(b"y", false), KeyDisposition::Held);
    assert_eq!(seq.on_key(b"\r", false), KeyDisposition::Held);
    assert_eq!(
        seq.complete(id),
        Some(vec![b"x".to_vec(), b"y".to_vec(), b"\r".to_vec()])
    );
    // Once delivered, keys flow straight through again.
    assert_eq!(seq.on_key(b"z", false), KeyDisposition::PassThrough);
}

#[test]
fn abort_key_cancels_the_pending_paste_and_releases_earlier_keys() {
    let mut seq = PasteSequencer::default();
    let id = seq.begin();
    assert_eq!(seq.on_key(b"x", false), KeyDisposition::Held);
    assert_eq!(
        seq.on_key(&[0x03], true),
        KeyDisposition::CancelledPaste(vec![b"x".to_vec()])
    );
    // The late result of the cancelled paste writes nothing.
    assert_eq!(seq.complete(id), None);
    assert!(!seq.has_pending());
}

#[test]
fn a_superseded_paste_keeps_the_held_keys() {
    let mut seq = PasteSequencer::default();
    let first = seq.begin();
    seq.on_key(b"1", false);
    let second = seq.begin();
    seq.on_key(b"2", false);
    assert_eq!(seq.complete(first), None);
    assert_eq!(
        seq.complete(second),
        Some(vec![b"1".to_vec(), b"2".to_vec()])
    );
}
