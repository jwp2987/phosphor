<div align="center">

<img src="assets/phosphor-logo.jpeg" alt="Phosphor" width="160" />

# Phosphor

**A local-first terminal with first-class AI — bring your own model, glowing since the VT100.**

<sub><i>Based on <a href="https://github.com/warpdotdev/warp">Warp</a> (via <a href="https://github.com/zerx-lab/zap">Zap</a> / OpenWarp); evolving independently as its own project.</i></sub>

</div>

> [!NOTE]
> **This is a personal project, mostly for fun and tinkering.** It's a playground
> for poking at Phosphor's BYOP (bring-your-own-provider) AI stack — prompt rendering,
> caching behavior, and provider plumbing — not a polished product. Things here
> are experimental and may change, break, or get thrown away. If you want the real
> thing, go upstream: [zerx-lab/zap](https://github.com/zerx-lab/zap). Everything
> below the upstream overview is what this fork has been messing with.
>
> Most of it is being tested against **FastFlowLM (FLM)** as a local BYOP
> endpoint, and several of the changes below exist specifically to play nicer
> with it — especially the prompt-cache / environment-context work, which is
> tuned for FLM's text-only partial-prefill (KV-cache) behavior.

Phosphor is an open, local-first terminal with first-class AI and agent support. Plug in any AI provider, bring in any CLI agent, manage SSH hosts inside the terminal — with keys, history and agent state staying on your machine by default.

## What Phosphor adds over upstream Warp

- **No mandatory cloud** — no account, login, Drive sync or cloud agent history required.
- **BYOP AI providers** — any OpenAI-compatible endpoint, plus native OpenAI / Anthropic / Gemini / DeepSeek / Ollama protocols, **and Google Vertex AI** (Gemini + Claude, authenticated through `gcloud` — no static key). Keys stay local.
- **Third-party CLI agents** — DeepSeek-TUI / Codex CLI / Claude Code / Google Antigravity (`agy`) wired into Blocks and the notification center.
- **Built-in SSH host manager** — manage hosts, configs and sessions inside the terminal, with tmux integration.
- **Editable system prompts** — minijinja templates rendered on the client.
- **Rendering fixes** — tuned Markdown pipeline; CJK soft-wrap caret and bold subpixel fixes.
- **Localized UI** — English / Simplified Chinese / Japanese out of the box, community-extensible.
- **Privacy defaults** — Cloud Agent / Computer Use / Referral / telemetry off by default.

## What this fork is playing with

All experimental, spread across feature branches. Roughly in order of how much
fun they were:

### Google Vertex AI (BYOP)
- **First-class Vertex AI provider** — pick "Vertex AI" as a provider type, enter
  your GCP **project** + **location** (and optionally a service-account email to
  impersonate), and add models like `gemini-2.5-flash` or `claude-sonnet-4-6`.
  The publisher (Gemini vs. Claude) is routed by model name automatically.
- **No static key** — Vertex uses short-lived OAuth2 bearer tokens, minted via
  the `gcloud` CLI (Application Default Credentials / the active account / SA
  impersonation) and cached in-process. Nothing to paste, nothing stored.
- Reuses the whole BYOP path — model picker, reasoning tiers, attachment caps —
  dispatched by model family. Includes a fix so streaming Claude on Vertex uses
  `:streamRawPredict` (Google's streaming method) rather than the unary endpoint.

### Prompts & providers
_Same FastFlowLM motivation as above, from the other direction: small models
served by FLM have tight context windows, and a bloated built-in system prompt
eats budget the actual conversation needs. Being able to hot-swap a leaner prompt
per model — live, without a rebuild — is the whole point here._
- **Per-profile, per-prompt system prompt overrides.** Every prompt slot in a
  profile can be set to **Auto** (pick a built-in by model family), a specific
  **built-in** family (`default` / `anthropic` / `lean` / `gpt` / `beast` /
  `codex` / `gemini` / `kimi` / `trinity` / `local` / `troubleshooting`), or a
  **custom file**. This
  covers the agent slots (base / coding / full-terminal-use / computer-use) and
  the auxiliary prompts (title generation, prompt suggestions, input completion,
  relevant files, workflow metadata, next command) — each picked independently in
  the profile editor's new **System Prompts** section.
- **Custom prompt files, hot-loaded.** Drop `.j2` / `.md` files into a `custom/`
  folder under your prompt template directory and they show up in every picker.
  They render through the same minijinja env as the built-ins (so
  `{% include "partials/..." %}` still works) and are re-read live — no rebuild.
  Every override degrades gracefully (bad name, missing file, `..` traversal,
  syntax error → falls back to the built-in / auto pick).
- **Active-AI prompts folded into the hot-reload env.** The command-suggestion /
  input-completion / relevant-files / next-command / workflow-metadata templates
  used to be baked into a separate environment; they now hot-reload from the
  prompt directory like everything else.
- **`lean.j2`** — a trimmed agent system prompt to A/B against the verbose
  default, for exactly this: keeping the system prompt small enough that a
  small FLM model doesn't blow its context window before the conversation starts.
- **`troubleshooting.j2`** — a built-in example of a task-focused prompt: a
  diagnose-and-fix agent (observe the failure, one hypothesis at a time, change
  one thing, verify). Pair it with a "Troubleshooting" profile. It's opt-in per
  slot, never auto-picked by model family — a template to copy and riff on.
- **Tool-list dedup** — when structured tools are sent, the redundant
  `# Available Tools` text block is suppressed to save prompt tokens.

### BYOP wire inspector
- A **live capture window** for outbound/inbound LLM traffic: the exact system
  prompt, structured tools, the environment block, one-shots (title gen, active
  AI), and streamed responses. Filterable, pausable, copyable — opened from the
  left panel header. Handy for seeing what actually goes over the wire.

### Prompt-cache & environment-context stability
_Mostly driven by testing against FastFlowLM (FLM): it partial-prefills a stable
prefix, so anything volatile near the front of the prompt tanks the cache. Note
this assumes a **text-only** FLM model — VL/MoE engines that can't partial-prefill
don't benefit from the tail-block design._
- Volatile environment context moved **out of the system prompt into a tail
  block** (and appended as a standalone message) so upstream KV-cache breakpoints
  stay stable instead of being invalidated every turn.
- Environment context is **frozen/sticky per conversation** rather than refilled
  from shifting block metadata, so replays stop drifting.
- Tolerate null optional tool args; resolve tool-call ownership so assistant text
  can't orphan results.

### Terminal UI (TUI)
- **`zap-tui-oss` — a keyboard-driven terminal frontend**, ported from upstream
  Warp's `warp_tui` crate and rewired onto the BYOP stack (no cloud
  orchestration). It shares the GUI's app identity, so your models, config and
  providers load unchanged; it boots interactive with shell/path Tab-completion
  and the full non-cloud slash-command set (`/model`, `/profile`, `/prompts`,
  `/init`, `/compact-and`, `/queue`, `/fork`, `/fork-and-compact`, `/fork-from`,
  `/rewind` with file-revert, …). Cloud-only surfaces (codebase search, cloud
  conversation history, credits/usage) are intentionally dropped for BYOP.
- Build/run it with `cargo run -p warp_tui` (the crate is a non-default
  workspace member; the GUI build is unaffected). Agent runs go through the same
  BYOP path as the GUI (`chat_stream` / `oneshot` / the prompt-override system).

### Odds and ends
- **Title-generation language fix** (this branch's namesake) — stop Chinese-biased
  few-shot examples leaking into tab titles.
- **English-only** BYOP tool schemas and skill wrapper.
- **Logging** — the GUI always logs to a file now, with a `ZAP_LOG_STDOUT` escape
  hatch for stdout.
- **Perf** — reuse HTTP clients, unblock async DNS, trim webfetch/history
  overhead, cap `file_glob` results and back off failed retries.

## Migrating from Zap, OpenWarp, or Warp

If you used this project under an earlier name (**Zap**, or originally
**OpenWarp**), or are coming from upstream **Warp**, see
[docs/migrate-from-warp.md](docs/migrate-from-warp.md) to bring your settings
across.

> [!IMPORTANT]
> **The storage identity rename landed on 2026-08-14, and there is no automatic
> migration.** Earlier builds stored everything under a `zap` identity; this one
> uses `phosphor`, so **a build from before that date and a build after it do not
> see each other's data**. On first launch the app starts fresh: no settings, no
> history, no saved API keys.
>
> This is deliberate — it is a beta, and writing a migration that moves OS
> keychain entries correctly was judged not worth the risk of doing it wrong.
> Nothing is deleted; the old directories are left untouched.

| | before | after |
|---|---|---|
| app id | `dev.zap.Zap` | `dev.phosphor.Phosphor` |
| binary | `zap-oss` | `phosphor-oss` |
| config / data / state (Linux) | `~/.config/zap`, `~/.local/share/zap`, `~/.local/state/zap` | the same paths under `phosphor` |
| skills, prompts, `.mcp.json` | `~/.zap/` | `~/.phosphor/` |
| keyring service | `dev.zap.Zap` | `dev.phosphor.Phosphor` |

To carry your files across by hand, before first launch:

```bash
cp -r ~/.config/zap ~/.config/phosphor
cp -r ~/.zap ~/.phosphor
```

Then repoint `prompt_template_dir` in `settings.toml` — it stores an absolute
path into the old directory, and because the copy leaves the original in place it
will keep silently reading the old one. **API keys cannot be copied this way**:
OS keychain entries are keyed by service name, so those are re-entered.

Deliberately *not* renamed, and not bugs: the TUI binary (`zap-tui-oss`), the
`ZAP_LOG_STDOUT` escape hatch, the `warp_*` / `zap_*` crate names, and the
`WARP_*` build variables. Those are lineage internals with no user-visible
surface — see `SCOPE.md` layer 4 and
[`specs/phosphor-rebrand/MERGE-CHECKLIST.md`](specs/phosphor-rebrand/MERGE-CHECKLIST.md),
which also lists the identifiers that must *stay* on the old name because
renaming them would silently lose data.

## Roadmap

**There is no separate roadmap document.** Current work lives in
[`TODO.md`](TODO.md); current position lives in [`docs/STATE.md`](docs/STATE.md),
which is generated.

Two former roadmaps are accounted for, so nobody hunts for them:

- `docs/roadmap.md` was **Zap's** roadmap with the name find-replaced during the
  2026-07-25 rebrand. It described a direction this fork has not taken — a hosted
  agent runtime, shared identity across surfaces, shareable session links — much
  of which has since been explicitly declined here. **Removed 2026-08-11.**
- [`specs/ROADMAP.md`](specs/ROADMAP.md) was the 2026-08-02 tier-migration plan —
  an **earlier form of `TODO.md`**, kept for its history. Superseded; do not plan
  from it.

## Repository map — what each document is for

This fork carries a lot of process documentation, because it is tracking a
moving upstream and most of it exists to stop a wrong answer being re-derived.
Start here.

**Read before doing parity work** — `CLAUDE.md` is the index and says *why* each
of these exists, which matters more than what it contains.

| file | what it is for |
|---|---|
| **[`docs/STATE.md`](docs/STATE.md)** | **Where the project is right now** — parity, guard status, open work, and whether the tree has actually been verified. **Generated by `script/state`; never edit it.** Read this first. |
| [`CLAUDE.md`](CLAUDE.md) | The required-reading index. Each row says why that document exists. |
| [`AGENTS.md`](AGENTS.md) | Working rules. §5.6 never weaken a test to go green, §5.10 no silent regressions, §5.11 every defect gets an issue. |
| [`ORACLE.md`](ORACLE.md) | The pin. Parity is measured against Warp `02b53fcd8`, **never `warp/master`** — master moves 50-80 tests/day, so measuring against it produces a gap that never shrinks. |
| [`DECLINED.md`](DECLINED.md) | Deliberate non-parity. What is absent **on purpose**, with machine-checkable markers so a decision cannot be silently reversed. Check it before filing parity debt. |
| [`TODO.md`](TODO.md) | The work ledger, and the project's definition of done. **Verify any entry against the code before acting on it** — entries here have stated the opposite of the tree more than once. |
| [`HANDOFF.md`](HANDOFF.md) | State of `main` and the operational lessons — the traps that have actually cost time. |

**Measuring the gap to the pin.** Five layers, in increasing order of authority:

| file | authority |
|---|---|
| `SCOPE-{AI,TERMINAL,REST}.md` | Per-file verdicts for all 854 test-bearing files at the pin. **Known overstated** — see the staleness banners. |
| [`docs/SWEEP-INVENTORY.md`](docs/SWEEP-INVENTORY.md) | Mechanical name-diff. Its `?` buckets over-reported portability by ~18x and are superseded. |
| [`docs/sweep/`](docs/sweep/) | Six per-area hand adjudications with per-test evidence. |
| [`docs/SWEEP-SUMMARY.md`](docs/SWEEP-SUMMARY.md) | The consolidated narrative. |
| **`docs/sweep-verdict-ledger.tsv`** | **The authority.** One row per absent pin test with its verdict, validated by `script/check_sweep_ledger` and consumed by the re-pin tooling. |

**Re-pin tooling** — built so the next pin move does not repeat this work:
`script/generate_repin_queue` (diffs two pins and carries forward ledger
verdicts), `script/generate_pin_identity_manifest` →
[`docs/PIN-IDENTITY-MANIFEST.md`](docs/PIN-IDENTITY-MANIFEST.md) (which files are
byte-identical to the pin and can be fast-forwarded), and
`script/check_declined_collisions` (flags an incoming pin change that collides
with a recorded decision).

**Build triage** — [`docs/build/TRIAGE.md`](docs/build/TRIAGE.md) collates every
agent's ranked "what I am least sure compiles" into one list grouped by file, so
a large batch of unbuilt work is worked through in a planned order. Predictions
get marked hit/missed after the build, which measures how far unbuilt agent
output can be trusted.

**Guards** — run by `script/precheck` and CI, each with a header comment
explaining what an earlier version got wrong: `check_cloud_boundary`,
`check_stub_coverage`, `check_declined_collisions`, `check_sweep_ledger`,
`check_settings_registry`, `check_channel_command_names`.

**Everything else:** [`CONTRIBUTING.md`](CONTRIBUTING.md),
[`docs/DESIGN-PHOSPHOR-FORK.md`](docs/DESIGN-PHOSPHOR-FORK.md),
[`docs/migrate-from-warp.md`](docs/migrate-from-warp.md),
[`docs/FLEET-ROUND.md`](docs/FLEET-ROUND.md) (how to run a parallel agent round),
[`docs/DEAD-CODE-AUDIT-207.md`](docs/DEAD-CODE-AUDIT-207.md),
[`docs/licensing-open-questions.md`](docs/licensing-open-questions.md),
[`docs/TODO-ARCHIVE.md`](docs/TODO-ARCHIVE.md) (completed and superseded work —
kept because much of it records *how a wrong answer was corrected*), and
`specs/**` for per-feature design notes.

## Parity with upstream Warp, and how it is verified

Phosphor tracks a **pinned** Warp release rather than `master`: Warp
**`2026.07.29.09.05` stable**, commit **`02b53fcd8`**. The pin is deliberate —
`master` is unreleased trunk moving 50–80 tests a day, so measuring against it
produces a gap that never closes. See [`ORACLE.md`](ORACLE.md) for the re-pin
policy.

**Tests carried across from upstream** (figures generated by `script/state` into
[`docs/STATE.md`](docs/STATE.md), 2026-08-11 snapshot):

| | count |
|---|---:|
| tests in the pinned Warp release | 10,026 |
| tests in this fork | 9,739 |
| **shared with the pin** | **7,785** |
| absent from the fork | 2,241 |

That is **~89.9% of the pin's non-cloud tests**, and **~96.2% present or
deliberately resolved** once the declined, divergent and covered-elsewhere
buckets are counted. Of the 2,241 absent, 1,130 are cloud surfaces this fork
drops by design and 417 are recorded decisions in [`DECLINED.md`](DECLINED.md);
the genuinely open bucket is **50 tests** under MISSING-SUBSYSTEM.

Two honest caveats, both of which the project enforces on itself: a shared test
*name* is not proof of a shared *assertion*, so `docs/sweep-verdict-ledger.tsv`
is the authority on any individual test; and the 398 unadjudicated absences are
projected at the same cloud ratio rather than counted.

### What actually runs

Ported tests are worth nothing unless they execute. Every push is checked by:

| gate | scope |
|---|---:|
| `cargo nextest` — `warp` + `warp_tui` | **6,411 tests** |
| `cargo nextest` — 40 workspace packages | **2,436 tests** |
| `cargo nextest` — `warpui_core` (TUI backend) | **565 tests** |
| `cargo check` | Linux, **Windows** and **macOS** |
| fork boundary guards | 6 scripts, see below |

**9,412 tests green in CI**, plus a GUI integration suite
(`cargo nextest -p integration`, 241 scenarios driving the real app under Xvfb)
that is reported but not yet gating, because it has no triaged baseline.

The suite gates fail on **change**, not on redness: `script/check_test_failures`
diffs against `script/known_test_failures.txt`, so a known red cannot quietly
become a merge blocker for everyone and a *new* red cannot hide among old ones.
Alongside them run `check_cloud_boundary` (no new imports of dropped cloud
modules), `check_stub_coverage` (no tests asserting against gutted no-op stubs),
`check_declined_collisions` (a recorded decision cannot be silently reversed),
`check_sweep_ledger`, `check_settings_registry` and
`check_channel_command_names`.

## AI-assisted development

**Most of the code and documentation in this fork was written with AI
assistance** — primarily Anthropic's Claude, via Claude Code — with a human
directing the work, making the product decisions, and reviewing what lands. Some
changes are written almost entirely by agents working in parallel; the layer-3
identity rename above was done that way, split across four agents and merged in
one round.

This is stated plainly because it changes how you should read the repository:

- **Treat confident-sounding documentation as a claim, not a fact.** Several
  documents here have asserted the exact opposite of the code. That is why
  [`docs/STATE.md`](docs/STATE.md) is generated rather than written, why
  [`TODO.md`](TODO.md) carries "verify any entry against the code before acting
  on it", and why so many comments in this codebase explain *how an earlier
  answer was wrong* instead of only what the code does.
- **The process documents exist to make this workflow safe**, not as ceremony.
  [`AGENTS.md`](AGENTS.md) §5.6 (never weaken a test to go green) and §5.10 (no
  silent regressions) exist because agents will otherwise make a red test green
  the easy way. [`DECLINED.md`](DECLINED.md) carries machine-checkable markers
  because a decision recorded only in prose gets silently re-litigated.
  [`docs/FLEET-ROUND.md`](docs/FLEET-ROUND.md) describes how a parallel round is
  actually run.
- **Agent output is not trusted until it compiles and the suite is green.**
  Work regularly lands unbuilt and is verified afterwards;
  [`docs/build/TRIAGE.md`](docs/build/TRIAGE.md) exists to score how often the
  agents' own confidence predictions were right.

The verification above is the answer to the obvious objection. ~9,400 executing
tests, three-platform compile checks and six mechanical guards are not a
substitute for review, but they are why machine-written change is allowed to land
here at all — and they are enforced by scripts rather than by anyone remembering
to look. Several of those guards exist *because* an agent previously found the
easy way around the check that preceded them.

Human review is the gate, but it is a real gate on a large volume of machine-
written change — so bugs of the kind a reviewer skims past are more likely here
than in a hand-written codebase of the same size. Weigh that before running it
somewhere that matters. The AGPL's no-warranty terms are not a formality here.

Contributions are welcome with or without AI assistance; see
[`CONTRIBUTING.md`](CONTRIBUTING.md).

## Licensing

Phosphor inherits Warp's split licensing:

- The UI framework crates — `crates/warpui` and `crates/warpui_core` — are
  licensed under the [MIT license](LICENSE-MIT).
- **Everything else** in this repository is licensed under
  [AGPL-3.0-only](LICENSE-AGPL).

Note that the split is narrower in practice than it looks: both MIT crates
depend on `markdown_parser` and `sum_tree`, which are AGPL-3.0-only, so a build
that links them is AGPL. **The distributed binary as a whole is AGPL-3.0-only.**

Copyright on the inherited code remains with Denver Technologies, Inc. (Warp).
Phosphor is a modified version, not the original — see
[Acknowledgements](#acknowledgements).

### Corresponding source (AGPL §13)

Phosphor is a modified version of an AGPL program, and it can be interacted
with remotely over a network (the agent/CLI session daemon). AGPL-3.0 §13
therefore requires that users interacting with it that way be offered the
Corresponding Source of *this* modified version. That offer is this repository:

**https://github.com/jwp2987/phosphor**

The complete corresponding source for any distributed build is the commit that
build was made from, in that repository, under the terms above.

Third-party components bundled with a release are listed in
`THIRD_PARTY_LICENSES.txt`, generated at package time by
`script/prepare_bundled_resources`. Known-unresolved attribution questions are
tracked in [docs/licensing-open-questions.md](docs/licensing-open-questions.md).

## Acknowledgements

- [Warp](https://github.com/warpdotdev/warp) — the upstream terminal Phosphor is built on.
- [Zap / OpenWarp (zerx-lab)](https://github.com/zerx-lab/zap) — the BYOP Warp fork this project descends from.
- [DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI) — first-class CLI agent partner.
