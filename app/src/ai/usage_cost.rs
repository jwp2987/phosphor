//! Backing logic for the `/usage` and `/cost` slash commands, shared verbatim by the GUI
//! (`terminal::input::slash_commands`) and the TUI (`warp_tui::terminal_session_view`) so the
//! two surfaces cannot drift — see AGENTS §5.9.
//!
//! # Sanctioned BYOP divergence from Warp (AGENTS §5.10)
//!
//! Both commands mean something different here than upstream, and the reason is the one
//! divergence this fork is allowed: Warp's versions are surfaces onto Warp's *hosted
//! subscription billing*, which this fork does not have and will never have.
//!
//! * Warp's `/usage` dispatches `OpenBillingAndUsagePane` — a pane showing the user's plan,
//!   credit balance and quota against Warp's servers. There is no plan, no credit balance and
//!   no quota here: the user pays their own provider directly. The fork's honest equivalent of
//!   "how much am I using" is the one budget a BYOP conversation actually spends against, its
//!   model's context window — which the fork already tracks
//!   ([`AIConversation::context_window_usage`]) and already renders in both footers. `/usage`
//!   is a new *presentation* of that existing number, not a second accounting path.
//! * Warp's `/cost` toggles a usage footer whose money figure comes from
//!   `cost_in_cents`/`credits_spent` computed **server-side** by Warp, which knows its own
//!   price list. A BYOP provider hands back token counts and nothing else — no adapter in this
//!   fork has ever seen a dollar figure. So `/cost` multiplies the token counts the provider
//!   did report by the rates the *user* configured for that provider/model
//!   ([`TokenPrice`]). The user is the one holding the invoice, so the user is the one who
//!   states the rate.
//!
//! This is acceptable rather than a regression because in both cases the upstream surface is
//! made of cloud-subscription data that has no local counterpart, and the replacement reports
//! the same *question* ("how much am I using / spending") from the only inputs a BYOP build
//! legitimately has. Where an input is missing, these functions say so in words — they never
//! substitute a default rate, and never render an unconfigured model as `$0.00`, because a
//! plausible-looking wrong money figure is worse than no figure at all.

use settings::Setting;
use warpui::{AppContext, SingletonEntity};

use crate::ai::agent::conversation::AIConversation;
use crate::ai::agent_providers::llm_id;
use crate::ai::llms::LLMPreferences;
use crate::settings::{AISettings, AgentProvider, AgentProviderModel, TokenPrice};

/// How a report should be surfaced. The two variants map onto each surface's existing
/// feedback affordances: an error toast / transient hint versus a plain toast / success hint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UsageCostOutcome {
    /// The command could not report anything, and the string says why.
    Unavailable(String),
    /// A report to show the user.
    Report(String),
}

impl UsageCostOutcome {
    /// The message text, whichever variant this is.
    pub fn message(&self) -> &str {
        match self {
            Self::Unavailable(message) | Self::Report(message) => message,
        }
    }

    /// Whether this should be surfaced as an error rather than as information.
    pub fn is_unavailable(&self) -> bool {
        match self {
            Self::Unavailable(_) => true,
            Self::Report(_) => false,
        }
    }
}

/// One model's accumulated token counts for a conversation, plus the price (if any) the user
/// configured for it.
///
/// The counts mirror the four buckets the provider stream reports and the proto splits out;
/// `uncached_input` deliberately excludes the two cache buckets so the three never
/// double-count (see the `token_usage` assembly in `agent_providers::chat_stream`).
#[derive(Debug, Clone, PartialEq)]
pub struct ModelTokenTotals {
    /// What to call this model in the report: `"<model name> (<provider name>)"` when the id
    /// resolves to configured settings, otherwise the raw model id from the stream.
    pub label: String,
    /// Input (prompt) tokens that were *not* served from, or written to, the prompt cache.
    pub uncached_input: u64,
    /// Output (completion) tokens.
    pub output: u64,
    /// Input tokens served from the provider's prompt cache.
    pub cache_read: u64,
    /// Input tokens written into the provider's prompt cache.
    pub cache_write: u64,
    /// The resolved price: the model's own rate, else its provider's default, else `None`.
    pub price: Option<TokenPrice>,
}

impl ModelTokenTotals {
    /// Every token this model was billed for.
    pub fn total_tokens(&self) -> u64 {
        self.uncached_input + self.output + self.cache_read + self.cache_write
    }

    /// This model's spend in USD, or `None` when no rate is configured for it.
    ///
    /// `None` is emphatically not `Some(0.0)`: an unconfigured model has an *unknown* cost,
    /// whereas a genuinely free endpoint the user has priced at `0.0` really did cost nothing,
    /// and the report distinguishes the two.
    pub fn cost_usd(&self) -> Option<f64> {
        let price = self.price?;
        let (cache_read_rate, _) = price.cache_read_rate();
        let (cache_write_rate, _) = price.cache_write_rate();
        let dollars = self.uncached_input as f64 * price.input_usd_per_million_tokens
            + self.output as f64 * price.output_usd_per_million_tokens
            + self.cache_read as f64 * cache_read_rate
            + self.cache_write as f64 * cache_write_rate;
        Some(dollars / 1_000_000.0)
    }

    /// Whether cached tokens were billed at the plain input rate because no cache rate was
    /// configured. Providers that discount cache reads steeply (Anthropic bills them at 0.1x)
    /// make that fallback over-report, so the report calls it out rather than hiding it.
    pub fn used_input_rate_for_cache(&self) -> bool {
        let Some(price) = self.price else {
            return false;
        };
        (self.cache_read > 0 && !price.cache_read_rate().1)
            || (self.cache_write > 0 && !price.cache_write_rate().1)
    }
}

// ---------------------------------------------------------------------------
// `/usage` — context-window occupancy
// ---------------------------------------------------------------------------

/// Builds the `/usage` report for `conversation` (`None` = no active conversation).
///
/// Reuses [`AIConversation::context_window_usage`] — the same 0.0–1.0 fraction the GUI usage
/// footer and the TUI statusline already display — rather than recomputing occupancy.
pub fn context_usage_report(conversation: Option<&AIConversation>, ctx: &AppContext) -> UsageCostOutcome {
    let Some(conversation) = conversation else {
        return UsageCostOutcome::Unavailable(
            "Cannot show context usage: no active conversation".to_owned(),
        );
    };
    let active = active_byop_model(ctx);
    let model_label = active
        .as_ref()
        .map(|(provider, model)| model_label(provider, model));
    let context_window = active
        .as_ref()
        .map(|(_, model)| model.context_window)
        .filter(|window| *window > 0);
    format_context_usage_report(
        conversation.context_window_usage(),
        model_label.as_deref(),
        context_window,
    )
}

/// Renders the `/usage` message. `fraction` is the 0.0–1.0 occupancy; `0.0` is the
/// "nothing reported yet" sentinel the rest of the app already treats it as (the footers hide
/// themselves on it), so it gets an explanation rather than a misleading "0% used".
pub fn format_context_usage_report(
    fraction: f32,
    model_label: Option<&str>,
    context_window: Option<u32>,
) -> UsageCostOutcome {
    let suffix = match (model_label, context_window) {
        (Some(label), Some(window)) => {
            format!(" — {label}, {} token context window", thousands(window as u64))
        }
        (Some(label), None) => format!(" — {label}"),
        (None, Some(window)) => format!(" — {} token context window", thousands(window as u64)),
        (None, None) => String::new(),
    };

    if fraction <= 0.0 {
        let hint = match context_window {
            Some(_) => "send a message first",
            // No configured window means the send path never emits usage metadata at all, so
            // naming that as the fix is the actionable half of the answer.
            None => {
                "set a context window for this model in Settings > AI > Agent providers, then send a message"
            }
        };
        return UsageCostOutcome::Unavailable(format!(
            "No context-window usage reported yet: {hint}{suffix}"
        ));
    }

    let used = (fraction.clamp(0.0, 1.0) * 100.0).round() as u32;
    let remaining = 100 - used.min(100);
    UsageCostOutcome::Report(format!(
        "Context window: {used}% used, {remaining}% remaining{suffix}"
    ))
}

// ---------------------------------------------------------------------------
// `/cost` — token spend at the user's own configured rates
// ---------------------------------------------------------------------------

/// Builds the `/cost` report for `conversation` (`None` = no active conversation).
///
/// The leading guards mirror the shape of Warp's own `/cost` arm (no active conversation /
/// empty conversation / conversation in progress) so the command refuses in the same
/// situations upstream refuses; what it does once it gets past them is the BYOP divergence
/// documented at the top of this module.
pub fn conversation_cost_report(
    conversation: Option<&AIConversation>,
    ctx: &AppContext,
) -> UsageCostOutcome {
    let Some(conversation) = conversation else {
        return UsageCostOutcome::Unavailable(
            "Cannot show conversation cost: no active conversation".to_owned(),
        );
    };
    if conversation.is_empty() {
        return UsageCostOutcome::Unavailable(
            "Cannot show conversation cost: conversation is empty".to_owned(),
        );
    }
    if !conversation.status().is_done() {
        return UsageCostOutcome::Unavailable(
            "Cannot show conversation cost: conversation is in progress".to_owned(),
        );
    }

    let providers = AISettings::as_ref(ctx).agent_providers.value().clone();
    if providers.is_empty() {
        return UsageCostOutcome::Unavailable(
            "Cannot show conversation cost: no provider is configured. Add one in Settings > AI > Agent providers."
                .to_owned(),
        );
    }

    let totals: Vec<ModelTokenTotals> = conversation
        .total_token_usage()
        .into_iter()
        .map(|usage| {
            let resolved = resolve_model(&providers, &usage.model_id);
            let label = match &resolved {
                Some((provider, model)) => model_label(provider, model),
                None => usage.model_id.clone(),
            };
            let price = resolved
                .as_ref()
                .and_then(|(provider, model)| model.resolved_token_price(provider));
            ModelTokenTotals {
                label,
                uncached_input: u64::from(usage.total_input),
                output: u64::from(usage.output),
                cache_read: u64::from(usage.input_cache_read),
                cache_write: u64::from(usage.input_cache_write),
                price,
            }
        })
        .collect();

    format_cost_report(&totals)
}

/// Renders the `/cost` message from per-model totals.
///
/// Three shapes, and the distinction between them is the whole point of the command:
/// every model priced → a dollar figure; none priced → token counts and an explicit statement
/// that no rate is configured, naming what to set; some priced → the partial dollar figure
/// *plus* the unpriced models called out, so the total is never mistaken for the whole bill.
pub fn format_cost_report(totals: &[ModelTokenTotals]) -> UsageCostOutcome {
    if totals.is_empty() {
        return UsageCostOutcome::Unavailable(
            "Cannot show conversation cost: no token usage has been recorded for this conversation. Provider token counts are kept per session, so a conversation restored from a previous run starts empty."
                .to_owned(),
        );
    }

    let lines: Vec<String> = totals.iter().map(format_model_line).collect();
    let breakdown = lines.join("; ");
    let priced: Vec<&ModelTokenTotals> = totals
        .iter()
        .filter(|total| total.price.is_some())
        .collect();
    let unpriced: Vec<&ModelTokenTotals> = totals
        .iter()
        .filter(|total| total.price.is_none())
        .collect();
    let cache_caveat = if totals.iter().any(ModelTokenTotals::used_input_rate_for_cache) {
        " Cached tokens were billed at the plain input rate: no cache rate is configured, so this over-reports on providers that discount cache reads."
    } else {
        ""
    };
    let configure_hint = "Set input/output USD per 1M tokens on the model (or as the provider default) in Settings > AI > Agent providers.";

    if priced.is_empty() {
        return UsageCostOutcome::Unavailable(format!(
            "Cost unavailable: no token price is configured for the model(s) used. Tokens this conversation — {breakdown}. {configure_hint}"
        ));
    }

    let subtotal: f64 = priced
        .iter()
        .filter_map(|total| total.cost_usd())
        .sum::<f64>();
    if unpriced.is_empty() {
        UsageCostOutcome::Report(format!(
            "Cost this conversation: {} — {breakdown}.{cache_caveat}",
            format_usd(subtotal)
        ))
    } else {
        let unpriced_names: Vec<&str> = unpriced.iter().map(|total| total.label.as_str()).collect();
        UsageCostOutcome::Report(format!(
            "Cost this conversation: {} so far, excluding {} with no rate configured — {breakdown}.{cache_caveat} {configure_hint}",
            format_usd(subtotal),
            unpriced_names.join(", ")
        ))
    }
}

fn format_model_line(total: &ModelTokenTotals) -> String {
    let mut parts = vec![format!("{} in", thousands(total.uncached_input))];
    if total.cache_read > 0 {
        parts.push(format!("{} cache-read", thousands(total.cache_read)));
    }
    if total.cache_write > 0 {
        parts.push(format!("{} cache-write", thousands(total.cache_write)));
    }
    parts.push(format!("{} out", thousands(total.output)));
    let tokens = parts.join(" + ");

    match total.price {
        Some(price) => format!(
            "{}: {} ({tokens} at {}/{} per 1M)",
            total.label,
            format_usd(total.cost_usd().unwrap_or_default()),
            format_rate(price.input_usd_per_million_tokens),
            format_rate(price.output_usd_per_million_tokens),
        ),
        None => format!("{}: no rate configured ({tokens})", total.label),
    }
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The provider + model entry the agent would send to right now, or `None` when the active
/// model is not a configured BYOP model.
fn active_byop_model(ctx: &AppContext) -> Option<(AgentProvider, AgentProviderModel)> {
    let active = LLMPreferences::as_ref(ctx).get_active_base_model(ctx, None);
    let providers = AISettings::as_ref(ctx).agent_providers.value().clone();
    resolve_model(&providers, active.id.as_str())
}

/// Resolves a `byop:<provider_id>:<model_id>` LLM id against configured providers.
///
/// Unlike `agent_providers::lookup_byop` this deliberately does *not* require the provider or
/// model to be currently usable: `/cost` reports on tokens that were already spent, and a
/// provider disabled after the fact must not make its own spend disappear from the total.
fn resolve_model(
    providers: &[AgentProvider],
    llm_id: &str,
) -> Option<(AgentProvider, AgentProviderModel)> {
    let (provider_id, model_id) = llm_id::decode(&ai::LLMId::from(llm_id.to_owned()))?;
    let provider = providers.iter().find(|p| p.id == provider_id)?;
    let model = provider.models.iter().find(|m| m.id == model_id)?;
    Some((provider.clone(), model.clone()))
}

/// `"<model display name> (<provider name>)"`, collapsing to just the model name when the
/// provider is unnamed.
fn model_label(provider: &AgentProvider, model: &AgentProviderModel) -> String {
    let model_name = if model.name.is_empty() {
        model.id.as_str()
    } else {
        model.name.as_str()
    };
    if provider.name.is_empty() {
        model_name.to_owned()
    } else {
        format!("{model_name} ({})", provider.name)
    }
}

/// Formats a USD amount. Sub-dollar spend keeps four decimals — agent turns routinely cost
/// fractions of a cent, and rounding those to `$0.00` would read as "free".
fn format_usd(dollars: f64) -> String {
    if dollars >= 1.0 {
        format!("${dollars:.2}")
    } else {
        format!("${dollars:.4}")
    }
}

/// Formats a per-million-tokens rate with just enough precision to be recognisable against a
/// provider's published price list (`$3.00`, `$0.075`), never widening past four decimals.
fn format_rate(rate: f64) -> String {
    let mut text = format!("{rate:.4}");
    // Trim trailing zeros, but never below two decimals: `$3.00` is how a price list reads,
    // `$3.` is not.
    while text.ends_with('0') && text.split('.').nth(1).is_some_and(|frac| frac.len() > 2) {
        text.pop();
    }
    format!("${text}")
}

/// Groups an integer with `,` thousands separators.
fn thousands(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    let leading = digits.len() % 3;
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && index % 3 == leading {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
#[path = "usage_cost_tests.rs"]
mod tests;
