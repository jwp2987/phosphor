//! Fork-authored tests for the `/usage` and `/cost` report logic.
//!
//! There is nothing to port from `warp/master` here (AGENTS §5.10): upstream's `/usage` opens a
//! hosted billing pane and its `/cost` toggles a footer off server-computed credits, so Warp
//! has no coverage of context-window reporting or of pricing token counts against
//! user-configured rates. These assertions therefore encode *this fork's* intended behavior.
//!
//! The behaviour they exist to pin down is the one that is easiest to get quietly wrong: an
//! unconfigured rate must never render as a money figure, and must stay distinguishable from a
//! rate the user deliberately set to zero.

use super::*;

fn price(input: f64, output: f64) -> TokenPrice {
    TokenPrice {
        input_usd_per_million_tokens: input,
        output_usd_per_million_tokens: output,
        cache_read_usd_per_million_tokens: None,
        cache_write_usd_per_million_tokens: None,
    }
}

fn totals(label: &str, price: Option<TokenPrice>) -> ModelTokenTotals {
    ModelTokenTotals {
        label: label.to_owned(),
        uncached_input: 1_000_000,
        output: 100_000,
        cache_read: 0,
        cache_write: 0,
        price,
    }
}

// ---------------------------------------------------------------------------
// Price resolution: per-model overrides per-provider
// ---------------------------------------------------------------------------

#[test]
fn model_price_overrides_provider_price() {
    let mut provider = AgentProvider::new_empty();
    provider.token_price = Some(price(1.0, 2.0));
    let mut model = AgentProviderModel::from_id("m".to_owned());
    model.token_price = Some(price(3.0, 15.0));

    let resolved = model
        .resolved_token_price(&provider)
        .expect("model price should resolve");
    assert_eq!(resolved.input_usd_per_million_tokens, 3.0);
    assert_eq!(resolved.output_usd_per_million_tokens, 15.0);
}

#[test]
fn provider_price_is_the_default_when_model_has_none() {
    let mut provider = AgentProvider::new_empty();
    provider.token_price = Some(price(1.0, 2.0));
    let model = AgentProviderModel::from_id("m".to_owned());

    let resolved = model
        .resolved_token_price(&provider)
        .expect("provider price should resolve");
    assert_eq!(resolved.input_usd_per_million_tokens, 1.0);
    assert_eq!(resolved.output_usd_per_million_tokens, 2.0);
}

#[test]
fn no_price_anywhere_resolves_to_none() {
    let provider = AgentProvider::new_empty();
    let model = AgentProviderModel::from_id("m".to_owned());
    assert_eq!(model.resolved_token_price(&provider), None);
}

#[test]
fn model_price_overrides_provider_price_wholesale_including_cache_rates() {
    // The override is all-or-nothing by design: a model that names only input/output must not
    // inherit the provider's cache rates, or the reported figure would silently mix two
    // separately-entered price tables.
    let mut provider = AgentProvider::new_empty();
    provider.token_price = Some(TokenPrice {
        cache_read_usd_per_million_tokens: Some(0.1),
        ..price(1.0, 2.0)
    });
    let mut model = AgentProviderModel::from_id("m".to_owned());
    model.token_price = Some(price(3.0, 15.0));

    let resolved = model
        .resolved_token_price(&provider)
        .expect("model price should resolve");
    assert_eq!(resolved.cache_read_usd_per_million_tokens, None);
}

// ---------------------------------------------------------------------------
// Cost arithmetic
// ---------------------------------------------------------------------------

#[test]
fn cost_is_tokens_times_rate_per_million() {
    let total = totals("m", Some(price(3.0, 15.0)));
    // 1M input at $3 + 100k output at $15 = 3.00 + 1.50
    assert_eq!(total.cost_usd(), Some(4.5));
}

#[test]
fn unconfigured_price_yields_no_cost_rather_than_zero() {
    assert_eq!(totals("m", None).cost_usd(), None);
}

#[test]
fn explicit_zero_rate_yields_a_real_zero_cost() {
    // A user who prices a self-hosted endpoint at 0 has *answered* the question; that is a
    // different state from never having answered it, and only one of them is `None`.
    assert_eq!(totals("m", Some(price(0.0, 0.0))).cost_usd(), Some(0.0));
}

#[test]
fn cached_tokens_use_their_own_rate_when_configured() {
    let total = ModelTokenTotals {
        label: "m".to_owned(),
        uncached_input: 0,
        output: 0,
        cache_read: 1_000_000,
        cache_write: 1_000_000,
        price: Some(TokenPrice {
            cache_read_usd_per_million_tokens: Some(0.3),
            cache_write_usd_per_million_tokens: Some(3.75),
            ..price(3.0, 15.0)
        }),
    };
    assert_eq!(total.cost_usd(), Some(4.05));
    assert!(!total.used_input_rate_for_cache());
}

#[test]
fn cached_tokens_fall_back_to_the_input_rate_and_say_so() {
    let total = ModelTokenTotals {
        label: "m".to_owned(),
        uncached_input: 0,
        output: 0,
        cache_read: 1_000_000,
        cache_write: 0,
        price: Some(price(3.0, 15.0)),
    };
    assert_eq!(total.cost_usd(), Some(3.0));
    assert!(
        total.used_input_rate_for_cache(),
        "the report must disclose that cached tokens were billed at the input rate"
    );
}

#[test]
fn cache_fallback_is_not_flagged_when_there_are_no_cached_tokens() {
    assert!(!totals("m", Some(price(3.0, 15.0))).used_input_rate_for_cache());
}

#[test]
fn total_tokens_sums_all_four_buckets() {
    let total = ModelTokenTotals {
        label: "m".to_owned(),
        uncached_input: 1,
        output: 2,
        cache_read: 4,
        cache_write: 8,
        price: None,
    };
    assert_eq!(total.total_tokens(), 15);
}

// ---------------------------------------------------------------------------
// `/cost` report rendering
// ---------------------------------------------------------------------------

#[test]
fn cost_report_shows_a_dollar_figure_when_every_model_is_priced() {
    let outcome = format_cost_report(&[totals("Sonnet (Anthropic)", Some(price(3.0, 15.0)))]);
    assert!(!outcome.is_unavailable());
    let message = outcome.message();
    assert!(message.contains("$4.50"), "{message}");
    assert!(message.contains("Sonnet (Anthropic)"), "{message}");
    assert!(message.contains("1,000,000 in"), "{message}");
    assert!(message.contains("100,000 out"), "{message}");
    assert!(message.contains("$3.00/$15.00 per 1M"), "{message}");
}

#[test]
fn cost_report_without_a_rate_reports_tokens_and_never_a_money_figure() {
    let outcome = format_cost_report(&[totals("Sonnet (Anthropic)", None)]);
    assert!(
        outcome.is_unavailable(),
        "an unpriced conversation is not a cost report"
    );
    let message = outcome.message();
    assert!(message.contains("no token price is configured"), "{message}");
    // Names the model so the user knows which row to fill in.
    assert!(message.contains("Sonnet (Anthropic)"), "{message}");
    assert!(message.contains("1,000,000 in"), "{message}");
    assert!(message.contains("Settings > AI > Agent providers"), "{message}");
    assert!(
        !message.contains('$'),
        "an unconfigured rate must not produce a dollar figure: {message}"
    );
}

#[test]
fn cost_report_with_an_explicit_zero_rate_does_show_zero_dollars() {
    // The counterpart to the previous test: `$0.0000` is only ever printed because the user
    // said the endpoint is free, never because nobody said anything.
    let outcome = format_cost_report(&[totals("Local Llama", Some(price(0.0, 0.0)))]);
    assert!(!outcome.is_unavailable());
    assert!(outcome.message().contains("$0.0000"), "{}", outcome.message());
}

#[test]
fn cost_report_calls_out_partially_priced_conversations() {
    let outcome = format_cost_report(&[
        totals("Sonnet (Anthropic)", Some(price(3.0, 15.0))),
        totals("Haiku (Anthropic)", None),
    ]);
    assert!(!outcome.is_unavailable());
    let message = outcome.message();
    assert!(message.contains("$4.50"), "{message}");
    assert!(
        message.contains("so far, excluding Haiku (Anthropic) with no rate configured"),
        "a partial total must not read as the whole bill: {message}"
    );
}

#[test]
fn cost_report_discloses_the_cache_rate_fallback() {
    let outcome = format_cost_report(&[ModelTokenTotals {
        label: "Sonnet".to_owned(),
        uncached_input: 0,
        output: 0,
        cache_read: 1_000_000,
        cache_write: 0,
        price: Some(price(3.0, 15.0)),
    }]);
    assert!(
        outcome
            .message()
            .contains("Cached tokens were billed at the plain input rate"),
        "{}",
        outcome.message()
    );
}

#[test]
fn cost_report_with_no_recorded_usage_explains_why() {
    let outcome = format_cost_report(&[]);
    assert!(outcome.is_unavailable());
    assert!(
        outcome.message().contains("no token usage has been recorded"),
        "{}",
        outcome.message()
    );
}

#[test]
fn cost_report_breakdown_lists_cache_buckets_only_when_non_zero() {
    let with_cache = format_cost_report(&[ModelTokenTotals {
        label: "m".to_owned(),
        uncached_input: 10,
        output: 20,
        cache_read: 30,
        cache_write: 40,
        price: Some(price(3.0, 15.0)),
    }]);
    assert!(with_cache.message().contains("30 cache-read"), "{}", with_cache.message());
    assert!(with_cache.message().contains("40 cache-write"), "{}", with_cache.message());

    let without_cache = format_cost_report(&[totals("m", Some(price(3.0, 15.0)))]);
    assert!(!without_cache.message().contains("cache-read"), "{}", without_cache.message());
    assert!(!without_cache.message().contains("cache-write"), "{}", without_cache.message());
}

// ---------------------------------------------------------------------------
// `/usage` report rendering
// ---------------------------------------------------------------------------

#[test]
fn context_usage_report_states_used_and_remaining() {
    let outcome = format_context_usage_report(0.183, Some("gpt-5 (OpenAI)"), Some(200_000));
    assert!(!outcome.is_unavailable());
    let message = outcome.message();
    assert!(message.contains("18% used"), "{message}");
    assert!(message.contains("82% remaining"), "{message}");
    assert!(message.contains("gpt-5 (OpenAI)"), "{message}");
    assert!(message.contains("200,000 token context window"), "{message}");
}

#[test]
fn context_usage_report_rounds_the_same_way_the_footers_do() {
    // Matches `warp_tui::usage::format_context_usage` / the GUI usage view: round, don't
    // truncate, so the slash command can never disagree with the statusline.
    assert!(
        format_context_usage_report(0.995, None, None)
            .message()
            .contains("100% used")
    );
    assert!(
        format_context_usage_report(0.004, None, None)
            .message()
            .contains("0% used")
    );
}

#[test]
fn context_usage_report_treats_zero_as_not_reported_yet() {
    let outcome = format_context_usage_report(0.0, Some("gpt-5"), Some(200_000));
    assert!(outcome.is_unavailable());
    let message = outcome.message();
    assert!(message.contains("No context-window usage reported yet"), "{message}");
    assert!(message.contains("send a message first"), "{message}");
    assert!(
        !message.contains("0% used"),
        "the sentinel must not be rendered as a real 0% reading: {message}"
    );
}

#[test]
fn context_usage_report_without_a_window_names_the_setting_to_fill_in() {
    // No configured context window means the send path never emits usage metadata at all, so
    // "send a message" would be useless advice.
    let outcome = format_context_usage_report(0.0, Some("gpt-5"), None);
    assert!(outcome.is_unavailable());
    assert!(
        outcome
            .message()
            .contains("set a context window for this model"),
        "{}",
        outcome.message()
    );
}

#[test]
fn context_usage_report_omits_the_suffix_when_nothing_is_known_about_the_model() {
    let outcome = format_context_usage_report(0.5, None, None);
    assert_eq!(
        outcome.message(),
        "Context window: 50% used, 50% remaining"
    );
}

// ---------------------------------------------------------------------------
// Formatting helpers
// ---------------------------------------------------------------------------

#[test]
fn usd_keeps_four_decimals_below_a_dollar() {
    // Agent turns routinely cost fractions of a cent; rounding those to `$0.00` would read as
    // free, which is exactly the confusion these commands exist to avoid.
    assert_eq!(format_usd(0.004123), "$0.0041");
    assert_eq!(format_usd(0.0), "$0.0000");
    assert_eq!(format_usd(1.5), "$1.50");
    assert_eq!(format_usd(12.345), "$12.35");
}

#[test]
fn rates_render_like_a_published_price_list() {
    assert_eq!(format_rate(3.0), "$3.00");
    assert_eq!(format_rate(0.3), "$0.30");
    assert_eq!(format_rate(0.075), "$0.075");
    assert_eq!(format_rate(15.0), "$15.00");
}

#[test]
fn thousands_groups_from_the_right() {
    assert_eq!(thousands(0), "0");
    assert_eq!(thousands(999), "999");
    assert_eq!(thousands(1_000), "1,000");
    assert_eq!(thousands(12_340), "12,340");
    assert_eq!(thousands(1_234_567), "1,234,567");
}
