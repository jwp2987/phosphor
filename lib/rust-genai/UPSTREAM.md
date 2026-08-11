# Upstream pin — which genai are we forking from?

Warp has `ORACLE.md`: a pinned commit, a policy, and a re-pin procedure, so
parity measurements are a burndown instead of a treadmill. `lib/rust-genai`
had none of that until now — it was vendored at `91a4be9b7` (2026-05-03) by
a commit whose own message says it exists to unblock a broken release build,
not because anyone had measured what they were carrying. This file is the
missing pin.

## Policy

**Pin to an exact upstream *tag*, recorded here with its commit SHA. Never to
`main`/`master`/a branch.**

- genai still publishes proper release tags (unlike Warp, which stopped after
  2026-06-09) — use them. A tag is exact; approximating a date is a fallback
  of last resort, not a default.
- The pin only moves by an explicit, recorded update to this file — never
  implicitly by re-vendoring. Re-vendoring without updating this file is how
  the crate ended up with no oracle in the first place.
- All "what did we change" / "is this a regression" questions are answered
  against the current pin, not against whatever `gh api .../tarball/<ref>`
  happens to return today.
- Porting a fix newer than the pin is fine when there's a reason (a security
  fix, a bug actually hit in production). Note it in the PR. It does not move
  the pin by itself — a deliberate re-pin does.

## Current pin

| | |
|---|---|
| **Version** | `0.7.0-beta.18` |
| **Tag** | [`v0.7.0-beta.18`](https://github.com/jeremychone/rust-genai/releases/tag/v0.7.0-beta.18) |
| **Commit** | `52379bf21b10a8f10312109267f83a2b3456b0f7` |
| **Re-ported into this repo at** | this commit (a full re-port, not a rebase — see "Re-pin history" below) |
| **Delta measured on** | 2026-08-10 |
| **Modified files (src/)** | 15 (down from the prior pin's 18 — see "Re-pin history": several delta files were dropped because upstream now does the same job) |

## Re-pin history

### `0.6.0-beta.18` → `0.7.0-beta.18` (this pin, 2026-08-10)

This was a **re-port, not a rebase**: 0.7 reorganised the adapters directory
behind `adapters/all_adapters.rs` + `adapter/macros/` (a generic
`impl_pass_through_adapter!` macro that generates whole OpenAI/Anthropic
-compatible adapter structs, not just dispatch — see "Dispatcher refactor"
below), and split Anthropic's single `adapter_impl.rs` into
`adapter_shared.rs` / `ant_model.rs` / `ant_reasoning.rs` /
`ant_reasoning_legacy.rs` / `adapter_shared_tests.rs`. Essentially every file
in the prior delta had moved or been restructured, so each Phosphor change
was re-derived from a fresh diff and manually re-applied to its new home
rather than merged.

**Dispatcher refactor — dropped ours, took upstream's.** The prior pin
carried an undocumented, licence-noncompliant local refactor:
`adapter/dispatcher.rs` and `adapter/mod.rs` rewritten to route through a
new `dispatch_adapter!` macro in a wholly-new `adapter/dispatcher_macros.rs`
(see the old "Known gap" section, preserved below for history). Upstream 0.7
independently did the same kind of refactor — its own
`adapters/all_adapters.rs` + `adapter/macros/{dispatcher_macros,
adapter_impl_macros, adapter_kind_macros}.rs` — and goes further, also
macro-generating the ~15 pass-through OpenAI-compatible provider adapters
(Aliyun, BigModel, DeepSeek, Groq, Mimo, Nebius, Together, Xai, ...) that the
prior pin carried as individual per-provider directories. Since upstream's
structure does the same job, **this pin drops the fork's parallel
dispatcher refactor entirely and takes upstream's** — `dispatcher.rs`,
`mod.rs`, and `adapter/macros/` are now byte-identical to pristine upstream,
which also resolves the licence-compliance gap by elimination (nothing
"modified" remains that would need a notice). The 8 now-unnecessary
per-provider directories (`aliyun/`, `bigmodel/`, `deepseek/`, `groq/`,
`mimo/`, `nebius/`, `together/`, `xai/`) were deleted along with them —
upstream's `all_adapters.rs` pass-through list covers all of them by name.

**Dropped, superseded by upstream** (see `CHANGES-PHOSPHOR.md` for detail):
`chat/chat_options.rs`'s `extra_body` field and `chat/tool/tool_base.rs`'s
`cache_control` field are both now upstream's own additions, with identical
field names and semantics (upstream's tool `cache_control` goes further —
it auto-applies a request-level breakpoint to the static tools+system
prefix, which the fork's version didn't do). `adapter/adapters/gemini/
openapi_schema.rs` no longer exists at all — Gemini forwards raw JSON
Schema now (`responseJsonSchema`/`parametersJsonSchema`) instead of
converting to an OpenAPI 3.0.3 subset, so the whole conversion function the
fork's one-line test-rename patched is gone.

**Carried forward, re-applied to the new file layout:** the Vertex
streaming-URL fix (still lives in `adapter/adapters/vertex/adapter_impl.rs`,
still a live upstream bug at `0.7.0-beta.18`), the 1M-context Anthropic beta
header and the screenshot/cache_control-ordering fix (both now in
`adapter/adapters/anthropic/adapter_shared.rs`, the new home for what
`adapter_impl.rs` used to hold), the openai_resp reasoning-object gate
(`adapter/adapters/openai_resp/adapter_impl.rs`, unchanged location), the
BYOP gzip/proxy config (`client/web_config.rs`, `webc/web_client.rs`,
byte-identical files between 0.6 and 0.7 pristine — ported with zero
adjustment), and `chat/usage.rs`'s `extra` field (plus its "mechanical
fallout" `extra: Default::default()` at every `Usage { .. }` construction
site — now including two sites that didn't exist at the prior pin,
`adapter/adapters/bedrock/{streamer,converse}.rs`, since Bedrock is new
since `0.6.0-beta.18`).

### Why a commit *and* a tag

genai's tags are exact releases from crates.io, so there's no dating
approximation to make (contrast `ORACLE.md`'s Warp pin, which had to locate an
un-tagged commit by date). Recording both the tag and its resolved commit SHA
means the pin survives even if a tag were ever force-moved upstream — compare
against the SHA, not the mutable ref.

### How this pin was derived

`gh api` against `api.github.com` works from this environment; a plain `curl`
to `crates.io` does not (timed out / no response — untested why, but don't
rely on it). Fetch upstream via the GitHub tag, not crates.io:

```
gh api repos/jeremychone/rust-genai/tarball/v0.7.0-beta.18 > genai-0.7.0-beta.18.tar.gz
gh api repos/jeremychone/rust-genai/git/refs/tags/v0.7.0-beta.18 --jq '.object.sha'
```

Extract it, then diff the whole `lib/rust-genai/` tree against it (not just
`src/` — `Cargo.toml`, `README.md`, and the `doc/` → `docs/` rename all carry
real differences too):

```
diff -rq <pristine-extract> lib/rust-genai -x .git -x target -x .github
```

Do **not** diff against a vendoring/porting commit alone and call it the
delta — measure against a fresh pristine extract of the pinned tag every
time (see "Historical: the `91a4be9b7` vendoring gap" below for why this
matters — it's exactly the mistake that produced a wrong, understated delta
once already).

## Historical: the `91a4be9b7` vendoring gap (resolved at the `0.7.0-beta.18` re-pin)

This section is kept for the record; the gap it describes no longer exists
in the tree — see "Re-pin history" above.

The crate was originally vendored at `91a4be9b7` (2026-05-03) from pristine
`v0.6.0-beta.18`, and that vendored tree was not a clean import — three files
differed from pristine without the delta being attributable to any later
Phosphor commit: `src/adapter/dispatcher.rs` and `src/adapter/mod.rs` were
rewritten to route through a new `dispatch_adapter!` macro (avoiding N-way
repeated `match` arms across every `AdapterKind` variant), and
`src/adapter/dispatcher_macros.rs` (the macro itself, using the `paste`
dependency) didn't exist upstream at all. The refactor was
behavior-preserving but undocumented: not in `CHANGES-PHOSPHOR.md`'s file
list, and the three files carried no Apache-2.0 §4(b) modified-file notice,
unlike every other file in the true delta — a licence-compliance gap, not
just a documentation gap.

The `0.7.0-beta.18` re-pin resolved this **by elimination**: upstream 0.7
independently ships the equivalent (and broader) macro-based refactor behind
`adapters/all_adapters.rs` + `adapter/macros/`, so this pin drops the fork's
parallel dispatcher rewrite entirely and takes upstream's structure instead.
`dispatcher.rs` and `mod.rs` are now byte-identical to pristine upstream —
nothing "modified" remains there that would need a notice.

## Re-pinning procedure

1. **Find the target tag.** `gh api repos/jeremychone/rust-genai/tags --paginate --jq '.[].name' | sort -V | tail -20`.
   Prefer the newest *stable* (non-`-beta`/`-alpha`) unless there's a specific
   reason to track a beta (e.g., we're already carrying a beta-only fix).
2. **Fetch it** via the `gh api .../tarball/<tag>` recipe above — `curl` to
   crates.io has not worked from this environment; re-test before assuming it
   never will, but don't block on it.
3. **Re-measure the delta**: diff the fresh pristine extract against
   `lib/rust-genai/`, file by file, not just by directory listing — a file can
   exist in both trees and still differ.
4. **Re-classify every changed file** the same way this document's companion
   classification did: PHOSPHOR-SPECIFIC (genuinely ours), UPSTREAMABLE (send
   it to jeremychone — carrying it forever is pure cost), ALREADY FIXED
   UPSTREAM (drop our patch, take theirs), UNKNOWN/RISKY (say so, don't guess).
   Check the new tag's `CHANGELOG.md` top section first — it is dense and
   often answers "did upstream already fix this" directly, e.g. `0.7.0-beta.x`
   independently added `Tool::with_cache_control` and `ChatOptions::extra_body`
   with the *same field names and semantics* as our own additions.
5. **Update this file**: new version, tag, commit SHA, delta file count, date.
6. **Update `CHANGES-PHOSPHOR.md`** to match the new delta. Never let it drift
   back into being a "lower bound" — if a full re-derivation isn't done, say
   so explicitly in the file rather than presenting a partial list as complete.
7. Confirm every genuinely-modified file still carries its Apache-2.0 §4(b)
   notice (`grep -L "MODIFIED by the Phosphor fork" <changed files>`) before
   calling the re-pin done.

## What this pin does and does not buy

Pinning to a tag doesn't freeze what upstream ships — genai moves fast (beta
releases roughly weekly at the observed cadence, i.e. similar order of
magnitude to Warp's release cadence, though nowhere near Warp's per-day test
growth since genai is a much smaller crate). What the pin buys is the same
thing `ORACLE.md` buys for Warp: "what changed" becomes a diff against a fixed
point instead of a moving one, so the answer doesn't silently change every
time someone re-fetches.
