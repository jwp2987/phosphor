# Google Vertex AI backend — work scope

Scoping doc for adding Vertex AI as a first-class BYOP provider type.

## Executive summary

The hard part is **already done** inside the vendored `lib/rust-genai`: there is a
complete `AdapterKind::Vertex` / `VertexAdapter` that builds the Vertex URL, routes
by publisher, and shapes requests/responses. **All remaining work is on the Zap
side, and it is almost entirely GCP OAuth2 token minting + refresh** — because
Vertex does not use a static API key.

## What genai already provides (no work)

`lib/rust-genai/src/adapter/adapters/vertex/adapter_impl.rs`:

- URL construction:
  `https://{location}-aiplatform.googleapis.com/v1/projects/{project}/locations/{location}/`
  (falls back to `global` when no location). Reads `VERTEX_PROJECT_ID` /
  `VERTEX_LOCATION` from env, or takes an explicit `Endpoint` from the resolver.
- Publisher routing by model name: `gemini*` -> `publishers/google` (Gemini wire
  format), `claude*` -> `publishers/anthropic` (Anthropic wire format). Reuses the
  existing Gemini and Anthropic request/response adapters.
- Auth contract (adapter comment, line ~132): *"For Vertex AI the 'api key' is an
  OAuth2 Bearer token supplied by the AuthResolver"* — it sets
  `Authorization: Bearer {api_key}`.
- `AdapterKind::Vertex` is already in the enum with name/namespace/parsing wired.

genai also already supports **async resolvers**, which token refresh needs:
`ServiceTargetResolver::from_resolver_async_fn` and
`AuthResolver::ResolverAsyncFn` (`lib/rust-genai/src/resolver/`).

## The core problem

Zap's current `build_client` uses a **synchronous** resolver and
`AuthData::from_single(static_key)`, where `static_key` is a fixed string read from
secure storage. Vertex tokens are **short-lived (~1h) OAuth2 access tokens** derived
from a service account or Application Default Credentials, and must be refreshed.
So Vertex cannot reuse the static-key path unchanged.

## Work breakdown

### 1. GCP token acquisition + refresh (the real work)

No GCP/JWT/OAuth dependency exists in the tree today (`Cargo.lock` has none).

- **Recommended:** add a maintained crate that handles credential discovery,
  signing, caching and refresh — `gcp_auth` (supports service-account JSON, ADC,
  metadata server, gcloud) or `yup-oauth2`. Preferred over hand-rolling RS256 JWT
  signing + token exchange with `jsonwebtoken` + `reqwest`.
- Scope of a token provider:
  - Input: service-account JSON (or a path to it), or ADC.
  - Output: a valid access token for scope
    `https://www.googleapis.com/auth/cloud-platform`.
  - Internal caching + refresh-before-expiry (the crates above do this).
- Wrap it in a small Zap-side `VertexTokenProvider` singleton keyed by
  provider id, mirroring how `AgentProviderSecrets` is keyed.

### 2. Wire the token into build_client (async resolver)

- Add a Vertex branch in `chat_stream::build_client_uncached` that uses
  `from_resolver_async_fn` (instead of the current sync `from_resolver_fn`) and,
  inside it, `await`s the token provider to get a fresh bearer, then
  `AuthData::from_single(token)` + `AdapterKind::Vertex`.
- **Client cache keying (`BYOP_CLIENT_CACHE`) must change for Vertex.** Today the
  cache key includes `api_key`; a refreshing token would thrash or pin a stale
  token. Key Vertex clients on `(project, location, service-account identity)`
  instead of the token, and let the async resolver fetch the current token per
  request. Audit `build_client` / `CachedByopClient` for this.
- Endpoint: build the aiplatform URL from project+location and pass it as the
  resolver `Endpoint` (do not rely on process env, which is global and unsafe with
  multiple providers).

### 3. Config model (`app/src/settings/ai.rs`)

- Add `AgentProviderApiType::Vertex`.
- Vertex needs `project_id` and `location`, which a single `base_url` cannot carry.
  Add optional fields to `AgentProvider` (e.g. `vertex_project`, `vertex_location`),
  `#[serde(default, skip_serializing_if = ...)]` for back-compat.
- The service-account JSON is a secret: store it in `AgentProviderSecrets`
  (it stores arbitrary strings; the JSON fits) or store a file path there. Do not
  put it in `settings.toml`.

### 4. Exhaustive match-site arms

`AgentProviderApiType` is matched across the codebase; add Vertex arms. Definite
sites (`app/src/settings/ai.rs`, `app/src/ai/agent_providers/chat_stream.rs`):

- `display_name` -> "Vertex AI"
- `from_debug_str` -> `"Vertex"`
- `default_base_url` -> not a static string; derive from project/location, or
  return a placeholder and require project/location in the UI.
- `effective_adapter_kind_for` -> `AdapterKind::Vertex`.

Then audit the grouping matches (many are `matches!(..)`, not exhaustive, and may
just fall through): `reasoning.rs` (~63 refs), `prompt_renderer.rs` (~29),
`attachment_caps.rs` (~12). Vertex should reuse Gemini/Claude behavior keyed on the
model name, so most reduce to "treat like Gemini or Anthropic per model family."

### 5. Endpoint normalization

`normalize_endpoint_url` needs a Vertex branch: build the aiplatform URL from
project + location and skip the OpenAI `/v1` path logic entirely.

### 6. Capabilities / reasoning

Vertex serves Gemini and Claude models, so `attachment_caps` and reasoning-effort
inference should reuse the Gemini / Anthropic paths selected by model-name family.
models.dev does not key Vertex the same way; expect to rely on model-family
inference or user-entered caps rather than the catalog.

### 7. UI (`app/src/settings_view/ai_page.rs` + provider widget)

When `api_type == Vertex`, the provider form changes:

- Replace the plain `api_key` + `base_url` fields with: `project_id`, `location`
  (dropdown of common regions + free text), and a service-account JSON / key-file
  input (or an "use Application Default Credentials" toggle).
- Model ids are entered as `gemini-2.5-flash`, `claude-sonnet-4-x`, etc.; the
  adapter picks the publisher automatically.

## Effort estimate

- Steps 3–5 (enum/config/endpoint plumbing): ~0.5 day, mechanical.
- Step 1 (token provider via `gcp_auth`/`yup-oauth2`): ~1 day incl. SA-vs-ADC
  handling and refresh.
- Step 2 (async resolver + cache-key change): ~0.5–1 day; the cache keying is the
  one subtle correctness item.
- Steps 6–7 (caps + UI): ~1 day.

**Total: ~3–4 days.** Low technical risk because genai already owns the protocol;
the risk concentrates in GCP auth (service-account vs ADC, token refresh, and the
client-cache keying change).

## Open decisions

- Support **ADC only**, **service-account JSON only**, or both? SA JSON is simplest
  to reason about and store; ADC is friendlier on a developer box already logged in
  via `gcloud`. Recommend starting with service-account JSON.
- Whether to also expose Vertex-hosted **Claude** (publishers/anthropic) or restrict
  to Gemini initially. genai supports both for free; gating is a UI decision.
