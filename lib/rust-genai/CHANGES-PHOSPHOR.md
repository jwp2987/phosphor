# Changes made by the Phosphor fork

`lib/rust-genai` is a **vendored, modified** copy of
[rust-genai](https://github.com/jeremychone/rust-genai) by Jeremy Chone,
dual-licensed **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE` in this
directory).

Apache-2.0 §4(b) requires modified files to carry prominent notices stating
they were changed. Each modified file **should** carry a three-line notice at
the top pointing here; this file records what the changes are. See
[Compliance gap](#compliance-gap-three-files-carry-no-notice) — that "should"
is not yet a "does" for every file.

See `UPSTREAM.md` for the pinned upstream version, how to re-derive this list,
and the re-pin policy. This file is the *content* (what changed); `UPSTREAM.md`
is the *pin* (what we're diffing against and how).

## Provenance — this list is now the real delta, not a lower bound

The list below is derived from a diff against **pristine upstream `genai
v0.6.0-beta.18`** (tag `v0.6.0-beta.18`, commit `cb343d74c`, fetched via `gh
api repos/jeremychone/rust-genai/tarball/v0.6.0-beta.18` — see `UPSTREAM.md`),
not from `git diff 91a4be9b7..HEAD`.

That distinction matters: the earlier version of this file was derived from
the latter (a diff against the commit that vendored the crate into this repo)
and said so itself — *"That commit was not a pristine upstream import... this
list is a lower bound."* It was right to hedge. The vendoring commit
`91a4be9b7` was already dirty relative to true upstream: **three files
differ from pristine `v0.6.0-beta.18` that were never in this file** —
`adapter/dispatcher.rs`, `adapter/mod.rs`, and a wholly new
`adapter/dispatcher_macros.rs` (see below). The old list named 16 files; the
true count is **19** (18 modified + 1 new) under `src/`, plus `Cargo.toml`,
`README.md`, and a `doc/` → `docs/` rename outside `src/`. The old list was
off by three files, all undocumented until this pass — not a huge miss, but
real, and it's exactly the three files with the compliance gap below.

## Modified files (`src/`)

Grouped by what the change is *for*, not file size — that's more useful for
deciding what to do with each on an upstream bump. Every file also appears
individually in `UPSTREAM.md`'s delta if you need the raw diff again.

### BYOP / proxy / streaming behavior — genuinely Phosphor's, unlikely to be wanted upstream

| file | what changed |
|---|---|
| `client/web_config.rs` | `gzip` default flipped `true` → `false`; new `no_proxy` field; new `set_proxy_settings(url, user, pass, no_proxy)` helper for BYOP custom-proxy configuration (including SOCKS5 — see the `socks` reqwest feature added in `Cargo.toml`) |
| `webc/web_client.rs` | Same `gzip(true)` → off, applied to the fallback `WebClient::default()` path (kept in sync with `web_config.rs` per its own comment) |

Rationale for gzip-off (from the code comment): `Accept-Encoding: gzip`
combined with certain reverse proxies (nginx `gzip on; gzip_proxied any;`)
forces full deflate-frame buffering before an SSE client can decode any text,
turning token-level streaming into ~K-byte bursts every ~400ms. This is a
BYOP/relay concern, not a general-purpose-library concern — upstream's
gzip-on-by-default is the right choice for direct API use, which is most of
its userbase. Not proposed for upstreaming.

### Relay/gateway compatibility workarounds — Phosphor-specific, defensible but narrow

| file | what changed |
|---|---|
| `adapter/adapters/anthropic/adapter_impl.rs` | Sends `anthropic-beta: context-1m-2025-08-07` by default for models that support 1M context (Sonnet 4+, Opus 4.6+). Without it, some third-party relay gateways (e.g. anyrouter) 400-reject the request outright; going direct to Anthropic this header is a documented no-op below 200K prompt tokens, so it's safe to send unconditionally. This is specifically a relay-compatibility default — a library talking to Anthropic directly has no reason to need it. |

### Provider-specific usage passthrough — Phosphor-specific, no upstream equivalent found (even at 0.7.0-beta.18)

| file | what changed |
|---|---|
| `chat/usage.rs` | Adds `pub extra: HashMap<String, Value>` to `Usage`, `#[serde(flatten)]`d, to generically capture provider-specific usage fields the typed struct doesn't model (example given: Ollama-compatible servers reporting `active_kv_tokens` etc.) |
| `adapter/adapters/anthropic/adapter_impl.rs`, `adapter/adapters/cohere/adapter_impl.rs`, `adapter/adapters/cohere/embed.rs`, `adapter/adapters/gemini/adapter_impl.rs`, `adapter/adapters/gemini/embed.rs`, `adapter/adapters/openai/embed.rs`, `adapter/adapters/openai_resp/resp_types/resp_usage.rs` | One-line `extra: Default::default()` at each `Usage { .. }` construction site — mechanical fallout from the field above, not an independent change |

### Now duplicated by upstream — candidates to drop on the next version bump

| file | what changed | upstream status |
|---|---|---|
| `chat/chat_options.rs` | Adds `extra_body: Option<Value>` + `with_extra_body()` + `extra_body()` — arbitrary JSON shallow-merged into the request body, "typed fields win" | **Added upstream in `0.7.0-beta.x`** (PR #255) with the **identical field name and merge semantics**. Confirmed by diffing `chat_options.rs` at `0.7.0-beta.18`. |
| `chat/tool/tool_base.rs` | Adds `cache_control: Option<CacheControl>` + `with_cache_control()` to `Tool`, so Anthropic prompt-cache breakpoints can be placed per-tool | **Added upstream in `0.7.0-beta.x`**, same field/method name and Anthropic semantics — upstream's version goes further: `ChatOptions::with_cache_control` now *also* auto-applies a breakpoint to the static tools+system prefix, which Phosphor's version does not do (a feature gap relative to 0.7, not relative to 0.6). |
| `adapter/adapters/openai/adapter_shared.rs` (the `reasoning_effort` gate half) | Widened the `reasoning_effort`-injection gate from `AdapterKind::OpenAI` only to `OpenAI \| DeepSeek` | **Superseded upstream in `0.6.1`** (2026-05-24, before 0.7 even) — upstream removed the gate entirely: *"now all openai compatible adapters get the reasoning suffix resolved."* Broader than Phosphor's fix. |

None of the three files above need a corresponding upstream contribution —
upstream already solved the same problem, in two cases with the exact field
names Phosphor picked independently.

### Still-live upstream bugs — worth sending to jeremychone

| file | what changed | verified against 0.7.0-beta.18 |
|---|---|---|
| `adapter/adapters/vertex/adapter_impl.rs` | Vertex + Anthropic-publisher + `ServiceType::ChatStream` must hit `:streamRawPredict`, not `:rawPredict` — Vertex's unary Claude endpoint silently ignores `stream: true` | **Still broken upstream at `0.7.0-beta.18`** — its `vertex/adapter_impl.rs` still routes `ServiceType::Chat \| ServiceType::ChatStream` to the same `:rawPredict` URL. This is a real bug any Vertex+Anthropic+streaming genai user would hit. |
| `adapter/adapters/openai_resp/adapter_impl.rs` | Only emit the request's `reasoning` object when the caller set an explicit reasoning effort (`effort_keyword.is_some()`), not merely because `capture_reasoning_content` is set. Some non-reasoning models (e.g. `gpt-5.3-codex-spark`) reject any request carrying a `reasoning` key with a 400/502; Phosphor's app sets `capture_reasoning_content` unconditionally for all models, so the old gate (`effort_keyword.is_some() \|\| capture_reasoning`) broke every non-reasoning model call. | **Still the old, broader gate at `0.7.0-beta.18`** (`if effort_keyword.is_some() \|\| capture_reasoning`). Not fixed upstream. Worth upstreaming, though note upstream's surrounding code has moved on (Responses-API stateful sessions, encrypted-reasoning-blob carry-forward) — this is a small, isolated hunk to port, not a big one. |

**Important correction to a prior belief:** the Gemini tool-schema
`const`→`enum` compatibility fix (commit `d70ffc219`, "Fix apply_file_diffs
tool schema") does **not** live in `lib/rust-genai` at all. Its only genai-crate
touch was an incidental test rename in `gemini/openapi_schema.rs` (see
below); the actual fix is in `app/src/ai/agent_providers/tools/edit.rs`, an
app-level tool-schema definition that calls genai's existing
`to_openapi_schema()` correctly. There is nothing here to upstream for that
fix — genai's own schema converter was never wrong.

### Cosmetic / no behavior change

| file | what changed |
|---|---|
| `adapter/adapters/gemini/openapi_schema.rs` | One test renamed (`test_non_object_schema_passthrough` → `test_non_object_passthrough`). No source change. |

### Undocumented until this pass — behavior-preserving refactor, missing its notice

| file | what changed |
|---|---|
| `adapter/dispatcher.rs` | Rewritten to dispatch through a new `dispatch_adapter!` macro instead of an 18-arm `match` repeated 7 times (once per `Adapter` trait method). Every `AdapterKind` → adapter-struct mapping is unchanged. |
| `adapter/mod.rs` | Adds `mod dispatcher_macros;` |
| `adapter/dispatcher_macros.rs` | **New file, does not exist upstream.** Defines the `dispatch_adapter!` macro (via the new `paste` dependency) that `dispatcher.rs` now uses. |

This was not attributable to any single commit message describing it as such
— it was already present at the vendoring commit `91a4be9b7` and simply never
got written down. It is functionally inert (same behavior, less repetition),
but see [Compliance gap](#compliance-gap-three-files-carry-no-notice).

## Compliance gap: three files carry no notice

`adapter/dispatcher.rs` and `adapter/mod.rs` are modified relative to
upstream and carry **no** Apache-2.0 §4(b) notice. `adapter/dispatcher_macros.rs`
is new (not a "modification" of an existing upstream file, so §4(b) doesn't
strictly apply to it the same way), but it exists *because of* the
dispatcher rewrite and was likewise never mentioned anywhere. Every other
file in the delta above does carry the three-line notice. This should be
fixed by whoever next touches these files for a substantive reason — this
pass is documentation-only and does not edit `src/`, to avoid invalidating
the just-measured delta.

## Non-`src/` differences

| path | what differs | why |
|---|---|---|
| `Cargo.toml` | Adds `[workspace]` (isolates this crate from the parent Cargo workspace — this is *why* `cargo test -p genai` from the repo root fails outright, per `UPSTREAM.md`); adds `paste = "1"` (for `dispatcher_macros.rs`); adds the `socks` feature to the `reqwest` dependency (for BYOP SOCKS5 proxy support via `web_config.rs::set_proxy_settings`) |
| `README.md` | Mentions the `Vertex` and `Aliyun` adapters (already-upstream providers, see below) and updates an example constant; doc-only |
| `doc/` → `docs/` | Directory renamed; content otherwise matches pristine upstream's `doc/for-llm/api-reference-for-llm.md` closely, though that specific file's content is itself slightly ahead of `0.6.0-beta.18` (references `0.6.0-beta.19-WIP`, `Vertex`, OpenAI-Responses `previous_response_id`/`store`) — an upstream-provenance quirk in the doc file, not a Phosphor edit. Worth a closer look if this file is ever relied on. |
| `tests/data/yakbak/gemini/thinking_stream/response_000.txt` | Byte-identical content; only line endings differ (`\r\n` vs `\n`). Not a deliberate edit — almost certainly a checkout/`autocrlf` artifact. |

## Correction: Vertex AI support is upstream-native, not a Phosphor addition

The previous version of this file listed `vertex/adapter_impl.rs` as
*"Google Vertex AI provider support"* — implying Phosphor added the Vertex
adapter. **That's wrong.** Vertex (Gemini + Anthropic via Vertex Model
Garden) shipped upstream as part of the `0.6.0` release cycle and is already
present, fully wired (including in `dispatcher.rs`'s adapter table and
`AdapterKind`), in pristine `0.6.0-beta.18`. Phosphor's only change to this
file is the one-bug streaming-URL fix listed above under "still-live upstream
bugs." The misattribution likely happened because Vertex support looks like
exactly the kind of thing this fork would add (BYOP, more providers) — but it
was already there.

## Upstreaming

- The `reasoning_effort` DeepSeek-widening note in the previous version of
  this file, and the root `Cargo.toml` `[patch]` comment referencing a
  `deepseek-reasoning-effort` branch, are moot: upstream's `0.6.1` fix
  supersedes it entirely (see the table above). If a `[patch]` entry still
  references that branch, it can be dropped once we're past `0.6.1`.
- The two "still-live upstream bugs" above (Vertex streaming URL,
  openai_resp reasoning-object gating) are real candidates to send upstream.
  Neither has landed as of `0.7.0-beta.18`.
