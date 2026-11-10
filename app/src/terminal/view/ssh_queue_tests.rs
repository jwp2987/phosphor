//! #1998: every entry behind a dropped ssh connect hold is accounted for
//! ([`dropped_entry_inputs`]).

use super::*;
use crate::editor::{EditorAction, HeldPasteKey};
use crate::terminal::input_queue::{EditorKeyPayload, HELD_KEY, HELD_PASTE, HELD_SEQ, HELD_TYPED};

const EVERY_THEN: [SshDropThen; 4] = [
    SshDropThen::Discard,
    SshDropThen::HandBackText,
    SshDropThen::WriteToLocalPty,
    SshDropThen::GiveUpToPty,
];

fn pty_bytes(bytes: &[u8]) -> EntryKind {
    EntryKind::PtyBytes {
        bytes: bytes.to_vec(),
        takes_over_agent_control: true,
    }
}

fn editor_text(text: &str) -> EntryKind {
    EntryKind::EditorKey(EditorKeyPayload::new(HeldPasteKey::Text(text.to_owned())))
}

fn editor_action(action: EditorAction) -> EntryKind {
    EntryKind::EditorKey(EditorKeyPayload::new(HeldPasteKey::Action(action)))
}

fn inputs(kind: EntryKind, then: SshDropThen) -> Vec<HeldInput> {
    match dropped_entry_inputs(kind, then, true) {
        DroppedEntry::Inputs(inputs) => inputs,
        other => panic!("{then:?}: expected inputs, got {other:?}"),
    }
}

/// What a run of held inputs shows in an empty editor, as the hand-back computes it
/// (`finish_dropped_ssh_gate`, `SshDropThen::HandBackText`).
fn handed_back_text(held: Vec<HeldInput>) -> String {
    let mut collector = SshTypeahead::default();
    for input in held {
        collector.push(input);
    }
    collector.unexecuted_text()
}

/// What a run of held inputs writes to the pty (`WriteToLocalPty`, `GiveUpToPty`).
fn written_bytes(held: Vec<HeldInput>) -> String {
    held.into_iter()
        .map(|input| input.into_parts().1)
        .collect::<String>()
}

/// The issue's case: an Enter entry and a pty-bytes entry queued behind the hold, with the
/// hold's own keys around them, then each drop kind. Nothing is missing from the handed-back
/// text or from the write, and the order is the typed order.
#[test]
fn an_enter_and_pty_bytes_behind_a_dropped_hold_are_handed_on_by_every_path() {
    let segment = || {
        vec![
            EntryKind::Held {
                class: HELD_TYPED,
                text: "ls".to_owned(),
            },
            EntryKind::Enter,
            pty_bytes(b"pwd\r"),
            EntryKind::Text("echo hi".to_owned()),
        ]
    };
    let run = |then: SshDropThen| -> Vec<HeldInput> {
        segment()
            .into_iter()
            .flat_map(|kind| inputs(kind, then))
            .collect()
    };
    assert_eq!(
        handed_back_text(run(SshDropThen::HandBackText)),
        "ls\npwd\necho hi",
        "the Enter is a line break, the pty bytes their text and their Enter"
    );
    for then in [SshDropThen::WriteToLocalPty, SshDropThen::GiveUpToPty] {
        assert_eq!(
            written_bytes(run(then)),
            "ls\rpwd\recho hi",
            "{then:?}: the bytes they are, in typed order"
        );
    }
}

/// Each kind on each path, with the fix on.
#[test]
fn each_entry_kind_behind_a_dropped_hold_has_a_form_or_is_counted() {
    // The hold's own forms and typed text: handed on by every path, as before.
    for then in EVERY_THEN {
        for (class, expected) in [
            (HELD_TYPED, HeldInput::Typed("a".to_owned())),
            (HELD_KEY, HeldInput::Key("a".to_owned())),
            (HELD_PASTE, HeldInput::Paste("a".to_owned())),
            (HELD_SEQ, HeldInput::Seq("a".to_owned())),
        ] {
            let held = EntryKind::Held {
                class,
                text: "a".to_owned(),
            };
            assert_eq!(inputs(held, then), vec![expected], "{then:?}");
        }
        assert_eq!(
            inputs(EntryKind::Text("t".to_owned()), then),
            vec![HeldInput::Typed("t".to_owned())],
            "{then:?}"
        );
    }

    // A held signal: its byte at the give-up, never delivered anywhere else (OD-5).
    let signal = || EntryKind::Held {
        class: HELD_SIGNAL,
        text: "\u{3}".to_owned(),
    };
    assert_eq!(
        inputs(signal(), SshDropThen::GiveUpToPty),
        vec![HeldInput::Key("\u{3}".to_owned())]
    );
    for then in [
        SshDropThen::Discard,
        SshDropThen::HandBackText,
        SshDropThen::WriteToLocalPty,
    ] {
        assert_eq!(
            dropped_entry_inputs(signal(), then, true),
            DroppedEntry::SignalDiscarded,
            "{then:?}"
        );
    }

    // Control keys and sequences are the keys they are wherever the keys go on.
    for then in [
        SshDropThen::HandBackText,
        SshDropThen::WriteToLocalPty,
        SshDropThen::GiveUpToPty,
    ] {
        assert_eq!(
            inputs(EntryKind::Enter, then),
            vec![HeldInput::Key("\r".to_owned())],
            "{then:?}"
        );
        assert_eq!(
            inputs(EntryKind::Control("\u{15}"), then),
            vec![HeldInput::Key("\u{15}".to_owned())],
            "{then:?}"
        );
        assert_eq!(
            inputs(EntryKind::Seq("\u{1b}[A"), then),
            vec![HeldInput::Seq("\u{1b}[A".to_owned())],
            "{then:?}"
        );
        assert_eq!(
            inputs(editor_action(EditorAction::Enter), then),
            vec![HeldInput::Key("\r".to_owned())],
            "{then:?}"
        );
        assert_eq!(
            inputs(editor_text("ab"), then),
            vec![HeldInput::Typed("ab".to_owned())],
            "{then:?}"
        );
        // No form outside the editor, and passive pty-only bytes: counted, never silent.
        assert_eq!(
            dropped_entry_inputs(EntryKind::Raw(vec![0x1b, b'[', b'M']), then, true),
            DroppedEntry::NoForm("raw"),
            "{then:?}"
        );
    }

    // Pty bytes: one input with the bytes for a write; the keys they were for the text.
    for then in [SshDropThen::WriteToLocalPty, SshDropThen::GiveUpToPty] {
        assert_eq!(
            inputs(pty_bytes(b"a\x7f\r"), then),
            vec![HeldInput::Seq("a\u{7f}\r".to_owned())],
            "{then:?}"
        );
    }
    assert_eq!(
        inputs(pty_bytes(b"a\x7f\r"), SshDropThen::HandBackText),
        vec![
            HeldInput::Typed("a".to_owned()),
            HeldInput::Key("\u{7f}".to_owned()),
            HeldInput::Key("\r".to_owned()),
        ]
    );
    assert_eq!(
        dropped_entry_inputs(pty_bytes(&[0xff, 0xfe]), SshDropThen::HandBackText, true),
        DroppedEntry::NoForm("pty_bytes"),
        "bytes that are no text are counted, not turned into something else"
    );

    // A deliberate discard hands on only what it always did (the caller counts it).
    for kind in [
        EntryKind::Enter,
        pty_bytes(b"x"),
        EntryKind::Control("\u{15}"),
    ] {
        let tag = kind.tag();
        assert_eq!(
            dropped_entry_inputs(kind, SshDropThen::Discard, true),
            DroppedEntry::NoForm(tag)
        );
    }
}

/// `PHOSPHOR_SSH_DROP_ACCOUNTS_ENTRIES=0`: the behaviour before #1998. Only the give-up write
/// turns an Enter and pty bytes into what they are; the other paths skip them, and a control
/// key, a sequence and an editor key are skipped everywhere.
#[test]
fn the_kill_switch_restores_the_old_skips() {
    assert_eq!(
        dropped_entry_inputs(EntryKind::Enter, SshDropThen::GiveUpToPty, false),
        DroppedEntry::Inputs(vec![HeldInput::Key("\r".to_owned())])
    );
    assert_eq!(
        dropped_entry_inputs(pty_bytes(b"x"), SshDropThen::GiveUpToPty, false),
        DroppedEntry::Inputs(vec![HeldInput::Seq("x".to_owned())])
    );
    for then in [SshDropThen::HandBackText, SshDropThen::WriteToLocalPty] {
        assert_eq!(
            dropped_entry_inputs(EntryKind::Enter, then, false),
            DroppedEntry::NoForm("enter"),
            "{then:?}"
        );
        assert_eq!(
            dropped_entry_inputs(pty_bytes(b"x"), then, false),
            DroppedEntry::NoForm("pty_bytes"),
            "{then:?}"
        );
    }
    for then in EVERY_THEN {
        assert_eq!(
            dropped_entry_inputs(EntryKind::Control("\u{15}"), then, false),
            DroppedEntry::NoForm("control"),
            "{then:?}"
        );
        assert_eq!(
            dropped_entry_inputs(editor_text("ab"), then, false),
            DroppedEntry::NoForm("editor_key"),
            "{then:?}"
        );
        // Unchanged by the switch.
        assert_eq!(
            dropped_entry_inputs(EntryKind::Text("t".to_owned()), then, false),
            DroppedEntry::Inputs(vec![HeldInput::Typed("t".to_owned())]),
            "{then:?}"
        );
    }
}
