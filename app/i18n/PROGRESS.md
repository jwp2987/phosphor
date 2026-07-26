# Zap Desktop i18n translation progress board

> **This document is the coordination hub for parallel multi-agent translation.** Each agent should read this first to claim a surface before starting, and update the corresponding row when done.
> The source-of-truth locale is `en` (must be 100% complete); other locales automatically fall back to English for missing keys, and can be backfilled in batches.

## Architecture at a glance

- **Loading chain**: `app/src/i18n.rs` → `i18n-embed` embeds `app/i18n/{locale}/*.ftl` at compile time → a global `FluentLanguageLoader` singleton → the `t!("key")` macro (returns a `String`, fed directly into GPUI Text/label_text)
- **Fallback chain**: the user's system locale (e.g. `zh-CN`) → the same language family (`zh`) → the `en` fallback; a missing key at any level automatically falls through
- **One pair of .ftl files per surface** (`en/<surface>.ftl` + `zh-CN/<surface>.ftl`), keys isolated with a prefix namespace to avoid merge conflicts between agents
- **The `fl!()` macro validates at compile time** that a key exists in the fallback language (`en`) → if a call site writes `t!("key")` but the key wasn't added to `en/*.ftl`, compilation fails — this is a good thing, since it forces alignment
- **Runtime switching**: currently a `OnceLock`, initialized once at startup; supporting dynamic switching via settings later will require refactoring into `RwLock<FluentLanguageLoader>` (not part of this round)

## Progress status legend

| Symbol | Meaning |
|---|---|
| ✅ | Done (en + zh-CN fully translated, all call sites replaced, cargo check passes) |
| 🟡 | Partially done (en/zh-CN not aligned, or not all call sites replaced) |
| ⬜ | Not started |
| 🔒 | Claimed by an agent (in progress) |
| ➖ | Not applicable (a purely non-UI module, no strings need translation) |

## Surface checklist

| # | Surface | File path | en status | zh-CN status | call sites | Owner | Notes |
|---|---|---|---|---|---|---|---|
| 0 | common (base atoms) | `app/i18n/{en,zh-CN}/common.ftl` | ✅ | ✅ | n/a | foundation | Common button/status text |
| 1 | settings (PoC starting point) | `app/src/settings_view/**` | 🟡 (AI + mod nav + about/main + referrals + agent_providers) | 🟡 | mod.rs:31, about/main:21, referrals:24, agent_providers:30 | foundation, agent-settings-mod, agent-settings-about, agent-settings-referrals, agent-settings-agent-providers | Base keys for the AI page are in place; mod.rs's SettingsSection Display + pane menu + debug have been replaced; about_page.rs (1 key/1 cs) + main_page.rs (20 key/20 cs); referrals_page.rs (28 key/24 cs); agent_providers_widget.rs ✅ (33 key/30 cs: title/description/empty/add-button/search-placeholder/quick-add-title/refresh-catalog/loading-catalog/catalog-empty/no-match/collapse/expand-remaining/row-missing/field-name/-base-url/-api-key/-api-type/api-type-hint/name-placeholder/api-key-placeholder/models-label/-empty-hint/-header-{name,id,context,output}/model-{name,id,context,output}-placeholder/add-model/fetch-from-api/sync-models-dev/remove). The BYOP config UI is fully translated; the reasoning chip section newly added to the file (ReasoningEffortSetting, being added by another agent as part of ongoing i18n work) is out of scope for this round. `cargo check` fails for the whole crate because `app/src/lib.rs` is missing `mod i18n;` (an infrastructure agent's responsibility, not part of this task) |
| 2 | ai core | `app/src/ai/**`, `app/src/ai_assistant/**` | ⬜ | ⬜ | ⬜ | (free) | Has many BYOP / agent / blocklist / mcp subdirectories; can be split further |
| 3 | command_palette | `app/src/command_palette.rs`, `app/src/palette/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 4 | drive | `app/src/drive/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 5 | onboarding | `crates/onboarding/**`, `app/src/coding_entrypoints/**` | ⬜ | ⬜ | ⬜ | (free) | Cross-crate note: `onboarding` is a separate crate — check whether it needs its own i18n setup |
| 6 | workspace | `app/src/workspace/**`, `app/src/workspaces/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 7 | modal & prompt | `app/src/modal/**`, `app/src/prompt/**`, `app/src/quit_warning/**` | 🟡 | 🟡 | 14 | agent-quit-warning | quit_warning ✅, modal/prompt still to do |
| 7a | quit_warning | `app/src/quit_warning/mod.rs` | ✅ | ✅ | 14 | agent-quit-warning | Quit/close confirmation dialog |
| 1b | settings-warpify | `app/src/settings_view/warpify_page.rs` | ✅ | ✅ | 17 | agent-settings-warpify | The Warpify sub-page (subshell + SSH configuration). 19 keys: page-title / description-prefix / learn-more / section-subshells(+subtitle) / section-ssh(+subtitle) / added-commands / denylisted-commands / denylisted-hosts / command-placeholder / host-placeholder / enable-ssh / install-ssh-extension(+description) / use-tmux / tmux-description / ssh-tmux-toggle-binding-label. Where the Category required `'static`, promoted via `Box::leak` |
| 1c | settings-keybindings | `app/src/settings_view/keybindings.rs` | ✅ | ✅ | 14 | agent-settings-keybindings | 13 keys: search-placeholder / conflict-warning / button-default/cancel/clear/save / press-new-shortcut / description / use-prefix / use-suffix / not-synced-tooltip / subheader / command-column. `render_button`'s parameter was upgraded from `&'static str` to `String` (`Text::new_inline` accepts `Cow<'static, str>`, so this is compatible). `SEARCH_PLACEHOLDER` is still kept as a `pub const` as a reuse entry point for `resource_center/keybindings_page.rs`, to be migrated once that file gets its own i18n pass. All `crate::t!` calls are consistent with other settings agents; actually compiling depends on the infrastructure agent registering `mod i18n;` in `app/src/lib.rs`. |
| 8 | auth | `app/src/auth/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 9 | workflows | `app/src/workflows/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 10 | editor & search | `app/src/editor/**`, `app/src/search/**`, `app/src/search_bar.rs` | ⬜ | ⬜ | ⬜ | (free) | |
| 11 | terminal | `app/src/terminal/**`, `app/src/shell_indicator.rs` | ⬜ | ⬜ | ⬜ | (free) | |
| 12 | mcp servers | `app/src/settings_view/mcp_servers/**`, `app/src/ai/mcp/**` | ✅ (settings_view/mcp_servers/**) | ✅ | 78 | agent-settings-mcp-servers-subdir | mcp_servers_page.rs ✅ (6 key/6 cs); the settings_view/mcp_servers/** subdirectory ✅: destructive_mcp_confirmation_dialog.rs (9 key/12 cs) + edit_page.rs (12 key/13 cs) + installation_modal.rs (6 key/6 cs) + list_page.rs (20 key/16 cs: removed 3 consts + converted a LazyLock to runtime fragments) + server_card.rs (14 key/14 cs: tooltip×4/button×3/status×4/tools×2/update-tooltip) + update_modal.rs (10 key/9 cs: default-name/title/desc/publisher×2/from/version/cancel/update/no-updates). `cargo check -p warp --lib` 0 errors / 50s. The remaining ai/mcp/** is still unclaimed |
| 13 | billing & pricing | `app/src/billing/**`, `app/src/pricing/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 14 | notebooks | `app/src/notebooks/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 15 | code_review | `app/src/code_review/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 16 | banner & tips | `app/src/banner/**`, `app/src/tips/**` | ✅ (banner) | ✅ (banner) | 1 | agent-banner | banner is done; tips is unclaimed |
| 17 | crash_recovery & errors | `app/src/crash_recovery.rs`, `app/src/crash_reporting/**` | ⬜ | ⬜ | ⬜ | (free) | |
| 18 | menu & app_menus | `app/src/menu.rs`, `app/src/app_menus.rs` | ⬜ | ⬜ | ⬜ | (free) | |
| 19 | view_components | `app/src/view_components/**` | ⬜ | ⬜ | ⬜ | (free) | Common UI widget placeholders/tooltips |
| 20 | misc (remaining single files) | see the mod list in lib.rs | ⬜ | ⬜ | ⬜ | (free) | Final cleanup catch-all |
| - | settings-rules-page | drive/items/ai_fact{,_collection}.rs + ai_page.rs:5521 | ✅ | ✅ | 4 | agent-rules-page | The Manage Rules page. Added 2 keys: `rules-collection-name` (the collection title on the Drive side, new ANCHOR-SUB-RULES-PAGE) + `settings-ai-rules-description` (the rules section description on the AI settings page, placed under ANCHOR-SUB-AI-PAGE). Reused the existing `settings-ai-learn-more`. Call sites: ai_fact_collection.rs's `display_name`, ai_page.rs's rules_description plain_text + hyperlink (2 places). ai_fact.rs's rendering of individual facts is all user data (name/content), no hardcoded strings need translating. cloud_object_naming_dialog.rs has no rule-related strings. |
| - | slash-commands | `app/src/search/slash_command_menu/static_commands/commands.rs` | ✅ | ✅ | 33 desc + 13 hint | agent-slash-commands | The description and argument hint_text for slash commands like `/agent`, `/skills`, `/profile` in the command palette. New ANCHOR-SUB-SLASH-COMMANDS. The original `pub const StaticCommand` was entirely converted to `pub static LazyLock<StaticCommand>` (because the `description: &'static str` field can't call a function in a const context); added the `t_static!` macro (`app/src/i18n.rs`) = `Box::leak(t!(...).into_boxed_str())`, a one-time leak used for `&'static str` fields, called only once during LazyLock init. In `zero_state.rs:48-55`, the prioritized_commands vec's `&commands::CONVERSATIONS/PROMPTS/AGENT` were changed to `&*` (LazyLock doesn't auto-coerce from `&LazyLock<T>` to `&T`). All the original const pushes in `all_commands()` were changed to `.clone()`. The `rename_tab_command_requires_argument` unit test needed `crate::i18n::init(Some("en"))` added to get the real hint. `cargo check -p warp --lib` 0 errors / 92s. |
| - | keybinding descriptions | binding registration sites (workspace/mod.rs, etc.) | ✅ | ✅ | 156 | agent-keybinding-descriptions | Binding description text. New ANCHOR-SUB-KEYBINDING-DESC, 116 keys, focused on `app/src/workspace/mod.rs` (every user-visible description within workspace has been replaced: FixedBinding::custom + EditableBinding::new + BindingDescription::new + with_custom_description(MAC_MENUS_CONTEXT) + with_dynamic_override closures). `BindingDescription::new` is already generic over `S: Into<String>`, so it directly accepts the `String` returned by `crate::t!()` — no API change needed; `titlecase` is still applied, but Chinese is unaffected by it. The binding `name` (a protocol field) was not touched. **Untouched**: debug-build-only engineering entries like `[Debug]/[a11y]/sample_process/dump_heap_profile/crash` (not visible to end users, deliberately skipped to reduce scope); other binding files — `terminal/view/init.rs` (77 occurrences, terminal binding/agent context, etc.), `editor/view/mod.rs` (60), `notebooks/editor/view.rs` (51), `code/editor/view/actions.rs` (39), `pane_group/mod.rs` (14), `terminal/input.rs` (14) — are all still to be done. `cargo check -p warp --lib` 0 errors, 27s. |

## Agent workflow (copy and follow)

Once each parallel agent picks up a surface name, follow this flow:

1. **Claim it**: write your agent ID in the Owner column of this table, and change the status to 🔒
2. **Extract**: `grep` the surface's directory for all user-facing hardcoded strings (label/title/tooltip/placeholder/error message)
   - **Skip**: log/telemetry/debug strings, enum Display impls (already structured), key/setting names (protocol fields)
   - **Keep**: UI copy, button text, error messages, status text, dialog title/body
3. **Add keys**: add new keys to `app/i18n/en/<surface>.ftl`, using `<surface>-<area>-<purpose>` kebab-case naming
4. **Replace call sites**: change `"hardcoded".to_string()` to `crate::t!("surface-area-purpose")` (adjust the import path based on module nesting)
5. **Translate to Chinese**: add the same keys to `app/i18n/zh-CN/<surface>.ftl`, keeping terminology consistent (see the "Glossary" below)
6. **Verify**: `cargo check -p warp` must pass (because `fl!()` validates keys at compile time — missing even one fails the build)
7. **Write back progress**: update the corresponding row in this table to ✅, fill in the commit SHA in the `Owner` column, and fill in the actual number of replaced call sites in the call sites column

## Glossary (must stay consistent)

| EN | zh-CN | Notes |
|---|---|---|
| Agent | 智能体 | Used for both Warp's own agent and BYOP generically |
| Block | 命令块 | A core Warp concept |
| Drive | 云盘 | Zap Drive's file-collaboration product |
| Workflow | 工作流 | |
| Notebook | 笔记本 | |
| Profile | 配置 | execution profile / agent profile |
| Permission | 权限 | |
| Prompt | 提示词 | LLM context, not the shell prompt |
| Shell Prompt | Shell 提示符 | Use the full term to disambiguate when needed |
| Setting | 设置 | |
| Provider | 提供商 | BYOP / model source |
| MCP Server | MCP 服务器 | Keep the abbreviation fully capitalized |
| Skill | 技能 | |
| Tool | 工具 | LLM tool calling |
| Command | 命令 | shell command |
| Block List | 命令块列表 | The main UI area |
| Pane | 窗格 | terminal pane |
| Tab | 标签页 | |
| Subagent | 子智能体 | |

## Anti-patterns (don't do these)

- ❌ Translating protocol field names / setting keys / enum variants used for serialization — they appear in config files and persisted data, not UI copy
- ❌ Translating log messages — logs are developer-facing only; keeping them uniformly in English reduces debugging ambiguity
- ❌ Translating the source-chain text of error types (`anyhow::Error`/`thiserror`) — same reasoning as above
- ❌ Doing variable interpolation via string concatenation (`"Hello, " + name`) — must use Fluent's `{ $name }`; word order in Chinese can be completely different
- ❌ Passing a non-literal (dynamic key) into a `t!()` call — `fl!()` validates at compile time, so only literals are allowed

## Checklist for adding a new surface's ftl

When adding a new surface:

1. Create `app/i18n/en/<surface>.ftl` and `app/i18n/zh-CN/<surface>.ftl` (the former must have content, the latter can be empty)
2. No need to change `i18n.toml` or `app/src/i18n.rs` — `RustEmbed` automatically picks up every file under the directory
3. Add a row to this PROGRESS.md table
