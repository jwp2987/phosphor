use super::*;

#[test]
fn model_default_table() {
    let cases: &[(&str, bool)] = &[
        // Large hosted models: snapshots on.
        ("claude-opus-4-1", true),
        ("claude-sonnet-4-5", true),
        ("claude-3-5-sonnet-20241022", true),
        ("anthropic/claude-sonnet-4", true),
        ("us.anthropic.claude-opus-4-1-v1:0", true),
        ("gpt-4", true),
        ("gpt-4o", true),
        ("gpt-4.1", true),
        ("gpt-5", true),
        ("gpt-5-codex", true),
        ("openai/gpt-5", true),
        ("o1", true),
        ("o3", true),
        ("o3-pro", true),
        ("gemini-2.5-pro", true),
        ("google/gemini-1.5-pro-002", true),
        ("grok-4", true),
        ("grok-code-fast-1", true),
        ("GPT-5", true),
        // Small / fast tiers inside those families: snapshots off.
        ("gpt-5-mini", false),
        ("gpt-5-nano", false),
        ("gpt-4o-mini", false),
        ("gpt-4.1-nano", false),
        ("o3-mini", false),
        ("o4-mini", false),
        ("claude-haiku-4-5", false),
        ("claude-3-5-haiku-20241022", false),
        ("gemini-2.5-flash", false),
        ("gemini-2.5-flash-lite", false),
        ("grok-3-mini", false),
        // Everything else, including the model this exists for.
        ("deepseek-v4-flash", false),
        ("deepseek-chat", false),
        ("deepseek-reasoner", false),
        ("qwen3-coder", false),
        ("llama3:70b", false),
        ("gpt-oss:20b", false),
        ("gpt-oss-120b", false),
        ("gemini-2.5-nonsense", false),
        ("claude-instant", false),
        ("omni-model", false),
        ("o", false),
        ("", false),
    ];
    for (id, expected) in cases {
        assert_eq!(model_id_handles_command_snapshots(id), *expected, "{id}");
    }
}

#[test]
fn local_endpoints_are_detected() {
    for url in [
        "http://localhost:11434",
        "http://127.0.0.1:8080/v1",
        "http://[::1]:11434",
        "http://0.0.0.0:8000",
        "http://192.168.1.20:11434",
        "http://10.0.0.5/v1",
        "http://172.16.3.4:1234",
        "http://172.31.255.1",
        "http://box.local:11434/v1",
        "http://user:pw@localhost:9/v1",
    ] {
        assert!(is_local_endpoint(url), "{url}");
    }
    for url in [
        "https://api.openai.com/v1",
        "https://api.anthropic.com",
        "http://172.32.0.1",
        "http://172.15.0.1",
        "https://localhost.example.com/v1",
        "https://openrouter.ai/api/v1",
        "",
    ] {
        assert!(!is_local_endpoint(url), "{url}");
    }
}

#[test]
fn local_or_ollama_overrides_a_big_model_id() {
    assert!(default_handles_command_snapshots(
        "claude-sonnet-4-5",
        "https://api.anthropic.com",
        false
    ));
    assert!(!default_handles_command_snapshots(
        "claude-sonnet-4-5",
        "http://localhost:11434",
        false
    ));
    assert!(!default_handles_command_snapshots(
        "gpt-5",
        "https://example.com",
        true
    ));
}
