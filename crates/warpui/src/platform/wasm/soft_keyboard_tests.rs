use super::*;

#[test]
fn test_map_insert_text() {
    let event = HiddenInputEvent::InsertText {
        text: "hello".to_string(),
    };
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::TextInserted(s)) if s == "hello"));
}

#[test]
fn test_map_backspace() {
    let event = HiddenInputEvent::Backspace;
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::Backspace)));
}

#[test]
fn test_map_delete() {
    let event = HiddenInputEvent::Delete;
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::Backspace)));
}

#[test]
fn test_map_blur() {
    let event = HiddenInputEvent::Blur;
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::KeyboardDismissed)));
}

#[test]
fn test_map_keydown_enter() {
    let event = HiddenInputEvent::KeyDown {
        key: "Enter".to_string(),
    };
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::KeyDown(key)) if key == "Enter"));
}

#[test]
fn test_map_unicode_insert() {
    let event = HiddenInputEvent::InsertText {
        text: "👋🌍".to_string(),
    };
    let result = map_hidden_input_event(event);
    assert!(matches!(result, Some(SoftKeyboardInput::TextInserted(s)) if s == "👋🌍"));
}

// ============================================================================
// Log redaction
//
// `SoftKeyboardInput`'s derived `Debug` prints the user's typed text verbatim.
// It used to be `{:?}`-formatted onto both a per-keystroke `log::debug!` and the
// send-failure `log::error!` in `windowing/winit/event_loop/mod.rs`. `log_shape()`
// is what those lines format now, so it must never echo a string payload.
// ============================================================================

#[test]
fn log_shape_omits_inserted_text() {
    let secret = "hunter2 correct horse battery staple";
    let shape = SoftKeyboardInput::TextInserted(secret.to_string()).log_shape();
    assert!(
        !shape.contains(secret),
        "log_shape leaked the typed text: {shape}"
    );
    assert!(!shape.contains("hunter"), "log_shape leaked a fragment: {shape}");
    // The variant and a length still have to survive, or the diagnostic is useless.
    assert_eq!(shape, format!("TextInserted({} chars)", secret.chars().count()));
}

#[test]
fn log_shape_counts_characters_not_bytes() {
    // A byte length would let a multi-byte-vs-ASCII comparison narrow down the
    // content; a char count is also just the honest answer to "how much was typed".
    let shape = SoftKeyboardInput::TextInserted("👋🌍".to_string()).log_shape();
    assert_eq!(shape, "TextInserted(2 chars)");
}

#[test]
fn log_shape_omits_key_name() {
    let shape = SoftKeyboardInput::KeyDown("Enter".to_string()).log_shape();
    assert!(
        !shape.contains("Enter"),
        "log_shape leaked the key name: {shape}"
    );
    assert_eq!(shape, "KeyDown(5 chars)");
}

#[test]
fn log_shape_of_payloadless_variants_is_the_variant_name() {
    assert_eq!(SoftKeyboardInput::Backspace.log_shape(), "Backspace");
    assert_eq!(
        SoftKeyboardInput::KeyboardDismissed.log_shape(),
        "KeyboardDismissed"
    );
}
