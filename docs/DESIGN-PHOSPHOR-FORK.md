# Phosphor fork — design decisions & direction

English design record for this fork. Companion to `AGENTS.md` (the code map)
and the `specs/` docs. Read this to understand *why* the fork is shaped
the way it is and where it is heading, before making architectural changes.

## 1. Identity & lineage

- **Upstreams (two, different roles):**
  - `warp` remote → `warpdotdev/warp` — **Warp OSS proper**. The original, still
    moving daily. Cloud-agent product.
  - `upstream` remote → `zerx-lab/zap` — the **"Zap" fork** this repo descends
    from. BYOP-native, but not actively developed (mostly bug-fix branches).
  - `origin` → this personal fork (`jwp2987/phosphor`, previously `jwp2987/zap`).
- **What this fork is:** a **BYOP** (Bring-Your-Own-Provider) terminal/agent. The
  cloud half of Warp (orchestration, Drive sync, auth/billing, remote agents) is
  stripped; the agent talks **directly to a user-configured OpenAI-compatible /
  Anthropic / Gemini / Ollama / Vertex endpoint** via the vendored `lib/rust-genai`.
  Primary test target: **FastFlowLM (FLM)**, a local small-model server.
- **North star — match Warp minus cloud.** Phosphor should stay as close to
  feature-parity with upstream Warp as possible, dropping **only genuinely
  cloud-dependent** features. When a Warp feature is missing from Phosphor, the default
  is to **build or port it for parity**, not stub or drop it. Drop only cloud:
  orchestration, remote/RunAgents/StartAgent child agents, `server_api`,
  connected self-hosted workers, ambient agents, cloud harness, oz child-launch,
  Drive sync, auth/billing. If a feature is cloud-*adjacent* (e.g. conversation
  restoration/selection may tie to cloud sync), check whether a local-only version
  is viable before dropping.
- **Direction:** started as personal tinkering; now moving toward a **real,
  distributable product**. This raises the bar: source-disclosure discipline
  (AGPL — see §6), upstream convergence, and maintainability all matter now.

## 2. BYOP architecture (the core of the fork)

- Requests go **straight to the configured `base_url`** through
  `chat_stream::build_client` (genai `ServiceTargetResolver`). No cloud proxy.
  Only external metadata call is the models.dev catalog fetch.
- **API keys** live in OS secure storage (`AgentProviderSecrets`), never in
  `settings.toml`. Provider list + non-secret config live in `settings.toml`
  under `agents.warp_agent.providers`.
- **Per-profile / per-prompt system prompt overrides**: Auto / built-in family /
  custom file per model slot, stored in the sqlite object store, hot-reloadable
  from a prompt template dir (`ZAP_PROMPT_DIR` or the settings panel). Path-guarded
  (no absolute / `..`). Motivation: keep small FLM-served models from drowning in
  a 9k-token default prompt.
- **Prompt-cache discipline**: volatile env (time, git) is kept out of the stable
  system-prompt prefix so upstream KV/prompt caches survive.

## 3. Warp OSS sync strategy

The fork is ~1700 commits behind current Warp OSS across a hard product split, so
**wholesale rebase is impossible**. Instead:

- **General fixes → prefer `zerx-lab/zap`**, not Warp OSS. Same lineage, BYOP-native,
  clean cherry-picks. Warp-OSS picks conflict heavily (architecture rewrites).
- **Reactive, not systematic**: cherry-pick a fix when a bug actually bites, using
  `git cherry-pick -x` for provenance. The clean-apply zone is narrow (see
  `specs/warp-oss-sync/SCOPE.md` for the measured yield: ~12/23 candidates landed,
  mostly `context_chips`).
- **TUI → only Warp OSS has it.** No BYOP-native TUI exists upstream, so the TUI is
  a port from Warp OSS with cloud→BYOP rewiring (see §4).

## 4. TUI port — guiding principles

Porting Warp's `warp_tui` (a ratatui terminal front-end) onto Phosphor. The crate rides
on ~3 months of `warpui_core` refactoring Phosphor never took, so the port is really a
**targeted re-convergence of the UI core**. Governing rules:

- **Isolate, don't refactor the GUI.** Every change is either `#[cfg(feature =
  "tui")]`-gated or a pure widening. The default GUI build must stay green — this
  is verified with a **GUI regression gate** (`cargo check -p warp --features gui`)
  after every change that touches shared `warpui_core` code. This gate has already
  caught one real regression; trust it.
- **Widen, don't fork, core bounds.** The view read/update/storage path was relaxed
  `T: View` → `T: Entity` (View: Entity, so a pure widening) so TUI views
  (`TuiView: Entity`, not `View`) can use `ViewHandle`/`ViewContext`. This mirrors
  upstream and keeps one code path.
- **Separate storage over enum surgery.** Rather than convert the GUI's
  `Window.views` map to upstream's `StoredView` enum (~83 hot-path sites), TUI views
  live in a **separate gated `Window.tui_views` map**. Chosen deliberately to keep
  GUI blast radius at zero. (Trade-off: diverges from upstream on this one point;
  the tui-only files that reference it are ours to maintain anyway.)
- **Trim imported crates to what's actually used.** `warp_search_core` was imported
  as the **inline_menu subset only** — the Tantivy full-text `searcher`/`macros`
  half is dropped, since `warp_tui` only needs the menu system. Keeps the dep
  surface (and build) small.
- **Adapt small missing deps, don't import heavy ones.** `warp_errors` (a Sentry
  helper crate) is adapted away to `log::error!` rather than imported.
  `warp_core::async::debounce` (129 lines) is vendored locally rather than pulling
  a missing module.
- **English comments.** All comments/docs added in this fork's feature work are
  English (the maintainer does not read the Chinese in the inherited codebase).

### Phase status (see `specs/warp-oss-sync/SCOPE.md` for detail)

- Phase 1 (warpui_core tui foundation): **done**, GUI-verified.
- Phase 2 (app-crate `tui` feature): **done**.
- Phase 3 (build warp_tui, drop cloud, rewire agent→BYOP): **in progress**
  (3a: trimmed warp_search_core imported).
- Phase 4 (entry bins + workspace wiring + full build): pending.

## 5. Upstream convergence direction

The fork increasingly re-adopts upstream's *shared infrastructure* where it lowers
long-term cost, while keeping the BYOP product identity. Concretely:

- `warpui_core` migrated to edition 2024 (matching upstream).
- View-core bounds relaxed to match upstream's Entity-generic design.
- `warp_search_core` now exists in Phosphor as a shared crate (as upstream intended).
- **Deferred convergence project:** Phosphor still has its own `inline_menu` inside the
  app crate (~53 files). Upstream shares it via `warp_search_core`. Eventually the
  GUI could migrate onto the shared crate and the legacy app-crate inline_menu be
  dropped — but that is a **large, separate refactor with a different API**, kept
  decoupled from the TUI port on purpose. Not a quick follow-up.

## 6. Licensing guardrail

Dual-licensed: **AGPL-3.0-only** (the `app`/`warp` crate and most of `crates/*`)
and **MIT** (`warpui`, `warpui_core`); `lib/rust-genai` is MIT/Apache-2.0.
Copyright: Denver Technologies (Warp). Because the shipped binary links the AGPL
app crate, **the distributed whole is AGPL**. Implications for the product
direction:

- Distributing a modified build requires offering **complete corresponding source
  under AGPL**. No closed-source distribution of a modified Warp/Phosphor.
- **Do not** use Warp's name/branding/trademarks; pick a distinct fork name. AGPL
  §7 declines trademark grants.
- Keep copyright/license notices intact.
- A public source repo (like this fork) already satisfies AGPL for personal use;
  the obligations bite on *distribution* or a *networked service*.

## 7. Working guardrails for agents

- Never break the default GUI build. Run the GUI regression gate after shared-crate
  changes.
- Prefer additive + feature-gated changes; prefer widening over forking bounds.
- New comments in English.
- Commit in small, verified increments; `-x` provenance on cherry-picks.
- Work on a branch, push to `origin`; don't assume distribution rights beyond that.
