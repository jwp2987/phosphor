# Changes made by the Phosphor fork

`lib/rust-genai` is a **vendored, modified** copy of
[rust-genai](https://github.com/jeremychone/rust-genai) by Jeremy Chone,
dual-licensed **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE` in this
directory).

Apache-2.0 §4(b) requires modified files to carry prominent notices stating
they were changed. Every genuinely-modified file below carries a three-line
notice at the top pointing here (`grep -L "MODIFIED by the Phosphor fork"`
across the delta list returns nothing — verified at this pin).

See `UPSTREAM.md` for the pinned upstream version, how to re-derive this
list, and the re-pin policy (including the full `0.6.0-beta.18` →
`0.7.0-beta.18` re-pin history). This file is the *content* (what changed);
`UPSTREAM.md` is the *pin* (what we're diffing against and how).

## Provenance

Derived from a diff against **pristine upstream `genai v0.7.0-beta.18`**
(tag `v0.7.0-beta.18`, commit `52379bf21b10a8f10312109267f83a2b3456b0f7`,
fetched via `gh api repos/jeremychone/rust-genai/tarball/v0.7.0-beta.18` —
see `UPSTREAM.md`). This was a **re-port, not a rebase**: 0.7 reorganised
the adapters directory behind `adapters/all_adapters.rs` + `adapter/macros/`
and split Anthropic's `adapter_impl.rs` into five files, so every file in
the prior (`0.6.0-beta.18`) delta had moved or been restructured. Each
change below was re-derived from a fresh diff against the prior pin and
manually re-applied to its new home — nothing was auto-merged.

**15 modified files** under `src/`, plus `Cargo.toml` (non-`src/`). Down
from the prior pin's 18: three files (`extra_body`, tool `cache_control`,
the DeepSeek `reasoning_effort` gate) were dropped because upstream now does
the same job, one (`gemini/openapi_schema.rs`) no longer exists at all, and
the undocumented `adapter/dispatcher.rs` + `adapter/mod.rs` +
`adapter/dispatcher_macros.rs` refactor was dropped wholesale in favor of
upstream's own equivalent (and broader) macro-based refactor — see
"Dispatcher / pass-through-adapter refactor" below.

## Modified files (`src/`)

### BYOP / proxy / streaming behavior — genuinely Phosphor's, unlikely to be wanted upstream

| file | what changed |
|---|---|
| `client/web_config.rs` | `gzip` default flipped `true` → `false`; new `no_proxy` field; new `set_proxy_settings(url, user, pass, no_proxy)` helper for BYOP custom-proxy configuration (including SOCKS5 — see the `socks` reqwest feature added in `Cargo.toml`). **Byte-identical to the prior pin's patch** — this file is unchanged between pristine `0.6.0-beta.18` and pristine `0.7.0-beta.18`, so the port required zero adjustment. |
| `webc/web_client.rs` | Same `gzip(true)` → off, applied to the fallback `WebClient::default()` path. Also byte-identical to port unchanged. |

Rationale for gzip-off (from the code comment): `Accept-Encoding: gzip`
combined with certain reverse proxies (nginx `gzip on; gzip_proxied any;`)
forces full deflate-frame buffering before an SSE client can decode any text,
turning token-level streaming into ~K-byte bursts every ~400ms. This is a
BYOP/relay concern, not a general-purpose-library concern. Not proposed for
upstreaming.

### Relay/gateway compatibility workarounds — Phosphor-specific, defensible but narrow

| file | what changed |
|---|---|
| `adapter/adapters/anthropic/adapter_shared.rs` | Sends `anthropic-beta: context-1m-2025-08-07` by default for models that support 1M context (Sonnet 4+, Opus 4.6+). Without it, some third-party relay gateways (e.g. anyrouter) 400-reject the request outright; going direct to Anthropic this header is a documented no-op below 200K prompt tokens, so it's safe to send unconditionally. **Moved here from the prior pin's `adapter_impl.rs`** — 0.7 moved `build_web_request_data` (and the header-construction logic it contains) into `adapter_shared.rs`; `adapter_impl.rs` is now a thin 103-line `impl Adapter for AnthropicAdapter` trait shim that delegates to it. The `model_supports_1m_context()` predicate was kept as an independent standalone function (string-matching on family/version) rather than folded into the new `ant_model.rs::AnthropicModel` capability matrix — that matrix answers a different question (effort/thinking capability tiers), and coupling the two would make a narrow, Phosphor-specific predicate depend on upstream's own model-capability internals for no benefit. |

### Provider-specific usage passthrough — Phosphor-specific, no upstream equivalent found (even at 0.7.0-beta.18)

| file | what changed |
|---|---|
| `chat/usage.rs` | Adds `pub extra: HashMap<String, Value>` to `Usage`, `#[serde(flatten)]`d, to generically capture provider-specific usage fields the typed struct doesn't model (example given: Ollama-compatible servers reporting `active_kv_tokens` etc.). Upstream added `#[derive(PartialEq, Eq)]` to `Usage` and its detail structs in 0.7 (for the new `otel` feature's span-attribute comparisons) — `HashMap<String, Value>` satisfies both derives fine (`serde_json::Value: Eq` since a fairly old serde_json release), confirmed by the crate's own test suite passing with the derives intact. |
| `adapter/adapters/anthropic/adapter_shared.rs`, `adapter/adapters/cohere/adapter_impl.rs`, `adapter/adapters/cohere/embed.rs`, `adapter/adapters/gemini/adapter_impl.rs`, `adapter/adapters/gemini/embed.rs`, `adapter/adapters/openai/embed.rs`, `adapter/adapters/openai_resp/resp_types/resp_usage.rs`, `adapter/adapters/bedrock/streamer.rs`, `adapter/adapters/bedrock/converse.rs` | One-line `extra: Default::default()` at each exhaustive `Usage { .. }` construction site — mechanical fallout from the field above, not an independent change. The last two (Bedrock) are **new sites** that didn't exist at the prior pin: Bedrock was added upstream after `0.6.0-beta.18`, so this is the first time its two `Usage` constructors needed the field. Sites that already use `..Default::default()` (openai's own `into_usage`, all of `ollama/`, the `otel` module's test fixtures) needed no change — the spread already picks up the new field for free. |

### Now duplicated by upstream — dropped, took upstream's version

| file | what changed at the prior pin | upstream status at `0.7.0-beta.18` |
|---|---|---|
| `chat/chat_options.rs` | Added `extra_body: Option<Value>` + `with_extra_body()` + `extra_body()` — arbitrary JSON shallow-merged into the request body, "typed fields win" | **Landed upstream in `0.7.0-beta.x`** (PR #255), identical field name and method names. Upstream wires it into OpenAI, OpenAIResp, *and* Anthropic (via `payload.x_merge(extra_body.clone())` in each adapter's `adapter_shared.rs`) — broader than the fork's version, which only reached the OpenAI-family adapters. **Dropped; nothing to port.** |
| `chat/tool/tool_base.rs` | Added `cache_control: Option<CacheControl>` + `with_cache_control()` to `Tool` | **Landed upstream in `0.7.0-beta.x`**, same field/method name and Anthropic semantics — and upstream's version goes further: `ChatOptions::with_cache_control` now *also* auto-applies a breakpoint to the static tools+system prefix when no explicit message/tool-level breakpoint is present (upstream's own `adapter_shared.rs::into_anthropic_request_parts`, the "Approach B" auto-breakpoint logic). **Dropped; nothing to port** — a strict feature gain over the fork's prior version. |
| `adapter/adapters/openai/adapter_shared.rs` (the `reasoning_effort` gate half) | Widened the `reasoning_effort`-injection gate from `AdapterKind::OpenAI` only to `OpenAI \| DeepSeek` | **Superseded upstream in `0.6.1`** (before 0.7 even) — upstream removed the gate entirely; confirmed still gate-free at `0.7.0-beta.18` (`openai/adapter_shared.rs::into_usage` and the reasoning-effort injection path apply unconditionally to every OpenAI-compatible adapter). **Dropped; nothing to port.** |

### Now gone upstream — the underlying feature was replaced

| file | what changed at the prior pin | status at `0.7.0-beta.18` |
|---|---|---|
| `adapter/adapters/gemini/openapi_schema.rs` | One test renamed (`test_non_object_schema_passthrough` → `test_non_object_passthrough`); no source change | **The whole file is gone.** Gemini no longer converts JSON Schema to an OpenAPI 3.0.3 subset (`to_openapi_schema()` doesn't exist anywhere in `0.7.0-beta.18`) — per the `0.7.0-beta.x` CHANGELOG, Gemini now forwards raw JSON Schema via `responseJsonSchema`/`parametersJsonSchema` instead. Nothing to port; the function the test-rename patched doesn't exist to have a test for. |

### Still-live upstream bugs — worth sending to jeremychone

| file | what changed | verified against 0.7.0-beta.18 |
|---|---|---|
| `adapter/adapters/vertex/adapter_impl.rs` | Vertex + Anthropic-publisher + `ServiceType::ChatStream` must hit `:streamRawPredict`, not `:rawPredict` — Vertex's unary Claude endpoint silently ignores `stream: true` | **Still broken upstream at `0.7.0-beta.18`** — `get_service_url`'s `VertexPublisher::Anthropic` arm still routed `ServiceType::Chat \| ServiceType::ChatStream` to the same `:rawPredict` URL before this fork's fix was re-applied. This is a real bug any Vertex+Anthropic+streaming genai user would hit; Claude-on-Vertex streaming appears to work in no other Rust genai implementation (rig's issue #1598 is an open feature request for the same capability). Regression-tested by `test_vertex_anthropic_chat_stream_uses_stream_raw_predict` (new at this pin — the prior pin shipped the fix with no test). |
| `adapter/adapters/openai_resp/adapter_impl.rs` | Only emit the request's `reasoning` object when the caller set an explicit reasoning effort (`effort_keyword.is_some()`), not merely because `capture_reasoning_content` is set. Some non-reasoning models (e.g. `gpt-5.3-codex-spark`) reject any request carrying a `reasoning` key with a 400/502; Phosphor's app sets `capture_reasoning_content` unconditionally for all models, so the old gate (`effort_keyword.is_some() \|\| capture_reasoning`) broke every non-reasoning model call. | **Still the old, broader gate at `0.7.0-beta.18`**, word-for-word the same upstream rationale comment as at `0.6.0-beta.18` justifying the wide gate. Not fixed upstream. Regression-tested by `test_capture_reasoning_content_alone_does_not_emit_reasoning_object` / `test_explicit_reasoning_effort_emits_reasoning_object_with_summary` (new at this pin). |

### Phosphor-specific, no upstream equivalent — carried forward

| file | what changed |
|---|---|
| `adapter/adapters/anthropic/adapter_shared.rs` | **The `ChatRole::Tool` handling that emits `Text`/`Binary` parts after the `tool_result` blocks of the same user turn** — how a computer-use screenshot reaches the model on the Anthropic path without breaking strict user/assistant alternation. Upstream's `ChatRole::Tool` arm only handles `ToolResponse` and (new in 0.7) `Custom` parts; every other part type (including `Text`/`Binary`) falls through a wildcard and is silently dropped. **CRITICAL:** `cache_control` is applied to the `tool_result` blocks **before** the trailing parts are appended (via a small `binary_to_anthropic_content_part()` helper added alongside, mirroring the image/document conversion the `ChatRole::User` arm already does inline, so that arm didn't need touching). Reversed, the prompt-cache breakpoint lands on a screenshot that changes every turn — the cache can never hit, every turn pays the write premium, and Anthropic returns no error, because `cache_control` on an image block is legal. Silent cost regression on the user's own bill. The regression test `test_screenshot_does_not_steal_the_cache_breakpoint` was ported and passes. |

## Dispatcher / pass-through-adapter refactor — dropped ours, took upstream's

At the prior pin, `adapter/dispatcher.rs` and `adapter/mod.rs` carried an
undocumented local rewrite routing through a new `dispatch_adapter!` macro
in a wholly-new `adapter/dispatcher_macros.rs` (using the `paste`
dependency), avoiding an 18-arm `match` repeated across every `Adapter`
trait method. None of the three files carried an Apache-2.0 §4(b) notice —
a licence-compliance gap on top of being undocumented.

Upstream 0.7 independently ships the same kind of refactor, and goes
further: `adapters/all_adapters.rs` + `adapter/macros/{dispatcher_macros,
adapter_impl_macros, adapter_kind_macros}.rs` not only dispatch through a
macro, but macro-generate whole pass-through adapter *structs* — every
OpenAI/Anthropic-compatible provider that needs nothing beyond "delegate to
`OpenAIAdapter`/`AnthropicAdapter` with a different base URL and env var"
(Aliyun, BigModel, DeepSeek, Groq, Kimi, Mimo, Moonshot, Nebius, Together,
Xai, AtlasCloud, QwenCloud, OpenRouter, Aihubmix, MiniMax — 15 providers) is
now a five-line `impl_pass_through_adapter!(...)` invocation instead of a
whole `adapter_impl.rs` + `mod.rs` pair.

**Judgement call: dropped the fork's parallel refactor entirely, took
upstream's.** Carrying our own dispatch macro alongside upstream's
equivalent-but-broader one would be pure duplicated cost for zero behavioral
gain — same conclusion the original `UPSTREAM.md` re-pin procedure
recommends ("if upstream's structure does the same job, drop ours and take
upstream's"). `dispatcher.rs` and `mod.rs` are now byte-identical to
pristine upstream. This also **resolves the licence-compliance gap by
elimination**: nothing "modified" remains in those files that would need a
notice, and the eight now-redundant per-provider directories this fork used
to carry (`aliyun/`, `bigmodel/`, `deepseek/`, `groq/`, `mimo/`, `nebius/`,
`together/`, `xai/` — none of which had Phosphor-specific changes) were
deleted along with the old dispatcher, since upstream's `all_adapters.rs`
pass-through list already covers all of them by name with the same
`AdapterKind` variants, env var names, and endpoints.

## Non-`src/` differences

| path | what differs | why |
|---|---|---|
| `Cargo.toml` | Adds `[workspace]` (isolates this crate from the parent Cargo workspace); adds the `socks` feature to the `reqwest` dependency (for BYOP SOCKS5 proxy support via `web_config.rs::set_proxy_settings`). Upstream 0.7 already adds its own `paste` dependency (for `adapter/macros/`) and switched `reqwest` to `default-features = false` with an explicit feature list (`json`, `stream`, `gzip`, `charset`, `http2`, `system-proxy`) — the fork's `socks` addition slots into that explicit list; no `[patch]` section needed since upstream doesn't gate `socks` behind anything. |

`README.md`, `CHANGELOG.md`, `BIG-THANKS.md`, `LICENSE-*`, `rustfmt.toml`,
`.gitignore`, `docs/`, `dev/`, `examples/` are all byte-identical to
pristine upstream at this pin (taken wholesale rather than re-applying the
prior pin's small doc-only edits, e.g. `README.md`'s provider list — upstream's
own README already documents far more providers than the fork's hand-edit
did).

## Upstreaming

The two "still-live upstream bugs" above (Vertex streaming URL,
openai_resp reasoning-object gating) are real candidates to send upstream.
Neither has landed as of `0.7.0-beta.18`.

## New providers / capabilities gained since the prior pin (`0.6.0-beta.18`)

Not Phosphor changes — noted here because they're new surface the app can
opt into. New `AdapterKind` variants since the prior pin: `Aihubmix`,
`Kimi`, `Moonshot`, `Baidu`, `MiniMax`, `Omlx`, `OpenCodeGo`, `BedrockApi`,
`BedrockSigv4` (feature-gated `bedrock-sigv4`), `OpenRouter`, `AtlasCloud`,
`QwenCloud`, `Custom(u8)` (the `genai_N::` namespace adapter). Also: an
`otel` Cargo feature (off by default) adding OpenTelemetry GenAI
semantic-convention tracing spans — **not verified working**, see "Found in
passing" below; Anthropic `ChatStreamEvent::Heartbeat` events (SSE ping
frames now surfaced as a stream event instead of silently absorbed); OpenAI
Responses freeform custom tools with grammar-constrained raw-string input;
`ChatOptions::extra_body` and `Tool::cache_control` (see above — now
upstream's own, superseding the fork's prior versions).

## Found in passing — not fixed, out of scope for this port

`src/otel/error.rs`'s `match error { .. }` (behind the off-by-default `otel`
feature) does not have an arm for `Error::CacheBreakpointNoEligibleContent`,
so `cargo test --features otel` fails to compile with a non-exhaustive-match
error at pristine `0.7.0-beta.18` — confirmed against the untouched pristine
tarball, not introduced by this port. Not part of the default feature set
(the crate's own baseline test command doesn't enable `otel`), not part of
this fork's delta, and not something Phosphor's app currently uses. Left
alone rather than silently patched into an undocumented, untracked local
change; worth a heads-up if `otel` is ever turned on.
