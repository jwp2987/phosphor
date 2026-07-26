<div align="center">

<img src="assets/phosphor-logo.jpeg" alt="Phosphor" width="160" />

# Phosphor

**A local-first terminal with first-class AI — bring your own model, glowing since the VT100.**

[简体中文](./README.zh-CN.md) · [日本語](./README.ja.md)

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

> Localized READMEs ([简体中文](./README.zh-CN.md) · [日本語](./README.ja.md))
> describe upstream Zap and don't cover these fork experiments.

## Migrating from Zap, OpenWarp, or Warp

If you used this project under an earlier name (**Zap**, or originally
**OpenWarp**), or are coming from upstream **Warp**, see
[docs/migrate-from-warp.md](docs/migrate-from-warp.md) to bring your settings
across. Note: on-disk config, secrets and data still live under the `zap` app id
for now, so existing installs keep working unchanged — the storage rename is
intentionally deferred (only the branding has changed so far).

## Roadmap

See [docs/roadmap.md](docs/roadmap.md).

## Acknowledgements

- [Warp](https://github.com/warpdotdev/warp) — the upstream terminal Phosphor is built on.
- [Zap / OpenWarp (zerx-lab)](https://github.com/zerx-lab/zap) — the BYOP Warp fork this project descends from.
- [DeepSeek-TUI](https://github.com/Hmbown/DeepSeek-TUI) — first-class CLI agent partner.
