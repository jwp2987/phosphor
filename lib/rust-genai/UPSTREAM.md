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
| **Version** | `0.6.0-beta.18` |
| **Tag** | [`v0.6.0-beta.18`](https://github.com/jeremychone/rust-genai/releases/tag/v0.6.0-beta.18) |
| **Commit** | `cb343d74c15fed24b926e63b9132a9eab100204f` |
| **Vendored into this repo at** | `91a4be9b7` (2026-05-03, "fix(ci): commit lib/rust-genai local fork to fix the release build") |
| **Delta measured on** | 2026-08-10 |
| **Modified files (src/)** | 18 (15 named in the original `CHANGES-PHOSPHOR.md`, plus 3 undocumented: `adapter/dispatcher.rs`, `adapter/mod.rs`, and a wholly new `adapter/dispatcher_macros.rs`) |

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
gh api repos/jeremychone/rust-genai/tarball/v0.6.0-beta.18 > genai-0.6.0-beta.18.tar.gz
gh api repos/jeremychone/rust-genai/git/refs/tags/v0.6.0-beta.18 --jq '.object.sha'
```

Extract it, then diff the whole `lib/rust-genai/` tree against it (not just
`src/` — `Cargo.toml`, `README.md`, and the `doc/` → `docs/` rename all carry
real differences too):

```
diff -rq <pristine-extract> lib/rust-genai -x .git -x target -x .github
```

Do **not** diff against the vendoring commit `91a4be9b7` alone and call it the
delta — that commit was already dirty when it landed (see "Known gap" below),
so a diff against it understates the true change set. This is exactly the
mistake the original `CHANGES-PHOSPHOR.md` made: it was derived from `git diff
91a4be9b7..HEAD`, and its own text called itself "a lower bound" as a result.
The corrected list in `CHANGES-PHOSPHOR.md` is the pristine-upstream diff, not
the vendoring-commit diff.

## Known gap in the vendoring commit

`91a4be9b7`'s vendored tree was not a clean upstream import — three files
differ from pristine `v0.6.0-beta.18` without the delta being attributable to
any later Phosphor commit:

- `src/adapter/dispatcher.rs` and `src/adapter/mod.rs` were rewritten to route
  through a new `dispatch_adapter!` macro (avoiding N-way repeated `match`
  arms across every `AdapterKind` variant).
- `src/adapter/dispatcher_macros.rs` (the macro itself, using the new `paste`
  dependency) does not exist upstream at all.

The refactor is behavior-preserving — every `AdapterKind` variant maps to the
same adapter struct before and after — but it was undocumented: not in
`CHANGES-PHOSPHOR.md`'s file list, and **the three files carry no Apache-2.0
§4(b) modified-file notice**, unlike every other file in the true delta. That
is a licence-compliance gap, not just a documentation gap — see the note in
`CHANGES-PHOSPHOR.md`.

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
