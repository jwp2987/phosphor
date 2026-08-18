# rmcp migration inventory — 0.10 (warpdotdev fork) → published crate

**Status: MIGRATED to `rmcp = { version = "1.6" }`. Nothing compiled, nothing
fetched — the tree has not been built and `Cargo.lock` has not been regenerated.**

History: the maintainer approved 1.6 on 2026-08-17. A first attempt was reverted
on discovering that rmcp 1.6 requires reqwest 0.13 while Phosphor was on 0.12
(§7). That prerequisite landed separately, and the rmcp migration was then
completed on 2026-08-18 (§8).

Measured 2026-08-17 against:

| what | where |
|---|---|
| current pin | `Cargo.toml:443` → `warpdotdev/rmcp` rev `c0f65dc441af7d714b9c453ac5e7ef641451abe3`, resolved by `Cargo.lock:11643-11645` to **0.10.0** |
| pinned source | `~/.cargo/git/checkouts/rmcp-aaacf7b4731e81c8/c0f65dc/` |
| oracle | Warp `42effe840` — `Cargo.toml:420` is `rmcp = { version = "1.6" }`, `Cargo.lock` resolves **exactly 1.6.0** |
| upstream clone | `…/scratchpad/rmcp-upstream` (tags `rmcp-v1.0.0` … `rmcp-v3.1.3`) |

---

## 1. Usage inventory — verified counts

**22 files, 196 lines containing `rmcp`, 214 raw occurrences.** The prior
estimate in `TODO.md:3214` ("22 files, 176 `rmcp::` references") has the file
count right and undercounts references by ~20; 176 is close to the count of
lines carrying a literal `rmcp::` path, which misses `use`-imported names
(`RawResource`, `ErrorData`, `AuthClient`, …) and comment references.

| file | refs |
|---|---|
| `app/src/ai/mcp/templatable_manager/native.rs` | 69 |
| `app/src/server/telemetry.rs` | 13 |
| `app/src/ai/mcp/templatable_manager.rs` | 13 |
| `app/src/ai/mcp/reconnecting_peer.rs` | 13 |
| `app/src/ai/agent/api/convert_conversation.rs` | 13 |
| `app/src/ai/mcp/templatable_manager/oauth.rs` | 12 |
| `app/src/ai/agent_sdk/driver/output.rs` | 12 |
| `app/src/ai/mcp/templatable_manager/native_tests.rs` | 6 |
| `crates/ai/src/agent/action_result/convert.rs` | 5 |
| `app/src/ui_components/json_tree_tests.rs` | 5 |
| `app/src/ai/agent_providers/tools/mcp_tests.rs` | 5 |
| `app/src/ai/agent/api/convert_to.rs` | 5 |
| `app/src/ai/mcp/templatable_manager/oauth_tests.rs` | 4 |
| `app/src/ai/blocklist/action_model/execute/call_mcp_tool.rs` | 4 |
| `app/src/ai/agent/mod.rs` | 4 |
| `app/src/ai/mcp/http_client.rs` | 3 |
| `app/src/ai/agent_providers/tools/mcp.rs` | 3 |
| `crates/ai/src/agent/action_result/mod.rs` | 2 |
| `app/src/ai/blocklist/action_model/execute/read_mcp_resource.rs` | 2 |
| `app/src/ai/mcp/templatable_manager/oauth/loopback.rs` | 1 |
| `app/src/ai/blocklist/inline_action/requested_command.rs` | 1 |
| `app/src/ai/agent_providers/cache_stability_tests.rs` | 1 |

Manifest declarations: `Cargo.toml:443` (workspace pin), `app/Cargo.toml:250`
(`features = ["client"]`), `app/Cargo.toml:310` (`features = ["auth",
"transport-streamable-http-client-reqwest", "transport-sse-client-reqwest",
"transport-child-process"]`), `crates/ai/Cargo.toml:46`.

---

## 2. Fork vs upstream — RESOLVED (not "could not determine")

The cargo checkout carries full git history. The fork is **upstream commit
`57d1ac9` ("chore: release v0.9.2", declared workspace version 0.10.0) plus
exactly three commits**, touching exactly two files:

| commit | date / author | file | substance |
|---|---|---|---|
| `5ea86b0` | 2025-09-01, David Stern | `transport/async_rw.rs` | "Ignore non-parseable lines on stdout." `Decoder::decode` `continue`s past a parse error instead of propagating it |
| `01ba6c9` | 2025-12-19, David Stern | `transport/auth.rs` | only attempt refresh when a refresh token or an expiry exists — superseded by the next commit |
| `c0f65dc` | 2026-04-06, Pei Li @warp.dev | `transport/auth.rs` | adds `StoredCredentials::{granted_scopes, token_received_at}`; rewrites `get_access_token` around a 30 s `REFRESH_BUFFER_SECS`; re-sends granted scopes on refresh. ~560 of its 690 added lines are tests. Partly a backport of upstream PR #731 |

No `warp`/`warpdotdev` string appears anywhere in the fork's source, and the
repo's own `Cargo.toml` declares `repository = "…/modelcontextprotocol/rust-sdk/"`
with no `[patch]` section. The divergence is only these three commits.

**Both fork patches are already upstream — but at different versions.** This is
the single most important result in this document:

| behaviour | fork 0.10 | 1.6.0 | 1.7.0–2.0.0 | 2.1.0+ / 3.x |
|---|---|---|---|---|
| `StoredCredentials.token_received_at` + `granted_scopes` | yes | **yes** | yes | yes |
| unparseable line on a child server's stdout | skipped, session lives | **session CLOSES** | session lives, replies `-32700` per junk line | skipped silently (fork-equivalent) |

Upstream's own commit messages date the ladder:
`321ab14` "fix: reply -32700 on stdio parse errors instead of closing (#833)" first
appears in **1.7.0**; `64d22de` "fix: don't respond to unparseable messages (#940)"
first appears in **2.1.0**.

Verified mechanically at 1.6.0: `AsyncRwTransport` is still
`FramedRead<R, JsonRpcMessageCodec<…>>`; `Decoder::decode` still does
`try_parse_with_compatibility(line, "decode")?`; `receive()` is
`next.await.and_then(|e| e.inspect_err(…).ok())`, so a decoder error becomes
`None`, which the service loop reads as transport-closed. `TokioChildProcess`
routes through `AsyncRwTransport`, and Phosphor spawns stdio MCP servers through
it (`native.rs:1783`) with stderr piped separately — so only genuine stdout junk
(banners, `console.log`, package-manager notices) triggers it. That is precisely
the case Warp wrote patch `5ea86b0` for.

**Migrating to exactly 1.6.0 therefore reintroduces the bug the fork patch fixes,
and the oracle carries no workaround** — `42effe840:crates/mcp/src/runtime.rs:137`
spawns `TokioChildProcess` plainly.

---

## 3. The removal nobody had spotted: the SSE client transport is gone from every published 1.x/2.x/3.x

`rmcp::transport::SseClientTransport`, `SseClientConfig`, the `SseClient` trait,
`SseTransportError` and the `transport-sse-client*` features were **deleted in v0.11.0**. Checked at `rmcp-v1.0.0`, `1.1`, `1.2`, `1.3`, `1.5`, `1.6`,
`1.8`, `2.1`, `3.1.3` — zero matches in all of them. Only the SSE *parsing*
primitives survive, behind the still-present `client-side-sse` feature, used
internally by the streamable-HTTP client.

Phosphor uses SSE as a **live fallback**: `determine_transport` preflights
Streamable HTTP and drops to legacy SSE on a 404
(`native.rs:1905-1949`, ~10 references). Losing it silently breaks every remote
MCP server that only speaks legacy SSE.

**The oracle already solved this**, and the solution is readable:
`42effe840:crates/mcp/src/sse_transport/` vendors the transport into Warp's own
crate — 5 files, **694 lines** (`mod.rs` 10, `sse_client.rs` 260,
`client_side_sse.rs` 282, `reqwest_impl.rs` 96, `auth_impl.rs` 46), plus a direct
`sse-stream = "0.2"` dependency, with the same `SseClientTransport` /
`SseClientConfig` API re-homed under `crate::sse_transport::`. The oracle's rmcp
feature list is correspondingly
`["client", "client-side-sse", "auth", "transport-streamable-http-client-reqwest", "transport-child-process"]`
— note `transport-sse-client-reqwest` is gone and `client-side-sse` is added.

Note the oracle has also restructured MCP into a `crates/mcp` crate that Phosphor
does not have; following upstream fully is two changes, not one. Porting only
`sse_transport/` is the smaller, sufficient move.

---

## 4. API surface consumed, and its status at 1.6 vs 3.1.3

Legend: **OK** = compiles unchanged · **warn** = deprecated alias, still compiles ·
**BREAK** = compile error · **GONE** = no replacement.

### Model

| item | Phosphor sites | 1.6 | 3.1.3 |
|---|---|---|---|
| `RawContent` (5 variants, exhaustive match) | output.rs ×5, call_mcp_tool.rs:316, requested_command.rs:202, convert.rs:1059 | **OK** — still `#[expect(clippy::exhaustive_enums)]`, same 5 variants | **BREAK** — replaced by `ContentBlock`; the `Annotated`/`.raw` wrapper is gone |
| `Content` = `Annotated<RawContent>`, `::text/::image/::resource` | convert_conversation.rs ×6, json_tree_tests.rs ×2 | **OK** | **BREAK** — alias deleted; `ContentBlock::*` |
| `RawResource` (destructured with `..`), `Resource`, `AnnotateAble::no_annotation()` | convert_to.rs, output.rs:207, mcp_tests.rs, cache_stability_tests.rs | **OK** — intentionally exhaustive; `size: Option<u32>` | **BREAK** — `RawResource`/`AnnotateAble` deleted; flat `Resource`, `size: Option<u64>` |
| `ResourceContents` both variants (literal + match) | convert_conversation.rs ×4, output.rs ×4, convert.rs ×2 | **OK** — byte-identical to 0.10 | OK-ish (`Meta`→`MetaObject`, `::text` mime default changed) |
| `Tool`, `ServerCapabilities`, `ErrorData`, `ErrorCode::INTERNAL_ERROR`, `JsonObject`, `RequestId`, `ClientJsonRpcMessage`, `ClientRequest`, `InitializeRequest::new` | various | **OK** | mostly OK |
| `CallToolResult` + `::success` / `::structured` | json_tree_tests.rs ×3, convert.rs, output.rs, mod.rs | **OK** (read-only + constructors) | OK (gains `result_type`) |
| `CallToolRequestParam` **literal** | call_mcp_tool.rs:145 | **BREAK** — now `CallToolRequestParams`, `#[non_exhaustive]`, +`meta`/`task`. Alias exists → use `::new(name).with_arguments(a)` | **BREAK**, and no alias at all |
| `ReadResourceRequestParam` **literal** | read_mcp_resource.rs:131 | **BREAK** — `ReadResourceRequestParams::new(uri)` | **BREAK**, no alias |
| singular `…Param` names in *type* position | reconnecting_peer.rs ×2, call_mcp_tool.rs:299, read_mcp_resource.rs:155 | **warn** (deprecated alias; no `-D warnings` in this repo) | **BREAK** — aliases removed |
| `ClientInfo` **literal** | native.rs:2167 | **BREAK** — `#[non_exhaustive]`, +`meta` | **BREAK** |
| `Implementation` **literal** | native.rs:2170 | **BREAK** — `#[non_exhaustive]`, +`description`; use `::new(name, version)` | **BREAK** |
| `ProtocolVersion` via `Default::default()` | native.rs:2168 | **silent change** — `LATEST` 2025-03-26 → **2025-11-25** | silent change → 2025-11-25 |
| `Tool`, `ServerCapabilities`, `RawResource` | mcp_tests.rs, cache_stability_tests.rs, native_tests.rs, convert_to.rs, output.rs | **OK — narrowly.** All three became `#[non_exhaustive]` (and `RawResource` gained `meta`, `Tool` gained `execution`, `ServerCapabilities` gained `extensions`/`tasks`), but Phosphor only ever uses `Tool::new` / `serde_json::from_value`, `ServerCapabilities::builder()`, and `RawResource` destructures that already end in `..` | mixed |

### Service / error

| item | sites | 1.6 | 3.1.3 |
|---|---|---|---|
| `Peer`, `RoleClient`, `ServiceExt`, `RunningService`, `DynService`, `ServiceRole`, `Tx/RxJsonRpcMessage`, `into_dyn`, `serve`, `peer_info`, `list_all_tools/resources`, `cancel` | throughout | **OK** (`serve`'s bound becomes `MaybeSendFuture`, which is exactly `Send` with the default feature set) | OK |
| `ServiceError` **exhaustive match** | native.rs:134-153, telemetry.rs:345-351 | **BREAK** — now `#[non_exhaustive]`; add a `_ =>` arm (2 sites) | same |
| `RmcpError` **exhaustive match** | native.rs:120-133, telemetry.rs:336-345 | **BREAK** — `#[non_exhaustive]`, +`TaskError`; add `_ =>` (2 sites) | same |
| `RmcpError::transport_creation::<T>` | native.rs ×7, http_client.rs | **OK** — identical | OK |
| `Transport<R>` trait, hand-implemented by `TransportLoggingWrapper` | native.rs:2188-2215 | **OK** — trait byte-for-byte identical | OK |

### Transport

| item | sites | 1.6 | 3.1.3 |
|---|---|---|---|
| `StreamableHttpClientTransport::with_client` / `::from_uri`, `…Config::with_uri` | native.rs ×5, http_client.rs | **OK** (config is now `#[non_exhaustive]`, but Phosphor only uses `::with_uri`) | OK |
| `TokioChildProcess::builder(..).stderr(..).spawn()` → `(transport, Option<ChildStderr>)` | native.rs:1783 | **OK** — identical | OK |
| `ConfigureCommandExt` | native.rs:52 | **OK** — still re-exported from `rmcp::transport` | moved to `transport::child_process` |
| `common::http_header::{EVENT_STREAM_MIME_TYPE, JSON_MIME_TYPE}` | native.rs:2060 | **OK** | OK |
| `SseClientTransport`, `SseClientConfig`, `sse_client::` | native.rs ×~10 | **GONE** — port oracle's `sse_transport/` (694 lines) | **GONE** — same, but the oracle's port targets 1.x internals |

### Auth (`oauth.rs`, `oauth/loopback.rs`, `oauth_tests.rs`)

| item | sites | 1.6 | 3.1.3 |
|---|---|---|---|
| `AuthError` (`InternalError`, `AuthorizationFailed`, `RegistrationFailed`), `AuthorizationSession` re-export from `rmcp::transport` | loopback.rs ×12, oauth.rs | **OK** | OK |
| `CredentialStore` trait (impl'd by `PersistingCredentialStore`) | oauth.rs:114-137 | **OK** — identical 3 methods, still `#[async_trait]` | OK |
| `InMemoryCredentialStore::new`, `AuthClient::new`, `OAuthTokenResponse`, `oauth2 = 5.0` | oauth.rs, oauth_tests.rs | **OK** — oauth2 stays 5.0; Phosphor's own `oauth2 = { 5.0.0, default-features = false, features = ["reqwest"] }` matches | OK |
| `StoredCredentials` **literal** (`{client_id, token_response, granted_scopes, token_received_at}`) | oauth.rs:185, 484 | **BREAK** — `#[non_exhaustive]`; exact drop-in `StoredCredentials::new(client_id, token_response, granted_scopes, token_received_at)` exists | **BREAK** — also gains `issuer`, and the type carries `current_scopes` machinery |
| `OAuthClientConfig` **literal** | oauth.rs:275, 390 | **BREAK** — `#[non_exhaustive]`; `::new(client_id, redirect_uri)` + field assignment | **BREAK** — also gains `application_type` |
| `AuthorizationManager::{get_credentials, set_credential_store, configure_client, refresh_token, discover_metadata, get_authorization_url}` | oauth.rs | **OK** | OK |
| `AuthorizationManager::register_client(name, redirect_uri)` | oauth.rs:370 | **BREAK** — gains a `scopes: &[&str]` arg | **BREAK** — same |
| `OAuthState::{new, set_credentials, handle_callback, get_credentials, into_authorization_manager}`, `if let` on variants | oauth.rs | **OK** (enum `#[non_exhaustive]`, but Phosphor only uses `if let`, never an exhaustive match) | OK |
| `OAuthState::start_authorization(&[], &uri, Some("Phosphor"))` | oauth.rs:352 | **OK** — same 3-arg signature | **BREAK** — redesigned to take an `AuthorizationRequest` builder |
| `AuthorizationSession` **literal** (`{auth_manager, auth_url, redirect_uri}`) | oauth.rs:406 | **BREAK** — `#[non_exhaustive]`, but 1.6 ships an exact drop-in: `AuthorizationSession::for_scope_upgrade(auth_manager, auth_url: String, redirect_uri: &str) -> Self`, a plain non-async constructor that fills precisely these three fields | **BREAK** — no equivalent escape hatch, and `start_authorization` is redesigned around it |
| `AuthError` exhaustive match, `OAuthState` exhaustive match | oauth.rs:372 (has `Err(e) =>` catch-all), `if let` on `OAuthState` | **OK** — both became `#[non_exhaustive]`, but Phosphor never matches either exhaustively | OK |
| `OAuthTokenResponse` | oauth_tests.rs:1,92 | **OK in practice** — alias retargeted `EmptyExtraTokenFields` → `VendorExtraTokenFields`, but Phosphor only builds it via `serde_json::from_value` and never names the underlying `StandardTokenResponse<..>` | same retarget |

`PersistedCredentials` stores `#[serde(flatten)] credentials: StoredCredentials`
in secure storage. `granted_scopes` and `token_received_at` are `#[serde(default)]`
at every version examined, so previously-persisted credentials still deserialize.

---

## 5. Comparison and recommendation

| | **1.6 (oracle parity)** | **3.1.3 (latest upstream)** |
|---|---|---|
| relationship to `ORACLE.md` | matches the pin's declaration exactly | deliberate divergence — maintainer call |
| fork patch: `token_received_at` / `granted_scopes` | present | present |
| fork patch: tolerate junk stdout | **absent at 1.6.0** — session closes; present from 1.7.0 (as `-32700` reply) and fully from 2.1.0 | present |
| model-layer sites to change | ~4 | ~25 (`ContentBlock`, flat `Resource`, no `…Param` aliases) |
| auth-layer sites to change | 5 (all mechanical except one) | ~10, incl. a redesigned `start_authorization` |
| service/error sites | 4 wildcard arms | same |
| SSE | must port oracle's `sse_transport/` (694 lines, exists and is readable) | same port, but with no version-matched reference |
| reference implementation available | **yes — the oracle is on this exact version** | no |
| genuinely blocked sites | **0** — every break has a published constructor | 1+ (`AuthorizationSession` has no escape hatch; `start_authorization` redesigned) |

> **Superseded in part by §7.** The version recommendation below still holds,
> but the effort estimates do not: they omit the prerequisite reqwest 0.12 → 0.13
> bump (32 files, 172 references, six crates), which dominates the work.

**Recommended target: the 1.x line — declare `rmcp = { version = "1.6" }`
(byte-identical to `42effe840:Cargo.toml:420`) and let the lockfile resolve to
the newest 1.x, i.e. 1.8.0.**

Rationale:

1. It is the oracle's own declaration, so it is parity, not divergence.
2. `^1.6` legitimately admits 1.7/1.8, so taking 1.8.0 in the lockfile keeps the
   *declared* dependency identical to the pin while avoiding the one real
   regression — at 1.6.0 a single junk stdout line kills a stdio MCP session; at
   1.8.0 the session survives (it replies `-32700` per junk line, noisier than
   the fork but not fatal). API surface at 1.8.0 is unchanged from 1.6.0 for
   every item Phosphor touches (spot-checked: features, `StoredCredentials`,
   the `…Param` aliases, SSE still absent).
3. Every break at 1.6/1.8 has a mechanical fix, and for the one structural gap
   (SSE) the oracle contains a working port to copy.
4. 3.1.3 costs several times the work, has no reference implementation, and buys
   Phosphor nothing it needs — the only 3.x-only gain relevant here is the
   silent-skip refinement of the stdout fix, which is a log-noise difference.

**The one thing the maintainer must decide explicitly:**

- **Exact-1.6.0 vs newest-1.x.** If byte-exact lockfile parity with the oracle
  matters more than the stdio behaviour, pin 1.6.0 and accept that stdio MCP
  servers emitting non-JSON on stdout will drop their session. This is a real
  regression against today's build, and the oracle has it too.

Every other break at 1.6 has a mechanical, published fix. There is no site that
requires inventing an approach.

**Silent behaviour changes with no compile error and no test coverage** — these
are the real risk of this migration, and all of them need a live MCP server to
observe:

1. `ProtocolVersion::LATEST` moves 2025-03-26 → **2025-11-25** (two protocol
   revisions), because `make_client_info()` uses `Default::default()`.
2. `AuthorizationSession::new` now returns `RegistrationFailed` where 0.10 fell
   back to a hardcoded `client_id: "mcp-client"`.
3. `start_authorization(&[], ..)` with an empty scope slice now auto-derives
   scopes from `WWW-Authenticate` / RFC 9728 resource metadata instead of
   sending none — Phosphor passes `&[]` at `oauth.rs:352`.
4. `discover_metadata` returns `NoAuthorizationSupport` rather than guessing
   endpoints.
5. Junk on a child server's stdout: session-closing at 1.6.0, `-32700` reply at
   1.7/1.8, silent skip at 2.1+.

---

## 6. Sequencing — with no compiler on this host

Nothing below may be attempted here; this is the order for a machine that can build.

0. **Bump `reqwest` 0.12 → 0.13 across the workspace, and verify it builds.**
   This is the prerequisite discovered in §7 and it is the largest and riskiest
   step. Nothing below compiles until it lands. Consider also whether to follow
   the oracle onto `aha-reqwest-eventsource`, and note the rmcp-era rename of
   reqwest's TLS feature (`rustls-tls` → `rustls`) that `lib/rust-genai` forwards.
1. **Decide the target version** (above). Nothing else is safe to start first.
2. **Land the SSE port first, on the current 0.10 pin**, as its own change:
   copy the oracle's `crates/mcp/src/sse_transport/` into Phosphor, add
   `sse-stream = "0.2"`, and repoint `native.rs`'s SSE references at it. Doing
   this while still on 0.10 means it can be verified in isolation, and it removes
   the largest single unknown from the version bump.
3. **Then** change `Cargo.toml:443` to `rmcp = { version = "1.6" }`, update the
   feature lists in `app/Cargo.toml:250,310` and `crates/ai/Cargo.toml:46` to the
   oracle's set (drop `transport-sse-client-reqwest`, add `client-side-sse`, and
   add `reqwest` or `reqwest-native-tls` — `transport-streamable-http-client-reqwest`
   no longer implies a TLS backend at 1.x). Regenerate `Cargo.lock` with cargo;
   do not hand-edit it.
4. Fix the mechanical breaks in this order (each is independently compilable):
   wildcard arms (2 files) → `…Params` builders (2 files) → `Implementation` /
   `ClientInfo` constructors (1 file) → `StoredCredentials::new` and
   `OAuthClientConfig::new` (1 file) → `register_client` arity (1 file).
5. `AuthorizationSession::for_scope_upgrade(..)` at `oauth.rs:406`, and the
   `AuthError`/`OAuthState` wildcard arms if any match is exhaustive.
6. Verify the two behaviours that change silently and that no test covers:
   the negotiated protocol version moves 2025-03-26 → 2025-11-25, and the
   junk-stdout handling. Both need a live MCP server to observe.

Before any of this: `process-wrap` moves 8.2 → 9.0 (`TokioCommandWrap` →
`CommandWrap`), which is internal to rmcp but will show up in lockfile review.

---

## 7. The reqwest prerequisite (found 2026-08-17, since resolved)

The maintainer approved the move to 1.6 and the migration was started. It was
stopped, and the tree restored, on discovering a coupling that none of the
earlier analysis had surfaced. **This has since been resolved by a separate
workspace-wide reqwest bump; the section is kept because it explains why the
two changes are inseparable.**

**`rmcp` 1.6 depends on `reqwest` 0.13; Phosphor is on `reqwest` 0.12.28.**

- `rmcp-v1.6.0:crates/rmcp/Cargo.toml:68` → `reqwest = { version = "0.13.2", … }`.
  `^0.13.2` is not satisfied by 0.12, so this is a hard, semver-incompatible split.
- `Cargo.toml:257` → Phosphor's workspace is `reqwest = { version = "0.12.28", … }`.
- The oracle is **already on reqwest 0.13.3** (`42effe840:Cargo.toml:246`,
  confirmed in its `Cargo.lock`). It also swapped `reqwest-eventsource` for
  `aha-reqwest-eventsource`. So upstream did the reqwest bump *and* the rmcp
  bump; the fork has done neither.

This matters because rmcp does **not** re-export `reqwest`, and every point where
Phosphor hands rmcp an HTTP client is typed on the app's own `reqwest::Client`.
rmcp implements its client traits only for *its* `reqwest::Client` (0.13), which
is a different type. Each of these is a compile error under 1.6:

| site | expression |
|---|---|
| `mcp/http_client.rs:5,18,21` | `StreamableHttpClientTransport<reqwest::Client>`, builds and returns a `reqwest::Client` |
| `templatable_manager/native.rs:1736,1737` | `ReqwestHttpTransport` / `ReqwestSseTransport` aliases |
| `templatable_manager/native.rs:1983,1985,2058` | `rmcp::transport::auth::AuthClient<reqwest::Client>` |
| `templatable_manager/oauth.rs:239,288,312,512` | `AuthClient<reqwest::Client>`, `AuthClient::new(reqwest::Client::new(), …)` |

**Blast radius of the prerequisite.** reqwest 0.12 → 0.13 is not contained to
MCP: **32 files, 172 references**, spanning `app`, `crates/asset_cache`,
`crates/http_client`, `crates/local_control`, `crates/warp_core` and
`lib/rust-genai`, plus `oauth2`'s `reqwest` feature (`Cargo.toml:211`), the
`reqwest-eventsource` dependency, and `lib/rust-genai`'s TLS feature forwarding
(`rustls-tls = ["reqwest/rustls"]` — note 1.6-era rmcp also renamed the reqwest
TLS feature from `rustls-tls` to `rustls`).

### Consequence for the decision

The approved change is **not a contained rmcp migration**. It is:

1. a workspace-wide `reqwest` 0.12 → 0.13 major bump (the large, risky part), then
2. the rmcp 0.10 → 1.6 work catalogued in §4 (mechanical), plus
3. the SSE vendoring in §3 (a verbatim copy of the oracle's 694 lines).

Step 1 must land, and be verified by a build, before steps 2 and 3 are meaningful.
It cannot be attempted on this host.

### The alternative, if the reqwest bump is unwanted

Cargo will happily hold reqwest 0.12 and 0.13 in one graph. Phosphor could add a
second, aliased dependency —
`reqwest_mcp = { package = "reqwest", version = "0.13", … }` — and use it only at
the ~11 MCP boundary sites above. That contains the change to `app/src/ai/mcp/`
but ships two reqwest stacks (and two TLS stacks) in the binary, with proxy and
certificate configuration needing to be applied twice. That is a real
architectural trade-off and belongs to the maintainer, not to this analysis.

### What was and was not done (first attempt, 2026-08-17)

- **Nothing was left changed.** The SSE port was written (verbatim from the
  oracle, verified byte-identical), placed at `app/src/ai/mcp/sse_transport/`,
  registered in `app/src/ai/mcp/mod.rs`, and then **fully reverted** when the
  blocker surfaced — `app/src/ai/mcp/mod.rs` is byte-identical to `HEAD`.
  Leaving an unwired `sse_transport` module in the tree would have been exactly
  the "present, compiles, never reached" failure mode this repo tracks.
- `Cargo.toml` and `Cargo.lock` were never touched.
- The SSE port itself is sound and reqwest-version-neutral where it matters
  (`reqwest_impl.rs` implements the *local* `SseClient` trait for whichever
  `reqwest::Client` is in scope). Only `auth_impl.rs`, which bridges to
  `rmcp::transport::auth::AuthClient`, is tied to rmcp's reqwest version. It can
  be re-applied in minutes once the reqwest question is settled.

---

## 8. Migration as landed (2026-08-18)

The reqwest prerequisite landed first: the workspace moved to
`reqwest = { version = "0.13" }`, `reqwest-eventsource` became
`aha-reqwest-eventsource`, and `oauth2` dropped its `reqwest` feature — all
matching `42effe840`. With reqwest aligned, every rmcp/reqwest boundary site
resolves, because rmcp 1.6 provides `impl StreamableHttpClient for
reqwest::Client` against that same 0.13.

### Manifests

| file | change |
|---|---|
| `Cargo.toml:464` | `rmcp = { git = "…warpdotdev/rmcp…", rev = "c0f65dc…" }` → `rmcp = { version = "1.6" }` — byte-identical to `42effe840:Cargo.toml:420` |
| `app/Cargo.toml` | dropped `transport-sse-client-reqwest` (deleted upstream in v0.11.0), added `client-side-sse`; added `sse-stream = "0.2"` in the `cfg(not(target_family = "wasm"))` section, matching the module's own gate |
| `crates/ai/Cargo.toml` | unchanged — `rmcp.workspace = true`, default features suffice |

`Cargo.lock` was **not** touched. It must be regenerated with cargo, and the
version it resolves decides whether the stdio bug in §2 is live: exactly 1.6.0
closes an MCP session on one unparseable stdout line; 1.7.0+ survives.

### Vendored SSE transport

`app/src/ai/mcp/sse_transport/` — `sse_client.rs`, `client_side_sse.rs`,
`reqwest_impl.rs`, `auth_impl.rs`, copied **byte-for-byte** from
`42effe840:crates/mcp/src/sse_transport/` and verified identical, plus a module
root `sse_transport.rs` (this tree uses `foo.rs` + `foo/`, not `mod.rs`).
Registered in `app/src/ai/mcp/mod.rs` under `#[cfg(not(target_family = "wasm"))]`
beside `http_client` and `reconnecting_peer`.

All five call sites in `templatable_manager/native.rs` were repointed at it — the
`ReqwestSseTransport` alias and the four `SseClientTransport::start*` /
`SseClientConfig` uses — so the legacy-SSE fallback (taken when the Streamable
HTTP preflight 404s) stays reachable. A `use crate::ai::mcp::sse_transport::{…}`
import keeps those call sites at their original width.

### Source changes

| file | change |
|---|---|
| `templatable_manager/native.rs` | `ClientInfo`/`Implementation` literals → `ClientInfo::new` / `Implementation::new`; outer `RmcpError` match gained a `_` arm; SSE paths repointed |
| `templatable_manager/oauth.rs` | 2× `StoredCredentials` → `::new`; 2× `OAuthClientConfig` → `::new(..).with_client_secret(..)`; `register_client` gained its `scopes` argument (`&[]`, preserving DCR behaviour); `AuthorizationSession` literal → `AuthorizationSession::for_scope_upgrade(..)` |
| `templatable_manager/oauth_tests.rs` | 5× `StoredCredentials` → `::new` |
| `server/telemetry.rs` | outer `RmcpError` match gained a `_` arm |
| `blocklist/…/call_mcp_tool.rs` | `CallToolRequestParam { .. }` → `CallToolRequestParams::new(..).with_arguments(..)` |
| `blocklist/…/read_mcp_resource.rs` | `ReadResourceRequestParam { .. }` → `ReadResourceRequestParams::new(uri)` |
| `mcp/reconnecting_peer.rs` | deprecated singular `…Param` type aliases → plural |

Verified as needing **no** change: `RawContent`, `RawResource`, `AnnotateAble`,
`Content::{text,image,resource}`, `ResourceContents` (both variants),
`ErrorData` (still intentionally exhaustive), `Tool::new`, `RawResource::new`,
`ServerCapabilities::builder`, `OAuthTokenResponse` (built via serde),
`CredentialStore`, the `Transport` trait impl, `ConfigureCommandExt`,
`TokioChildProcess::builder(..).stderr(..).spawn()`, and `mcp/http_client.rs`
(uses only reqwest APIs stable across 0.12 → 0.13).

### The five silent behaviour changes

No compile error, no test coverage; each needs a live MCP server to observe.

1. **Negotiated protocol version moves 2025-03-26 → 2025-11-25.**
   `ProtocolVersion::LATEST` changed and `make_client_info` takes the default.
   Two spec revisions in one step; commented at the site.
2. **`start_authorization(&[], ..)` now auto-derives scopes** from
   `WWW-Authenticate` / RFC 9728 protected-resource metadata instead of sending
   none (`oauth.rs`).
3. **`AuthorizationSession::new` returns `RegistrationFailed`** where 0.10 fell
   back to a hardcoded `client_id: "mcp-client"`.
4. **`discover_metadata` returns `NoAuthorizationSupport`** rather than guessing
   endpoint URLs.
5. **Unparseable child-server stdout**: session-closing at exactly 1.6.0,
   `-32700` reply at 1.7/1.8, silent skip at 2.1+. The fork's own patch was the
   silent skip, so 1.6.0 is a regression against the pre-migration build.

### Not verified here

Nothing was compiled. `rustfmt --check` was used only as a parse gate: every
edited file parses, and no formatting diff falls inside an edited region (the
tree carries pre-existing rustfmt drift unrelated to this work). Type checking,
`Cargo.lock` regeneration, and the behaviour above all need a build.
