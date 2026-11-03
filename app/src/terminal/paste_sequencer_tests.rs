use super::*;

#[test]
fn a_superseded_paste_keeps_the_held_keys() {
    let mut seq = PasteSequencer::<Vec<u8>>::default();
    let first = seq.begin();
    seq.hold(b"1".to_vec());
    let second = seq.begin();
    seq.hold(b"2".to_vec());
    assert_eq!(seq.complete(first), None);
    assert_eq!(
        seq.complete(second),
        Some(vec![b"1".to_vec(), b"2".to_vec()])
    );
}

// #1300: the input editor holds typed keys (not pty bytes) behind its paste with the same ordering.
#[test]
fn generic_keys_are_held_in_order_and_cancel_returns_them() {
    let mut seq = PasteSequencer::<&str>::default();
    assert!(!seq.hold("dropped?"));
    let id = seq.begin();
    assert!(seq.hold("a"));
    assert!(seq.hold("\r"));
    assert_eq!(seq.complete(id), Some(vec!["a", "\r"]));

    let id = seq.begin();
    assert!(seq.hold("b"));
    assert_eq!(seq.cancel(), vec!["b"]);
    assert_eq!(seq.complete(id), None);
}
