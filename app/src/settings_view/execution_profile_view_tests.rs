use super::*;

/// #1114: an unset slot follows the `/model` choice at request time, so the card says so.
#[test]
fn unset_slot_with_a_model_override_names_the_model_and_the_slash_model_note() {
    crate::i18n::init(Some("en"));
    let label = unset_slot_label("Ollama / gpt-oss:20b".to_string(), true);
    assert!(label.contains("Ollama / gpt-oss:20b"), "{label}");
    assert!(label.contains("/model"), "{label}");
}

#[test]
fn unset_slot_without_an_override_names_the_base_model_and_no_note() {
    crate::i18n::init(Some("en"));
    let label = unset_slot_label("Ollama / gemma4:31b".to_string(), false);
    assert!(label.contains("Ollama / gemma4:31b"), "{label}");
    assert!(label.contains("Same as base"), "{label}");
    assert!(!label.contains("/model"), "{label}");
}
