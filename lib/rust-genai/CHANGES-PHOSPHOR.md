# Changes made by the Phosphor fork

`lib/rust-genai` is a **vendored, modified** copy of
[rust-genai](https://github.com/jeremychone/rust-genai) by Jeremy Chone,
dual-licensed **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE` in this
directory).

Apache-2.0 §4(b) requires modified files to carry prominent notices stating
they were changed. Each modified file carries a three-line notice at the top
pointing here; this file records what the changes are.

## Scope caveat — read this before trusting the list

The per-file list below is derived from `git diff 91a4be9b7..HEAD --
lib/rust-genai/src/`, where `91a4be9b7` is the commit that vendored the crate
into this repository.

**That commit was not a pristine upstream import.** It already carried fork
changes — the DeepSeek `reasoning_effort` widening in
`openai/adapter_shared.rs` and the `extra_body` merge are both present at
`91a4be9b7` itself, and so do not appear in the diff. Establishing the true
delta needs a diff against upstream `genai 0.6.0-beta.18`, which is not
available offline in this checkout.

So: **this list is a lower bound.** Every file named here was definitely
changed. Files not named here may still differ from upstream.

## Modified files

Changes since the vendoring commit, largest first:

| file | what changed |
|---|---|
| `adapter/adapters/anthropic/adapter_impl.rs` | Mixed-TTL Anthropic prompt caching; 1M-context beta header sent by default; `ChatRole::Tool` also emits `Text`/`Binary` parts, after the `tool_result` blocks of the same user turn (upstream drops them) — this is how a computer-use screenshot reaches the model on the Anthropic path without breaking its strict user/assistant alternation |
| `client/web_config.rs` | `gzip` default flipped to `false` (upstream defaults it on); proxy-mode configuration |
| `adapter/adapters/openai_resp/adapter_impl.rs` | OpenAI Responses reasoning-parameter fixes (502 on unsupported params) |
| `chat/tool/tool_base.rs` | Tool-schema handling for providers that reject `const` (Gemini) |
| `webc/web_client.rs` | `gzip(true)` → off; custom-proxy application for BYOP streaming |
| `chat/usage.rs` | Usage-field additions |
| `adapter/adapters/vertex/adapter_impl.rs` | Google Vertex AI provider support |
| `chat/chat_options.rs` | `extra_body` extension — arbitrary JSON merged into the request body |
| `adapter/adapters/openai/adapter_shared.rs` | `reasoning_effort` injection widened from `OpenAI` to `OpenAI \| DeepSeek`; `extra_body` shallow-merged at top level; `reasoning_content` echo |
| `adapter/adapters/gemini/openapi_schema.rs` | One test renamed (`test_non_object_schema_passthrough` → `test_non_object_passthrough`) |
| `adapter/adapters/openai_resp/resp_types/resp_usage.rs` | Constructs the new `Usage::extra` field |
| `adapter/adapters/cohere/adapter_impl.rs` | Constructs the new `Usage::extra` field |
| `adapter/adapters/cohere/embed.rs` | Constructs the new `Usage::extra` field |
| `adapter/adapters/gemini/adapter_impl.rs` | Constructs the new `Usage::extra` field |
| `adapter/adapters/gemini/embed.rs` | Constructs the new `Usage::extra` field |
| `adapter/adapters/openai/embed.rs` | Constructs the new `Usage::extra` field |

The five one-line `Usage::extra` edits are call-site fallout from the `extra`
field added to `chat/usage.rs`, not independent changes.

No files were added to or removed from `src/` after the vendoring commit.

## Upstreaming

The `reasoning_effort` widening was intended for upstream (a
`deepseek-reasoning-effort` branch is referenced in the root `Cargo.toml`
`[patch]` comment). If it lands upstream, that hunk can be dropped here.
