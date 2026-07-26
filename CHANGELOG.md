# Changelog

This document records key changes across Zap releases. Only feature-level commits are included; internal rolling tags such as dev / stable are omitted.

## [Unreleased]

- **AI / BYOP**: ported opencode's `applyCaching`, enabling prompt caching; `write_to_long_running_shell_command` now rejects embedded LF in line mode; BYOP LRC monitor fallback now goes through a silent subtask; fixed a sender leak inside the `cancel_execution` 50ms window (#134 follow-up, #137)
- **Cloud stripping Phase 1–2**: added the `cloud-disabled` channel predicate; cleaned up billing/pricing, referral/reward, and cloud sharing dialog UI; unsubscribed the RTC UpdateManager; retired the notebook/folder sync queue
- **Platform**: fixed a panic when launching macOS via Spotlight/Finder/Launchpad; `run_shell_command` stdout now falls back to the command grid
- **Infra**: `.gitattributes` now forces LF; added a stale bot and a Claude Code GitHub workflow
- **Editor**: added syntax highlighting for 15 more languages in the code/Markdown viewer (Dart, Zig, SCSS, R, Julia, OCaml, Erlang, Nix, Groovy, Solidity, GraphQL, Protobuf, Clojure, Elm, CMake)

## [v2026.05.06.preview] — 2026-05-06

- **AI**
  - Integrated the DeepSeek CLI agent, improving LSP install reliability
  - LSP switched to a global `enabled_lsp_servers` setting; removed the `/index` command and the codebase indexing runtime
  - `/plan` now genuinely replicates Plan Mode (system prompt + hard tool guardrails)
  - Agent dynamic tool whitelist, `persist_conversations` setting, `ask_user_question` always asks even under auto-approve
  - BYOP supports provider extra headers
- **Fixes**
  - `apply_file_diffs` schema changed from `const` to `enum` to accommodate Gemini
  - Root-caused the SSE stutter — genai gzip was off by default + workflow split
  - Plan folder notebooks are now created immediately in cloud-less environments
- **Branding**: logo and icons switched to a white background; BYOP mode hides the credits/billing UI

## [v2026.05.04.preview] — 2026-05-04

- **SSH Manager**: data layer + persistence + keychain landed; full UI/UX integration (panel + center pane + drag-and-drop + collapse + Connect + Command Palette)
- **AI**: distinguish the model's "no suggestion" output and refine the hint system; BYOP history multimodal support extended to PDF/audio, opencode-style ERROR replacement; UserQuery.context.images kept alive end-to-end
- **UI**: title-bar search box can now be hidden; fixed keybinding-settings edit-state and shortcut-badge contrast
- **i18n**: localized the remaining fixed text in the main UI; `/model` now defaults to `alt-shift-/`
- **Fixes**: Anthropic adapter now sends the 1M context beta header by default; BYOP ToolCall now emits a placeholder card on the first frame; OpenAI-strict providers no longer echo back `reasoning_content`
- **Infra**: CI fixed the `.deb` build and enabled PR testing

## [v2026.05.03.preview(.2/.3/.4)] — 2026-05-03

- **Upstream sync**: merged in a large batch of warp-upstream commits (cross-window tab drag, shell script recognition, IME cursor, remote server init refactor, SSH remote-server auto-upgrade, cross-window tab drag, etc.); set up rerere + the `zap-ours` merge driver; added blocklist docs
- **AI / BYOP**: added a coerce layer for type-mismatched tool argument output; tightened the suspicious-backslash scan to eliminate ls/diff false positives
- **i18n**: filled in remaining Chinese localization (settings panel, etc.)
- **Website**: unified the GitHub URL to `zerx-lab/warp`; fixed mobile horizontal overflow
- **Fixes**: aligned the Windows taskbar ICO with upstream's format; NLD in terminal now defaults to true, restoring automatic Chinese-input-to-AI routing

## [v2026.05.02.preview] — 2026-05-02

- **AI / BYOP**
  - Closed the loop on conversation compaction — the `byop_compaction` module, settings persistence, auto prune, overflow passthrough, 1:1 replication of opencode
  - Moved reasoning effort from provider settings to the input-box picker
  - Wired multimodal attachment support into the BYOP path
  - Integrated local BYOP webfetch / websearch with Exa
  - Select system prompt templates by model identifier; added several new templates
- **Privacy / cloud stripping**
  - Physically removed easily-strippable P4 dead code (anonymous_id / EXPERIMENT_ID_HEADER / settings sync / app_focus)
  - Cut off four outbound channels: closed-source telemetry, Sentry, anonymous_id, and settings sync
  - Flipped three privacy toggle defaults from true → false
  - Two cleanup passes on `cloud_conversations` (UI / privacy / FeatureFlag / AIClient / cargo feature)
- **Refactor**: removed blocklist AI response scoring and telemetry; removed `agent_attribution` and the Oz changelog toggle
- **CI**: switched the weekly build to an official release with standardized tags

## [v2026.05.01.preview] — 2026-05-01

- **Cloud stripping**: physically removed 6 cloud LLM tools + child_agent + orchestration; physically removed the share-modal trio and the billing-denied modal; website logo switched to monochrome
- **AI**
  - Wired Workflow Autofill into BYOP one-shot
  - BYOP LRC now keeps injecting context on subsequent turns + hardened sanitize + control-key tokens
  - Chat stream now surfaces remote-login session hints and reasoning passthrough
  - Refined genai error mapping into Stream / Other variants
  - Chat stream adapter, fixed ToolCall None handling
- **Platform**: `warpui_core` avoids rescanning system fonts; sync commands unconditionally disable the pager, using `PAGER=cat` instead to preserve the real exit code
- **Website**: site-wide component and i18n refactor, Tailwind and global styles kept in sync

## [v2026.04.30.oss] — 2026-04-30

- **CI**: CHANNEL `preview` → `oss`; fixed Windows / macOS build failures
- **Refactor**: removed leftover cloud_mode code and settings

## [v2026.04.30.preview] — 2026-04-30

First preview release of the Zap community fork.

- **Branding & positioning**: renamed to Zap + logo redesign + community fork README
- **BYOP**
  - `async-openai` → `genai`, supporting explicit binding of 5 native protocols
  - Providers subpage + models.dev data source + quick-add search box
  - Trimmed down the prompt template
- **Decentralization cleanup**: removed the `UseComputer` / `RequestComputerUse` tools, Drive's `Create team` / `Join team` entry points, and referral-related code
- **i18n**: Fluent infrastructure + translated 12 settings_view files; completed i18n for the ai / features / teams pages
- **Website**: added a bilingual (Astro + Tailwind, EN/CN) BYOP landing page; responsive improvements
- **AI**: CJK input classification, reasoning split, BYOP tool_call diagnostics, LRC tag-in synthetic virtual subagent + floating-window spawn chain
- **CI**: Release now explicitly declares `contents: write` permission, fixing 403s

[Unreleased]: https://github.com/zerx-lab/warp/compare/v2026.05.06.preview...HEAD
[v2026.05.06.preview]: https://github.com/zerx-lab/warp/compare/v2026.05.04.preview...v2026.05.06.preview
[v2026.05.04.preview]: https://github.com/zerx-lab/warp/compare/v2026.05.03.preview.4...v2026.05.04.preview
[v2026.05.03.preview(.2/.3/.4)]: https://github.com/zerx-lab/warp/compare/v2026.05.02.preview...v2026.05.03.preview.4
[v2026.05.02.preview]: https://github.com/zerx-lab/warp/compare/v2026.05.01.preview...v2026.05.02.preview
[v2026.05.01.preview]: https://github.com/zerx-lab/warp/compare/v2026.04.30.oss...v2026.05.01.preview
[v2026.04.30.oss]: https://github.com/zerx-lab/warp/compare/v2026.04.30.preview...v2026.04.30.oss
[v2026.04.30.preview]: https://github.com/zerx-lab/warp/releases/tag/v2026.04.30.preview
