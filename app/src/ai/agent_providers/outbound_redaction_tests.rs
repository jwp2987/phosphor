//! Tests for outbound secret redaction (#1469): a secret in every kind of content is gone from
//! the serialized request when redaction is on, and the request is untouched when it is off.
//!
//! `SECRETS_REGEX` and the enable flag are process-global, so every test is `#[serial]` and
//! resets both through [`Armed`].

use genai::chat::{ChatMessage, ChatRequest, ContentPart, ToolCall, ToolResponse};
use regex::Regex;
use serde_json::json;
use serial_test::serial;

use super::outbound_redaction::{redact_chat_request, redact_json_strings, set_enabled};
use crate::terminal::model::secrets;

pub(crate) const SECRET: &str = "SECRET-4242";

/// Configures the detector with the test pattern and the global switch; resets on drop.
pub(crate) struct Armed;

impl Armed {
    pub(crate) fn on() -> Self {
        secrets::set_user_and_enterprise_secret_regexes(
            [&Regex::new(r"SECRET-[0-9]{4}").expect("valid regex")],
            std::iter::empty(),
        );
        set_enabled(true);
        Armed
    }

    pub(crate) fn off() -> Self {
        let armed = Self::on();
        set_enabled(false);
        armed
    }
}

impl Drop for Armed {
    fn drop(&mut self) {
        set_enabled(false);
        secrets::set_user_and_enterprise_secret_regexes(std::iter::empty(), std::iter::empty());
    }
}

fn request_with_secret_everywhere() -> ChatRequest {
    let call = ToolCall {
        call_id: "c1".to_owned(),
        fn_name: "run_shell_command".to_owned(),
        fn_arguments: json!({"command": format!("curl -H 'x: {SECRET}'"), "nested": [format!("a {SECRET}")], "n": 3}),
        thought_signatures: None,
    };
    let mut assistant = ChatMessage::assistant("running it");
    assistant.content.push(ContentPart::ToolCall(call));
    ChatRequest::from_messages(vec![
        ChatMessage::user(format!("my key is {SECRET}")),
        assistant,
        ChatMessage::from(ToolResponse::new("c1", format!("output: {SECRET}"))),
        ChatMessage::assistant(format!("summary of earlier turns mentioning {SECRET}")),
    ])
    .with_system(format!("project rules: token {SECRET}"))
}

fn wire(req: &ChatRequest) -> String {
    serde_json::to_string(req).expect("serializes")
}

#[test]
#[serial]
fn every_content_kind_is_redacted_in_the_serialized_request() {
    let _armed = Armed::on();
    let mut req = request_with_secret_everywhere();
    assert!(
        wire(&req).contains(SECRET),
        "fixture must contain the secret"
    );
    redact_chat_request(&mut req);
    let wire = wire(&req);
    assert!(!wire.contains(SECRET), "secret survived redaction: {wire}");
    // Structure and non-secret text survive.
    assert!(wire.contains("running it") && wire.contains("run_shell_command"));
    assert!(wire.contains(&"*".repeat(SECRET.len())));
}

#[test]
#[serial]
fn redaction_leaves_binary_and_signed_reasoning_alone() {
    let _armed = Armed::on();
    let mut msg = ChatMessage::assistant("x");
    msg.content
        .push(ContentPart::ReasoningContent(format!("thinking {SECRET}")));
    msg.content
        .push(ContentPart::ThoughtSignature(format!("sig {SECRET}")));
    let mut req = ChatRequest::from_messages(vec![msg]);
    redact_chat_request(&mut req);
    // Provider-signed blocks must round-trip byte-for-byte or the provider rejects the turn.
    assert!(wire(&req).matches(SECRET).count() == 2);
}

#[test]
#[serial]
fn json_strings_are_redacted_but_keys_and_numbers_are_not() {
    let _armed = Armed::on();
    let mut v = json!({ "k": [SECRET, 7, {"deep": format!("x{SECRET}")}] });
    redact_json_strings(&mut v);
    assert!(!v.to_string().contains(SECRET));
    assert_eq!(v["k"][1], 7);
}

#[test]
#[serial]
fn embeddings_and_rerank_inputs_are_redacted_when_on_and_untouched_when_off() {
    use super::embeddings::{prepare_embedding_input, prepare_rerank_input};
    {
        let _armed = Armed::on();
        let texts = prepare_embedding_input(vec![format!("const K = \"{SECRET}\";")]);
        assert!(!texts.concat().contains(SECRET));
        let (q, docs) =
            prepare_rerank_input(&format!("where is {SECRET}"), vec![format!("k={SECRET}")]);
        assert!(!q.contains(SECRET) && !docs.concat().contains(SECRET));
    }
    let _off = Armed::off();
    let texts = prepare_embedding_input(vec![format!("const K = \"{SECRET}\";")]);
    assert!(texts.concat().contains(SECRET));
    let (q, docs) =
        prepare_rerank_input(&format!("where is {SECRET}"), vec![format!("k={SECRET}")]);
    assert!(q.contains(SECRET) && docs.concat().contains(SECRET));
}
