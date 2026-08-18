use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};

use enum_iterator::{cardinality, Sequence};

#[cfg(feature = "test-util")]
pub use overrides::{get_overrides, set_overrides};

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, Sequence)]
pub enum FeatureFlag {
    Changelog,
    CrashReporting,
    DebugMode,
    Autoupdate,
    WithSandboxTelemetry,
    RecordAppActiveEvents,

    WelcomeTips,
    ThinStrokes,
    WelcomeBlock,
    KnowledgeSidebar,

    RuntimeFeatureFlags,

    /// Does grid storage go forwards or backwards
    SequentialStorage,

    /// If set, generators are executed in-band in all SSH sessions.
    InBandGeneratorsForSSH,

    /// If set, generators are executed using cmd.exe on Windows.
    RunGeneratorsWithCmdExe,

    /// Gates a bindable keyboard action for accepting command corrections.
    CommandCorrectionKey,

    /// If `true`, the "Show Initialization Block" menu item is added to the Blocks menu in the Mac
    /// menu bar.
    ToggleBootstrapBlock,

    /// Enabling context chips functionality for prompt
    ContextChips,

    /// Ligature Support in the Editor and Grid
    Ligatures,

    /// When enabled, the `History` rule from the command_corrections crate
    /// will be enabled. When the `History` rule is enabled, the command_corrections
    /// lib will use the user's history as a last-ditch effort to find a reasonable correction.
    CommandCorrectionsHistoryRule,

    /// Used to gate an experiment we're doing on WarpDev ONLY
    /// to get a sense of PTY throughput over time.
    RecordPtyThroughput,

    /// Whether to fetch generic string objects from the server.
    FetchGenericStringObjects,

    /// Enables a setting on Intel Dual-GPU Macs to enable use of the integrated GPU over the
    /// discrete GPU.
    IntegratedGPU,

    /// Defaults Windows builds to the high-performance GPU with Vulkan as the preferred backend.
    WindowsHighPerformanceGpuDefault,

    /// Zap Agent Mode.
    AgentMode,

    /// Whether the user is part of the Zap Alpha Program (AI Trusted Testers).
    /// This is enabled automatically for local and dev builds.
    /// Collect conversation and input autodetection data for agent mode.
    /// Also collects block data for Next Command, if enabled.
    AgentModeAnalytics,

    /// A setting to enable a traditional completions experience.
    ClassicCompletions,

    /// Force enable classic completions.
    ForceClassicCompletions,

    /// If enabled, autosuggestions are hidden when the tab completions
    /// menu is open (except when using completions-as-you-type).
    RemoveAutosuggestionDuringTabCompletions,

    /// Feature flag for cursor reflow fix (fixes part of the Alacritty resizing logic).
    ResizeFix,

    /// Enable multiselect in Notebooks and Zap Text.
    RichTextMultiselect,

    /// If enabled, the default input mode is set to waterfall for new users.
    DefaultWaterfallMode,

    /// Makes the input editor's prompt selectable.
    SelectablePrompt,

    /// Enables the settings file feature.
    SettingsFile,

    /// Enables the settings import onboarding block and pre-parsing
    /// configs on app startup.
    SettingsImport,

    /// Enables rect selection.
    RectSelection,

    /// Adds Alacritty as a supported terminal to import settings from.
    AlacrittySettingsImport,

    /// Enable dynamic enum parameter types for workflow arguments
    DynamicWorkflowEnums,

    /// Enables next action prediction within Zap, powered by AI.
    AgentPredict,

    /// Enables receiving shared Zap Drive objects.
    SharedWithMe,

    /// Enables workflows for use with Agent Mode.
    AgentModeWorkflows,

    /// Enables AI rules for use with Agent Mode.
    AIRules,

    /// Routes SSH sessions through the tmux-backed SSH wrapper.
    SSHTmuxWrapper,

    /// Reduces the amount of horizontal padding in the blocklist
    /// from 20px to 16px.
    LessHorizontalTerminalPadding,

    /// Enables the shell selector, allowing us to open a new tab in
    /// a shell other than the default shell.
    ShellSelector,

    /// Replaces the bookmark button with a "save as workflow" button.
    BlockToolbeltSaveAsWorkflow,

    /// Lazily builds scenes at render time instead of eagerly when a view
    /// changes.
    LazySceneBuilding,

    /// Removes the extraneous padding from the alt-screen that we previously had
    /// to keep consistent size between blocklist and alt-screen.
    ///
    /// See plan here: https://docs.google.com/document/d/1TBPSWNfh4KylkEgL5o5xyYgK_KQzUQk1oxjuIx2ipXw
    RemoveAltScreenPadding,

    /// Enables the full-screen "zen mode" setting, where we hide the tab bar if there's only one
    /// tab.
    FullScreenZenMode,

    /// Playground for reducing Zap UI clutter.
    MinimalistUI,

    /// Enables support for using native shell completions to supplement our
    /// completion specs.
    NativeShellCompletions,

    /// Adds avatar to the tab bar.
    AvatarInTabBar,

    /// Adds aliases for executing Zap Drive workflows.
    WorkflowAliases,

    SshDragAndDrop,
    DragTabsToWindows,

    /// Enables the overflow menu on AI blocks.
    AIBlockOverflowMenu,

    /// Enables cycling through the next command suggestions with down arrow.
    CycleNextCommandSuggestion,

    /// Enables multi-workspace selection.
    MultiWorkspace,

    /// Maximizes data in flat storage to reduce memory usage.
    MaximizeFlatStorage,

    /// Recognizes the OSC 8 hyperlink escape sequence and makes the
    /// linked text Cmd+click-able.
    OscHyperlinks,

    ImeMarkedText,

    /// Enables partial next command suggestions with a prefix.
    PartialNextCommandSuggestions,

    AIGeneratedOnboardingSuggestions,

    /// Enables iTerm image rendering
    ITermImages,

    /// Enables validation of autosuggestions.
    ValidateAutosuggestions,

    /// Enables prompt suggestions sourced via MAA.
    PromptSuggestionsViaMAA,

    /// Enables using `esc` to clear autosuggestions.
    ClearAutosuggestionOnEscape,

    /// If enabled, the default theme is set to Adeberry for new users.
    DefaultAdeberryTheme,

    /// New, less intrusive autoupdate UI.
    AutoupdateUIRevamp,

    /// Enables Kitty image rendering
    KittyImages,

    /// Enables support for Zap Packs.
    WarpPacks,

    /// Enables auto-generated AI memories.
    AIMemories,

    /// Enables the XML output system prompt for the primary (terminal) agent in Agent Mode.
    AgentModePrimaryXML,

    /// Enables the XML output system prompt for the pre-plan agent in Agent Mode.
    AgentModePrePlanXML,

    /// Enables Agent Mode onboarding.
    AgentOnboarding,

    /// Enables suggested rules.
    SuggestedRules,

    /// Enables suggested workflows for Agent Mode.
    SuggestedAgentModeWorkflows,

    /// Forces users to login.
    ForceLogin,

    /// Enables prediction of Agent Mode queries.
    PredictAMQueries,

    /// Enables full source code embedding of repos when using codebase context.
    FullSourceCodeEmbedding,

    /// Enables codebase indexing inside remote server daemon processes.
    RemoteCodebaseIndexing,

    /// Persists the codebase index merkle-tree snapshot across restarts, so a
    /// re-index after a restart is a diff rather than a full rebuild.
    CodebaseIndexPersistence,

    /// If enabled, command palette searches will use Tantivy search instead of the default fuzzy search.
    UseTantivySearch,

    /// Allows AI to call the grep tool.
    GrepTool,

    /// MCP server v0 functionality.
    McpServer,

    /// Enables image as context for AM.
    ImageAsContext,

    /// UNIX shells running "natively" on Windows via MSYS2.
    MSYS2Shells,

    /// Allows AI to call the file retrieval tools.
    FileRetrievalTools,

    /// Reload files in an AI conversation to prevent stale files.
    ReloadStaleConversationFiles,

    /// Retry truncated file edit responses from the coding agent.
    RetryTruncatedCodeResponses,

    /// Enables reading images with the `read_files` tool.
    ReadImageFiles,

    /// Enables the AI context menu, or at-menu.
    AIContextMenuEnabled,

    /// Enables the AI context menu outside of AI input mode.
    AtMenuOutsideOfAIMode,

    /// Enables the resume button for cancelled AI conversations.
    AIResumeButton,

    /// Enables the agent to decide whether to execute a command.
    AgentDecidesCommandExecution,

    /// Enables inline review comments on specific lines of code.
    ContextLineReviewComments,

    /// Enables the natural language classification model.
    NLDClassifierModelEnabled,

    /// Enables the fast-forward autoexecute button
    FastForwardAutoexecuteButton,

    /// Remembers the per-conversation fast-forward state across local session restoration.
    RememberFastForwardState,

    /// Enables the find/replace in code editor
    CodeFindReplace,

    /// Enables file search functionality in command palette
    CommandPaletteFileSearch,

    /// Enables the AI context menu nesting and commands
    AIContextMenuCommands,

    /// Enables sending stderr warnings in FileGlobV2 results.
    FileGlobV2Warnings,

    /// Enables code symbols in AI context menu
    AIContextMenuCode,

    /// Enables Zap Drive objects (like workflows) as context in AI context menu
    DriveObjectsAsContext,

    /// Expands code diff edits to replace the current pane instead of opening in a new tab.
    ExpandEditToPane,
    /// Enables fallback model load output messaging in the warping indicator.
    FallbackModelLoadOutputMessaging,

    /// Enables close button on left side of tabs
    TabCloseButtonOnLeft,

    /// Enables AI agent profile settings UI and functionality.
    ///
    /// TODO: When cleaning up this flag, also remove the `show_model_selectors_in_prompt`
    /// setting in [`SessionSettings`] (defined in `app/src/terminal/session_settings.rs`),
    /// as model selectors are always shown when this flag is enabled.
    ProfilesDesignRevamp,

    /// Enables return changed lines on apply diff result
    ChangedLinesOnlyApplyDiffResult,

    /// Enables us to render linked code blocks
    LinkedCodeBlocks,

    /// Enables the tabbed file viewer
    TabbedEditorView,

    /// Enables multiple agent profiles in settings for managing different AI agent configurations.
    MultiProfile,

    /// Enables the /pr-comments slash command.
    PRCommentsSlashCommand,

    /// Enables displaying imported PR review comments in the blocklist.
    PRCommentsV2,

    /// Gates the bundled skill-based implementation of PR comment fetching.
    PRCommentsSkill,

    /// An entrypoint pane type to launch other pane types from a search palette. The default view
    /// when creating a tab.
    WelcomeTab,

    /// A new first-time user experience which prioritizes choosing a coding repository.
    GetStartedTab,

    /// Enables Projects and Project management
    Projects,

    /// Enables selection-as-context functionality in the code editor.
    SelectionAsContext,

    /// A context chip that shows when the PWD is inside of a git repository.
    CodeModeChip,

    /// Enables the prompt chip that displays the GitHub PR for the current branch.
    GithubPrPromptChip,

    /// A button on the homepage for easily creating new projects.
    CreateProjectFlow,

    /// Enables vim keybindings in the code editor.
    VimCodeEditor,

    /// Allows opening file links using the $EDITOR environment variable.
    AllowOpeningFileLinksUsingEditorEnv,

    /// Enables improvements to our natural language detection functionality.
    NldImprovements,

    /// Enables the ability to undo closed panes.
    UndoClosedPanes,

    /// Enables revert button for diff hunks in the gutter.
    RevertDiffHunk,

    /// Enables saving code review pane changes
    CodeReviewSaveChanges,

    /// Enables the file tree (with an entrypoint through code mode).
    FileTree,

    /// Enables ignoring input suggestions.
    AllowIgnoringInputSuggestions,

    /// Enables the one-time modal on app startup for existing users for the Code launch.
    CodeLaunchModal,

    /// Enables API key authentication for Agent SDK
    APIKeyAuthentication,

    /// Enables API key management UI in settings
    APIKeyManagement,

    /// Enables OAuth support for MCP.
    McpOauth,

    /// Enables attaching diff sets (multiple hunks from multiple files) as context in Agent Mode.
    DiffSetAsContext,

    /// Enables file- and diff set-level comments in the code review header.
    FileAndDiffSetComments,

    /// Enables discarding per-file and discarding all changes
    DiscardPerFileAndAllChanges,

    /// Enables staging and un-staging from the code review pane: a per-file
    /// toggle in each file header, and a per-hunk toggle in the diff gutter.
    StageChanges,

    /// Enables UI zoom support (scaling the entire UI by a given percentage).
    UIZoom,

    /// Shows a confirmation dialog when cancelling an active summarization via Ctrl-C or stop.
    SummarizationCancellationConfirmation,

    /// Enables find/search in code review pane
    CodeReviewFind,

    /// Enables asynchronous find in terminal, running search on a background thread.
    AsyncFind,

    /// Enables auto-opening code review pane on first agent change and its setting UI.
    AutoOpenCodeReviewPane,

    /// Enables inline code review functionality
    InlineCodeReview,

    /// Enables the local docker sandbox entrypoints in the client.
    LocalDockerSandbox,

    /// Enables the /compact slash command.
    SummarizationConversationCommand,

    /// Enables the provider command for linking third-party services.
    ProviderCommand,

    /// Groups MCP tools and resources by their originating server when sending context to the AI backend.
    MCPGroupedServerContext,

    /// Enables the web search UI (when the model executes a web search).
    WebSearchUI,

    /// Enables the web fetch UI (when the model fetches content from URLs).
    WebFetchUI,

    /// Displays debugging IDs for MCP servers, installations, and gallery items.
    McpDebuggingIds,

    /// Enables rendering of images in markdown files and AI responses.
    MarkdownImages,
    /// Enables rendering Mermaid diagrams in markdown notebooks.
    MarkdownMermaid,
    /// Enables editable Mermaid diagrams to behave atomically in notebook and plan editors.
    EditableMarkdownMermaid,

    /// Enables rendering markdown tables in notebooks.
    MarkdownTables,

    /// Renders `.ipynb` (Jupyter) files as a formatted, read-only notebook in
    /// Warp's notebook viewer instead of showing the raw JSON in the code editor.
    JupyterNotebookRendering,

    /// Enables rendering markdown tables inline in AI block list responses.
    BlocklistMarkdownTableRendering,
    /// Enables rendering markdown images inline in AI block list responses.
    BlocklistMarkdownImages,

    /// Enables the /fork-from slash command.
    ForkFromCommand,

    /// Enables v2 of the context window usage UI.
    ContextWindowUsageV2,

    /// Enables global search
    GlobalSearch,

    /// Enables embedded code review comments.
    EmbeddedCodeReviewComments,

    /// Enables the revert to checkpoints feature.
    RevertToCheckpoints,

    /// Enables the /rewind slash command.
    RewindSlashCommand,

    AgentView,

    /// Enables block context functionality in Agent View.
    AgentViewBlockContext,

    /// Enables the inline history menu for quickly accessing previous commands and conversations.
    InlineHistoryMenu,

    /// Enables the inline repo switcher menu for switching between indexed repos.
    InlineRepoMenu,

    /// Enables support for AM file diffs backed by the V4A patch format.
    V4AFileDiffs,

    /// Enables loading conversations in the Agent Management View.
    InteractiveConversationManagementView,

    /// Enables agent tips displayed below the warping indicator in Agent Mode.
    AgentTips,

    /// Allows agent mode to use computer use tools.
    AgentModeComputerUse,

    /// Enables computer use functionality in local clients.
    LocalComputerUse,

    /// Enables background, per-window computer use: driving a specific window directly without
    /// raising it or moving the cursor.  Currently only supported on macOS.
    BackgroundComputerUse,

    /// Enables the "New agent" prompt chip in terminal mode when AgentView is enabled.
    ///
    /// When disabled (the default), the terminal message bar is shown instead.
    AgentViewPromptChip,

    /// Enables editing the agent input footer layout from the prompt context menu.
    AgentToolbarEditor,

    /// Enables configuring header toolbar item order, side placement, and visibility.
    ConfigurableToolbar,

    // Enables a side panel conversation list view for AgentView mode.
    AgentViewConversationListView,

    /// When enabled, the server will use message replacement + retroactive subtasks for
    /// summarization.
    SummarizationViaMessageReplacement,

    /// Enables pluggable notifications via OSC 9 and OSC 777 escape sequences.
    /// External programs can trigger system and in-app notifications.
    PluggableNotifications,

    /// Dev-only: simulate a GitHub-unauthed user in the Environments page flow.
    ///
    /// This is intended for developer testing and should have no effect in release builds.
    SimulateGithubUnauthed,

    /// When enabled, profile selection is displayed in an inline view above the Agent input (e.g. via /profile).
    InlineProfileSelector,

    /// Enables sending the server a list of Skills that the client has access to.
    ///
    /// If disabled, the server will send None as the SkillsContext.
    ListSkills,

    /// When enabled, we expose LSP as a tool to the agent.
    LSPAsATool,

    /// Enables conversation artifacts.
    ConversationArtifacts,

    /// Enables auto-syncing ambient plans to Zap Drive.
    SyncAmbientPlans,

    /// Enables platform skills support (--skill flag) for agent runs.
    ///
    /// Skills are loaded from `.agents/skills/`, `.warp/skills/`, `.claude/skills/`, and `.codex/skills/`
    /// directories to provide base prompts for agent runs.
    OzPlatformSkills,
    /// Enables loading and returning bundled skills in the SkillManager.
    BundledSkills,

    /// Enables the Zap launch modal announcing Zap going open-source.
    /// When enabled, the HOA onboarding flow is suppressed.
    ZapLaunchModal,

    /// Updated tab styling (background colors, border, close button positioning, margins).
    NewTabStyling,

    /// Enables file-based MCP server support via .mcp.json files in repo roots.
    FileBasedMcp,

    /// Enables passing user query arguments to skill invocations ($ARGUMENTS, $N).
    SkillArguments,

    /// When enabled, a conversation is only considered "active" once a new query has been
    /// sent since opening (rather than the moment its agent view is expanded).
    ActiveConversationRequiresInteraction,

    /// Enables attaching conversations as context in Agent Mode via the @ menu.
    ConversationsAsContext,

    /// Enables the rich input editor for CLI agents (e.g., Claude Code).
    /// Ctrl-G intercepts the keystroke and opens Zap's input editor instead of $EDITOR.
    CLIAgentRichInput,

    /// Enables incremental (diff-based) buffer updates for auto-reload instead of full replace.
    IncrementalAutoReload,

    /// Enables scroll position preservation in the code review pane when file
    /// content changes via auto-reload.
    CodeReviewScrollPreservation,

    /// Re-enables local Claude Code and Codex child harnesses in orchestration
    /// flows while the default behavior temporarily keeps them disabled.
    LocalClaudeCodexChildHarnesses,

    /// Gates the client-side multi-level orchestration surfaces. Upstream this
    /// covers three things; only the third exists here, because the first two
    /// are declined:
    ///
    /// - child conversations auto-executing their own `run_agents` calls --
    ///   agent-invoked agent spawning is declined (`DECLINED.md` #325);
    /// - the `run_agents` confirmation card's "may start child agents"
    ///   disclosure -- that card is declined (`DECLINED.md` #290);
    /// - **the TUI's `Agents:` bar becoming a drill-down bar** (breadcrumbs,
    ///   subtree rollup badges, per-level paging) plus parent-prefixed
    ///   received-message headers. That half is local and in scope, and is
    ///   what this flag actually gates in this fork.
    ///
    /// With the flag off the bar keeps the flat root-anchored rendering, which
    /// a byte-for-byte golden-row test pins. Placed in `DOGFOOD_FLAGS` only,
    /// matching upstream's own default-off state at the pin.
    MultiLevelOrchestration,

    /// Shows a pending user query indicator during summarization when a follow-up
    /// prompt is queued via `/fork-and-compact` or `/compact-and`.
    PendingUserQueryIndicator,

    /// Gates the `/queue` slash command, which lets users queue a follow-up prompt
    /// while the agent is mid-response.
    QueueSlashCommand,

    /// Extends [`FeatureFlag::QueueSlashCommand`] to the full multi-prompt queued-prompts
    /// panel: queueing shell commands (not just agent prompts) and routing a prompt
    /// submitted during summarization into the panel. Off without this flag, only the
    /// single-prompt auto-queue behavior applies.
    QueuedPromptsV2,

    /// Enables an agent tool for the CLI subagent to explicitly transfer command control to the
    /// user.
    TransferControlTool,

    /// Enables Kitty keyboard protocol support (CSI u encoding, progressive enhancement).
    KittyKeyboardProtocol,

    /// Detects the word "figma" in the terminal input in real-time and shows a
    /// contextual button above the input.
    FigmaDetection,

    /// Enables header rows on all inline menus (label, tabs, resize handle).
    InlineMenuHeaders,
    /// Clears the current prompt when opening the inline model selector from the
    /// model chip, then restores that prompt when the selector closes.
    RestorePromptOnInlineModelSelectorSearch,

    /// Enables associating a tab color with a directory so tabs automatically
    /// adopt the configured color when their working directory matches.
    DirectoryTabColors,

    /// Enables the new settings to control visibility of Zap Drive, Code Review Panel,
    /// and Project Explorer & Global Search features.
    ZapNewSettingsModes,

    /// Enables vertical tab layout as an alternative to the horizontal tab bar.
    VerticalTabs,

    /// Enables attaching code review comments, diff hunk, and attach as context
    /// from code review + code editor for House Of Agents work
    HoaCodeReview,

    /// Enables the `--harness` flag for `oz agent run`, allowing external agent
    /// CLIs (e.g. `claude`) to execute prompts instead of Zap's agent harness.
    AgentHarness,

    /// Enables the upgraded CLI agent session tracking and notifications infrastructure.
    HOANotifications,

    /// Enables the install/update chip for the OpenCode Zap plugin.
    /// Requires HOANotifications to also be enabled.
    OpenCodeNotifications,

    /// Enables the install/update chip for the Codex Zap notification plugin.
    /// Requires HOANotifications to also be enabled.
    CodexNotifications,

    /// Enables the Codex Phosphor plugin marketplace integration.
    /// When disabled, Codex uses native OSC9 notifications.
    CodexPlugin,

    /// Enables the install/update chip for the Gemini CLI Zap extension.
    /// Requires HOANotifications to also be enabled.
    GeminiNotifications,

    /// Enables tab configs — user-definable TOML templates for launching custom tab layouts.
    TabConfigs,

    /// Enables the ask_user_question tool allowing the agent to ask clarifying questions.
    AskUserQuestion,

    /// When enabled, solo users (not on a team) can use BYO API keys.
    SoloUserByok,

    /// Replaces the in-block warpification banner with a warpify footer.
    WarpifyFooter,

    /// Guided onboarding flow for existing users introducing HOA features
    /// (vertical tabs, agent inbox, tab configs).
    HOAOnboardingFlow,

    /// Enables commit, push, and create-PR actions in the code review panel.
    GitOperationsInCodeReview,

    /// Gates the remote control chip and `/remote-control` slash command in the CLI agent footer.
    HOARemoteControl,

    /// Trims trailing blank rows from CLI agent block output so unused vertical
    /// space is not rendered while the agent is running.
    TrimTrailingBlankLines,

    /// Gates the new SSH remote server flow that installs and connects to a
    /// persistent binary on the remote machine instead of using ControlMaster
    /// for command execution.
    SshRemoteServer,

    /// Enables summary mode in vertical tabs, showing condensed tab summaries
    /// instead of individual pane rows.
    VerticalTabsSummaryMode,

    /// Gates the user-configurable context window slider in AI settings and
    /// the execution profile editor. When disabled, the slider is hidden and
    /// `base_model_context_window_limit` is not sent on outbound requests, so
    /// the server falls back to its default.
    ConfigurableContextWindow,

    /// Enables the global HTTP proxy settings page and the proxy override logic
    /// in `Client::new()`. When disabled, the UI entry is hidden and
    /// `Client::new()` falls back to reqwest's default (reads environment
    /// variables). See Issue #72.
    HttpProxySettings,

    /// Enables state-mutating recovery for abnormal terminal lifecycle sequences.
    /// Gates the block-lifecycle transition coordinator's corrective completion
    /// recovery paths (see `app/src/terminal/model/lifecycle`).
    TerminalLifecycleRecovery,

    /// Gates the Grouped Tabs feature.
    GroupedTabs,

    /// Gates the Pinned Tabs feature, which lets users pin individual tabs
    /// and whole tab groups so they stay at the front of the tab list and
    /// are protected from reordering.
    PinnedTabs,

    /// Enables local control (the standalone `warpctrl` CLI plus the
    /// in-process localhost control surface it talks to). See
    /// `crates/local_control` and `app/src/local_control`.
    WarpControlCli,

    /// Gates NLD input classification matching the buffer against agent
    /// prompt history (in addition to shell command history). Ported from
    /// the pin (`02b53fcd8`) for #312; matches the pin's default-off state --
    /// still in development there and disabled on all channels pending
    /// misclassification-bug fixes (pin PR #12586) -- so this is not added to
    /// `DOGFOOD_FLAGS`/`PREVIEW_FLAGS`/`RELEASE_FLAGS` here either.
    NldPromptHistoryMatch,
}

static FLAG_STATES: [AtomicBool; cardinality::<FeatureFlag>()] =
    [const { AtomicBool::new(false) }; { cardinality::<FeatureFlag>() }];

/// This map is populated by UserPreferences, which take precedence
/// over the global feature flag state.
static USER_PREFERENCE_MAP: [AtomicTriState; cardinality::<FeatureFlag>()] =
    [const { AtomicTriState::new() }; { cardinality::<FeatureFlag>() }];

/// Flag for whether or not feature flags have been globally initialized. Outside
/// of tests, this ensures that feature flags are only used after they're set
/// up by the app's `run_internal` function.
#[cfg(debug_assertions)]
static FEATURES_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Features used in debugging.
pub const DEBUG_FLAGS: &[FeatureFlag] = &[FeatureFlag::DebugMode, FeatureFlag::RuntimeFeatureFlags];
/// Features enabled only for the WarpLocal developer build.
pub const LOCAL_FLAGS: &[FeatureFlag] = &[FeatureFlag::LocalClaudeCodexChildHarnesses];

/// Features enabled for the development team.  The expectation is that, over
/// time, these will move on to PREVIEW_FLAGS before being launched.
pub const DOGFOOD_FLAGS: &[FeatureFlag] = &[
    FeatureFlag::ToggleBootstrapBlock,
    FeatureFlag::RemoveAutosuggestionDuringTabCompletions,
    FeatureFlag::ResizeFix,
    FeatureFlag::AgentModeWorkflows,
    #[cfg(not(windows))]
    FeatureFlag::SSHTmuxWrapper,
    FeatureFlag::AgentModeAnalytics,
    FeatureFlag::LazySceneBuilding,
    FeatureFlag::SshDragAndDrop,
    FeatureFlag::MultiWorkspace,
    FeatureFlag::ImeMarkedText,
    FeatureFlag::MSYS2Shells,
    FeatureFlag::RetryTruncatedCodeResponses,
    FeatureFlag::ContextLineReviewComments,
    FeatureFlag::RunGeneratorsWithCmdExe,
    FeatureFlag::NLDClassifierModelEnabled,
    FeatureFlag::Projects,
    FeatureFlag::ProviderCommand,
    FeatureFlag::MarkdownImages,
    FeatureFlag::FileAndDiffSetComments,
    FeatureFlag::FileGlobV2Warnings,
    FeatureFlag::SummarizationViaMessageReplacement,
    FeatureFlag::LocalComputerUse,
    FeatureFlag::OzPlatformSkills,
    FeatureFlag::AgentViewBlockContext,
    FeatureFlag::MultiLevelOrchestration,
    FeatureFlag::PendingUserQueryIndicator,
    FeatureFlag::QueueSlashCommand,
    FeatureFlag::QueuedPromptsV2,
    // Codebase indexing. At the pin these two were enabled by a 100% server-side
    // experiment and listed here only so dogfood builds matched; this fork has no
    // server, so this list is the only thing that turns them on.
    FeatureFlag::FullSourceCodeEmbedding,
    FeatureFlag::CodebaseIndexPersistence,
    // End manually enabled Code features.
    FeatureFlag::DirectoryTabColors,
    FeatureFlag::EditableMarkdownMermaid,
    FeatureFlag::CodeReviewScrollPreservation,
    FeatureFlag::AgentHarness,
    FeatureFlag::RememberFastForwardState,
    FeatureFlag::HOANotifications,
    FeatureFlag::GeminiNotifications,
    FeatureFlag::LocalDockerSandbox,
    FeatureFlag::VerticalTabsSummaryMode,
    FeatureFlag::ConfigurableContextWindow,
    FeatureFlag::DragTabsToWindows,
    FeatureFlag::TerminalLifecycleRecovery,
    FeatureFlag::RestorePromptOnInlineModelSelectorSearch,
    FeatureFlag::WarpControlCli,
    FeatureFlag::JupyterNotebookRendering,
];

/// Features enabled for feature preview build users (e.g.: Friends of Zap).
/// All PREVIEW_FLAGS are also automatically added to dogfood builds (WarpDev).
pub const PREVIEW_FLAGS: &[FeatureFlag] = &[
    FeatureFlag::MarkdownTables,
    FeatureFlag::GitOperationsInCodeReview,
];

/// Features enabled for all release builds (i.e.: everything but WarpLocal).
/// NOTE: if you are promoting a feature from Preview to launch, you'll likely
/// want to enable the feature by default in app/Cargo.toml, rather than add it to RELEASE_FLAGS.
pub const RELEASE_FLAGS: &[FeatureFlag] = &[
    // `FeatureFlag::Autoupdate` is deliberately NOT here, unlike upstream.
    //
    // This fork does not ship autoupdate: `autoupdate` is not in `app/Cargo.toml`'s
    // `default` feature list, and the release workflow publishes no update feed. Upstream's
    // list assumes a product that updates itself; carrying that assumption meant the flag
    // was set for every release build even though the Cargo feature behind it was off, so
    // the runtime guards (`lib.rs`, `workspace/`, `resource_center/`) would branch as if
    // autoupdate existed.
    //
    // It stays reachable for anyone who wants it: `app/src/lib.rs`'s `extra_flags` still
    // adds it under `#[cfg(feature = "autoupdate")]`, so an explicit
    // `--features autoupdate` build behaves exactly as before.
    //
    // Removing it is also what makes it safe to apply this whole list to release-*profile*
    // builds and not just release *bundles* — see `enabled_features()` in `app/src/lib.rs`.
    FeatureFlag::Changelog,
    FeatureFlag::CrashReporting,
    // winit's IME path supports marked text on both macOS and Windows.
    // Windows must have this flag enabled to render IME preedit / input
    // composition text, otherwise only the OS candidate window is visible and
    // Zap discards the marked text updates entirely.
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    FeatureFlag::ImeMarkedText,
    FeatureFlag::BlocklistMarkdownTableRendering,
    // Remote server binary is not yet supported on Windows.
    #[cfg(not(windows))]
    FeatureFlag::SshRemoteServer,
];

/// Flags that we want to allow to switch at runtime (assuming RuntimeFeatureFlags is set)
pub const RUNTIME_FEATURE_FLAGS: &[FeatureFlag] = &[FeatureFlag::LocalClaudeCodexChildHarnesses];

/// Flags this fork forces off no matter what the channel, the compile-time
/// cargo features or a user preference say. `is_enabled` short-circuits on
/// these before it consults any other source.
///
/// This is a *hard* disable, not a default. It is only the right tool for a
/// flag whose subsystem genuinely cannot exist in a BYOP build -- an account
/// this fork has no server to authenticate against, or a surface driven by
/// that account. Anything that is merely off-by-default belongs in the
/// channel lists above (or nowhere, which already means off) so it stays
/// reachable.
///
/// `AgentModeComputerUse` used to be in this list. It was added by `5013248be`
/// ("hide Cloud Oz / Cloud Agent Computer Use / Privacy pages"), in the same
/// sweep as the `CloudMode*` flags, on the assumption that computer use was a
/// Warp cloud-agent capability. It is not: computer use drives the local
/// machine's own mouse and keyboard through `crates/computer_use` and never
/// touches a server -- see the "common false positives" section of
/// `DECLINED.md`. Hard-disabling it made the restored client-side dispatch
/// path unreachable, so it has been removed from the list. What keeps computer
/// use off for a user who has not asked for it is the per-profile
/// `ComputerUsePermission`, which defaults to `Never`.
pub const FORCE_DISABLED_FLAGS: &[FeatureFlag] = &[
    // Forces a Warp account sign-in before the app is usable. There is no
    // account and no server to sign in to.
    FeatureFlag::ForceLogin,
    // Renders the signed-in user's avatar in the tab bar. No account, no avatar.
    FeatureFlag::AvatarInTabBar,
    // The `/remote-control` chip and slash command, i.e. driving this agent
    // from another device. Disabled by `1289d311c`; left as-is here.
    FeatureFlag::HOARemoteControl,
];

impl FeatureFlag {
    /// Whether this flag is one of [`FORCE_DISABLED_FLAGS`], i.e. off
    /// regardless of channel, cargo features, user preference or test override.
    pub fn is_force_disabled(&self) -> bool {
        // `contains` over a 3-element slice; `FeatureFlag` is `PartialEq`.
        FORCE_DISABLED_FLAGS.contains(self)
    }

    pub fn is_enabled(&self) -> bool {
        if self.is_force_disabled() {
            return false;
        }

        #[cfg(all(debug_assertions, not(feature = "test-util")))]
        {
            use std::sync::atomic::Ordering;
            assert!(
                FEATURES_INITIALIZED.load(Ordering::Relaxed),
                "Tried to check FeatureFlag::{self:?} before feature flags were initialized"
            );
        }

        overrides::get_override(*self)
            .or(USER_PREFERENCE_MAP[*self as usize].get())
            .or(Some(FLAG_STATES[*self as usize].load(Ordering::Relaxed)))
            .unwrap_or(false)
    }

    #[allow(dead_code)]
    pub fn set_enabled(self, enabled: bool) {
        // Allow calling this in integration tests because we sometimes use it in the app
        // during flows that integration tests cover.
        if cfg!(test) && cfg!(not(feature = "integration_tests")) {
            panic!("Tried to globally enable {self:?} in a test. Use FeatureFlag::{self:?}.override_enabled instead");
        }
        FLAG_STATES[self as usize].store(enabled, Ordering::Relaxed);
    }

    /// Sets a user preference for this flag. User preferences take precedence
    /// over the global feature flag state, and can be used to allow explicit opt-in
    /// and explicit opt-out behavior.
    pub fn set_user_preference(self, enabled: bool) {
        USER_PREFERENCE_MAP[self as usize].set(enabled);
    }

    /// Sets a thread-local test override for this flag. The override lasts
    /// until the returned guard is dropped.
    ///
    /// **Warning**: overrides do not work for tests of multi-threaded code. If
    /// you need to test multi-threaded code that's behind a feature flag, you'll
    /// need to set an override in _each_ thread.
    ///
    /// Tests should create overrides early on and allow them to be
    /// dropped automatically when they finish. This keeps overrides scoped to
    /// the duration of the test, since Rust doesn't have test lifecycle hooks.
    #[cfg(feature = "test-util")]
    pub fn override_enabled(self, enabled: bool) -> overrides::OverrideGuard {
        overrides::override_flag(self, enabled)
    }

    pub fn flag_description(&self) -> Option<&'static str> {
        use FeatureFlag::*;

        // Note: many feature flags are purposefully omitted from this list, in order to avoid blowing up
        // the Preview changelog. Features below which are enabled for Preview via PREVIEW_FLAGS, will be added to the changelog.
        // Features which are added to Stable should ideally have their feature flag removed entirely, but at the
        // very least, the feature flag should be removed from the Preview changelog by removing it from PREVIEW_FLAGS.
        // ** ONLY Preview-exclusive features should be added to this list! **
        match self {
            CodeReviewFind => Some("Enables the find bar in the code review pane."),
            BlocklistMarkdownImages => {
                Some("Enables rendering markdown images inline in AI block list responses.")
            }
            GlobalSearch => Some("Enables global search in the left panel"),
            BlocklistMarkdownTableRendering => {
                Some("Enables rendering markdown tables inline in AI block list responses.")
            }
            MarkdownTables => Some("Enables rendering and interaction support for markdown tables in notebooks."),
            SettingsFile => Some("Enables configuring Phosphor via a user-editable `settings.toml` file, with hot reload and error reporting for invalid values."),
            GitOperationsInCodeReview => Some("Enables commit, push, and create-PR actions directly from the code review panel."),
            _ => None,
        }
    }
}

/// Marks that feature flags have been globally initialized.
pub fn mark_initialized() {
    #[cfg(debug_assertions)]
    FEATURES_INITIALIZED.store(true, std::sync::atomic::Ordering::Relaxed);
}

#[cfg(not(feature = "test-util"))]
mod overrides {
    #[inline(always)]
    pub fn get_override(_flag: super::FeatureFlag) -> Option<bool> {
        None
    }
}

/// Thread-local feature flag overrides for unit tests. For isolation, tests
/// should use overrides instead of globally modifying flags with [`super::FeatureFlag::set_enabled`].
#[cfg(feature = "test-util")]
mod overrides {
    use std::{cell::RefCell, collections::HashMap};

    use super::FeatureFlag;

    thread_local! {
        static FLAG_OVERRIDES: RefCell<HashMap<FeatureFlag,bool>> = RefCell::new(HashMap::new());
    }

    /// RAII guard to set feature flag overrides in tests. When the guard is
    /// dropped, it reverts to the global flag state.
    #[must_use = "if unused the override will be immediately cleared"]
    pub struct OverrideGuard {
        flag: FeatureFlag,
    }

    /// Gets the overridden state for a flag, if set.
    pub fn get_override(flag: FeatureFlag) -> Option<bool> {
        FLAG_OVERRIDES.with(|overrides| overrides.borrow().get(&flag).copied())
    }

    /// Gets the set of overridden flags.
    pub fn get_overrides() -> HashMap<FeatureFlag, bool> {
        FLAG_OVERRIDES.with(|overrides| overrides.borrow().clone())
    }

    /// Applies a set of overrides.
    ///
    /// This is intended to be used with [`get_overrides`] to apply a set of
    /// existing overrides to a newly-spawned thread.  If you are trying to
    /// override a single feature flag, use [`FeatureFlag::override_enabled`]
    /// instead.
    pub fn set_overrides(new_overrides: HashMap<FeatureFlag, bool>) {
        FLAG_OVERRIDES.with(|overrides| *overrides.borrow_mut() = new_overrides);
    }

    /// Set a thread-local override for a flag.
    pub fn override_flag(flag: FeatureFlag, enabled: bool) -> OverrideGuard {
        set_override(flag, enabled);
        OverrideGuard { flag }
    }

    fn set_override(flag: FeatureFlag, enabled: bool) {
        FLAG_OVERRIDES.with(|overrides| {
            let previous = overrides.borrow_mut().insert(flag, enabled);
            // We could support nested overrides, but it requires some care around
            // out-of-order drops - if overrides are set and then cleared out of
            // order, what should the state after each drop be?
            if previous.is_some() {
                panic!("Multiple overrides set for {flag:?}");
            }
        });
    }

    fn clear_override(flag: FeatureFlag) {
        FLAG_OVERRIDES.with(|overrides| {
            let previous = overrides.borrow_mut().remove(&flag);
            if previous.is_none() {
                panic!("Cleared override for {flag:?}, but none was set");
            }
        });
    }

    impl Drop for OverrideGuard {
        fn drop(&mut self) {
            clear_override(self.flag);
        }
    }
}

/// An atomic tri-state value.
///
/// This is initially unset, and can be set to a true or false value.
///
/// Writes and reads use [`Ordering::Relaxed`], so should not be used for
/// synchronization.
struct AtomicTriState(AtomicU8);

impl AtomicTriState {
    const fn new() -> Self {
        Self(AtomicU8::new(TriState::Unset as u8))
    }

    fn get(&self) -> Option<bool> {
        TriState::from(self.0.load(Ordering::Relaxed)).into()
    }

    fn set(&self, value: bool) {
        self.0.store(TriState::from(value) as u8, Ordering::Relaxed);
    }
}

/// A simple enum representing a tristate, to be used as the backing type
/// for [`AtomicTriState`].
enum TriState {
    Unset = 0,
    False = 1,
    True = 2,
}

impl From<bool> for TriState {
    fn from(value: bool) -> Self {
        if value {
            TriState::True
        } else {
            TriState::False
        }
    }
}

impl From<u8> for TriState {
    fn from(value: u8) -> Self {
        match value {
            0 => TriState::Unset,
            1 => TriState::False,
            2 => TriState::True,
            _ => unreachable!(),
        }
    }
}

impl From<TriState> for Option<bool> {
    fn from(value: TriState) -> Self {
        match value {
            TriState::Unset => None,
            TriState::False => Some(false),
            TriState::True => Some(true),
        }
    }
}

#[cfg(test)]
#[path = "features_test.rs"]
mod tests;
