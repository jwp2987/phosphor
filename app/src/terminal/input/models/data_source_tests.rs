use std::collections::HashMap;

use crate::ai::llms::{LLMContextWindow, LLMUsageMetadata};

use super::*;

/// Minimal `LLMInfo` fixture. Only `id`/`display_name` matter for
/// [`ModelSelectorDataSource::order_model_choices`], which is the only thing
/// under test here.
fn llm(id: &str, display_name: &str) -> LLMInfo {
    LLMInfo {
        display_name: display_name.to_string(),
        base_model_name: display_name.to_string(),
        id: id.into(),
        reasoning_level: None,
        usage_metadata: LLMUsageMetadata {
            request_multiplier: 1,
            credit_multiplier: None,
        },
        description: None,
        disable_reason: None,
        vision_supported: false,
        spec: None,
        provider: LLMProvider::Unknown,
        host_configs: HashMap::new(),
        discount_percentage: None,
        context_window: LLMContextWindow::default(),
    }
}

fn ordered_ids<'a>(choices: Vec<&'a LLMInfo>) -> Vec<String> {
    ModelSelectorDataSource::order_model_choices(choices)
        .into_iter()
        .map(|llm| llm.id.to_string())
        .collect()
}

#[test]
fn order_model_choices_moves_auto_by_id_to_the_front() {
    let claude = llm("claude-4", "Claude 4");
    let auto = llm("auto", "Auto");
    let gpt = llm("gpt-5", "GPT-5");

    assert_eq!(
        ordered_ids(vec![&claude, &auto, &gpt]),
        vec!["auto", "claude-4", "gpt-5"],
        "the auto model must sort first even though it wasn't first in the input"
    );
}

#[test]
fn order_model_choices_moves_auto_by_display_name_to_the_front() {
    // `is_auto` matches on display_name OR id containing "auto" — a model
    // whose id doesn't say "auto" but whose display name does must still
    // sort first (mirrors the pin's `is_auto`, ported unchanged into this
    // fork at `app/src/ai/execution_profiles/model_menu_items.rs`).
    let claude = llm("claude-4", "Claude 4");
    let smart_auto = llm("model-123", "Smart Auto");

    assert_eq!(
        ordered_ids(vec![&claude, &smart_auto]),
        vec!["model-123", "claude-4"]
    );
}

#[test]
fn order_model_choices_is_case_insensitive() {
    let claude = llm("claude-4", "Claude 4");
    let auto = llm("AUTO", "AUTO");

    assert_eq!(ordered_ids(vec![&claude, &auto]), vec!["AUTO", "claude-4"]);
}

#[test]
fn order_model_choices_preserves_relative_order_within_each_bucket() {
    let auto_1 = llm("auto-1", "Auto 1");
    let claude = llm("claude-4", "Claude 4");
    let auto_2 = llm("auto-2", "Auto 2");
    let gpt = llm("gpt-5", "GPT-5");

    assert_eq!(
        ordered_ids(vec![&auto_1, &claude, &auto_2, &gpt]),
        vec!["auto-1", "auto-2", "claude-4", "gpt-5"],
        "both auto models must come first, in their original relative order, \
         followed by the non-auto models in their original relative order"
    );
}

#[test]
fn order_model_choices_is_a_no_op_when_no_model_is_auto() {
    let claude = llm("claude-4", "Claude 4");
    let gpt = llm("gpt-5", "GPT-5");

    assert_eq!(ordered_ids(vec![&claude, &gpt]), vec!["claude-4", "gpt-5"]);
}

#[test]
fn order_model_choices_handles_an_empty_list() {
    assert_eq!(ordered_ids(vec![]), Vec::<String>::new());
}
