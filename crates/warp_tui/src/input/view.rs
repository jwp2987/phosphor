//! [`TuiInputView`] — ratatui-rendered TUI prompt input.
//!
//! Implements [`TuiView`] + [`TypedActionView`]. The view:
//!
//! - Holds a [`ModelHandle<CodeEditorModel>`] constructed in `LayoutMode::CharCell`.
//! - Renders the core [`TuiEditorElement`] verbatim (editable, scroll-windowed).
//! - Owns prompt submission and the `!` shell-mode composition.
//! - Dispatches keystrokes as [`TuiInputAction`] typed actions.
//! - Emits [`TuiInputViewEvent::Submitted`] when the user presses Enter.
//!
//! # Architecture
//!
//! The view works directly with [`CodeEditorModel`] (char-cell mode) so that future
//! TUI features — vim, syntax highlighting, diff, hidden lines — come for free from
//! the shared editor infrastructure. Rendering and mouse interaction come from the
//! shared core element ([`crate::editor_element`]). Editor session mechanisms live
//! model-side, mirroring the GUI split: viewport scroll state on the char-cell
//! render state (`CharCellState`), drag-selection state on the selection model,
//! visual-row kill edits on `CodeEditorModel`. What stays here is input policy:
//! prompt-only keybindings, submit, inline menus, and shell mode.
//!

use std::ops::Range;
use std::rc::Rc;

use string_offset::{ByteOffset, CharOffset};
use vim::vim::{MotionType, VimMode, VimModel, VimSubscriber as _};
use warp::editor::{CodeEditorModel, CodeEditorModelEvent};
use warp::settings::AppEditorSettings;
use warp::tui_export::{
    AIAgentExchangeId, AcceptSlashCommandOrSavedPrompt, BlocklistAIInputModel, ClientProfileId,
    InputType, LLMId, TuiMcpAction,
};
use warp_editor::model::CoreEditorModel;
use warpui_core::elements::MouseStateHandle;
use warpui_core::elements::tui::{TuiContainer, TuiElement, TuiFlex, TuiHoverable, TuiText};
use warpui_core::keymap::macros::*;
use warpui_core::keymap::{self, EditableBinding, Keystroke};
use warpui_core::text::{byte_offset_for_char_offset, count_chars_up_to_byte};
use warpui_core::{
    AppContext, BlurContext, Entity, FocusContext, ModelHandle, SingletonEntity as _, TuiView,
    TypedActionView, ViewContext, ViewHandle,
};

use crate::completions_menu::TuiAcceptedCompletion;
use crate::editor_element::{TuiEditorAction, TuiEditorElement, TuiEditorStyles};
use crate::editor_interaction::{
    TuiEditorBehavior, TuiEditorCommand, TuiEditorInteractionOutcome, TuiEditorState,
    apply_editor_action, apply_editor_clipboard_action, apply_editor_paste, follow_editor_cursor,
};
use crate::exchange_menu::TuiExchangeMenuAction;
use crate::inline_menu::{TuiInlineMenu, TuiInlineMenuAccepted, active_inline_menu};
use crate::input_hints;
use crate::input_mode_policy::{self, AI_LOCKED_CONFIG, SHELL_LOCKED_CONFIG};
use crate::input_suggestions_mode::{TuiInputSuggestionsMode, TuiInputSuggestionsModeModel};
use crate::keybindings::{
    KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG, PLAN_TOGGLE_AVAILABLE_FLAG, TUI_BINDING_GROUP,
};
use crate::read_only_menu::TuiReadOnlyMenuKind;
use crate::transcript_view::TuiTranscriptView;
use crate::tui_builder::TuiUiBuilder;

/// Keymap-context flag set while the input has contextual Escape behavior.
///
/// The input owns a single Escape binding so modes can arbitrate explicitly in
/// [`TuiInputView::handle_escape`] instead of relying on keymap registration
/// order. Inline menus take priority; later input modes should be handled only
/// after the menu branch.
const INPUT_HANDLES_ESCAPE_FLAG: &str = "TuiInputHandlesEscape";
// ─────────────────────────────────────────────────────────────────────────────
// Keybindings
// ─────────────────────────────────────────────────────────────────────────────

/// Registers the input view's editing keybindings (the readline/chord
/// table). Called once at TUI startup from `keybindings::init` — these
/// bindings exist only in the TUI process; the GUI never registers them.
///
/// Each command is an [`EditableBinding`] named `tui:input:*`, so it is
/// user-remappable by name (via `keybindings.yaml`, once the TUI loads
/// overrides — a follow-up). Commands with multiple default keys register one
/// binding per key under the same name, which the keymap supports directly:
/// it tracks every binding registered under a name, and a custom-trigger
/// override replaces the trigger on all of them. Printable-character
/// insertion is not a binding — it stays element-level in
/// [`TuiEditorElement`]'s event dispatch, matching the GUI.
pub fn init(app: &mut AppContext) {
    app.register_editable_bindings([
        // Submit and contextual Escape are prompt policy, not editor policy.
        EditableBinding::new(
            "tui:input:submit",
            "Submit the input",
            TuiInputAction::Submit,
        )
        .with_context_predicate(id!("TuiInputView"))
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("enter"),
        EditableBinding::new(
            "tui:input:handle_escape",
            "Handle contextual input escape",
            TuiInputAction::HandleEscape,
        )
        .with_context_predicate(id!("TuiInputView") & id!(INPUT_HANDLES_ESCAPE_FLAG))
        .with_group(TUI_BINDING_GROUP)
        .with_key_binding("escape"),
    ]);
}

// ─────────────────────────────────────────────────────────────────────────────
// View events
// ─────────────────────────────────────────────────────────────────────────────

/// Events emitted by [`TuiInputView`].
#[derive(Debug, Clone)]
pub enum TuiInputViewEvent {
    /// The user pressed Enter to submit the current input. Contains the final text.
    Submitted(String),
    /// The terminal delivered one complete bracketed-paste payload.
    Pasted(String),
    /// Backspace was pressed with an empty normal input.
    BackspaceAtEmptyInput,
    /// The user selected a slash command menu item.
    AcceptedSlashCommand(AcceptSlashCommandOrSavedPrompt),
    /// The user selected a conversation menu item.
    AcceptedConversation(warp::tui_export::AgentConversationEntryId),
    /// The user selected a model menu item.
    AcceptedModel(LLMId),
    /// The user selected an action from the MCP menu.
    AcceptedMcp(TuiMcpAction),
    /// Shift+Up should move focus from the first visual row to the region above.
    MoveFocusUp,
    /// The user accepted a row from the up-arrow prompt-and-command history
    /// menu. Carries the text to fill into the input and submit, and the
    /// row's kind (issue #387).
    AcceptedPromptAndCommandHistory(String, warp::tui_export::TuiUpArrowHistoryItemKind),
    /// The user accepted a shell command/path completion from the Tab-completion
    /// popup. The session view applies the replacement to the input buffer.
    AcceptedCompletion(TuiAcceptedCompletion),
    /// The user selected an agent execution profile from the `/profile` picker.
    AcceptedProfile(ClientProfileId),
    /// The user selected a saved prompt from the `/prompts` picker. Carries the
    /// prompt's query text to insert into the input.
    AcceptedPrompt(String),
    /// The user selected an exchange from the `/fork-from` or `/rewind` picker.
    AcceptedExchange(AIAgentExchangeId, TuiExchangeMenuAction),
    /// The vim mode changed (Insert<->Normal<->Visual<->Replace). Emitted so the
    /// parent session view can re-render its footer vim-mode indicator.
    /// Selected prompt text was copied to the host clipboard.
    ClipboardCopySucceeded,
    /// Selected prompt text could not be copied to the host clipboard.
    ClipboardCopyFailed,
    VimModeChanged,
}

// ─────────────────────────────────────────────────────────────────────────────
// Typed action enum
// ─────────────────────────────────────────────────────────────────────────────

/// Prompt policy plus shared editor actions dispatched to [`TuiInputView`].
///
/// Each variant corresponds to one or more keybindings.
#[derive(Debug, Clone)]
pub enum TuiInputAction {
    /// Apply input emitted by the shared editor element.
    Editor(TuiEditorAction),
    /// Submit the current input (`Enter`).
    Submit,
    /// Handle contextual input Escape behavior, prioritizing an open inline menu.
    HandleEscape,
    /// Apply an editing command shared with generic TUI editors.
    EditorCommand(TuiEditorCommand),
    /// Place the cursor at `offset` without starting a drag selection
    /// (the `!` gutter click).
    SetCursor { offset: CharOffset },
}

/// A snapshot of the input buffer and cursor position at the moment a shell
/// completion request was issued, used to detect staleness once results
/// arrive (the user may have kept typing while the async fetch was in
/// flight).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TuiCompletionInputSnapshot {
    pub(crate) buffer_text: String,
    pub(crate) cursor_byte_offset: usize,
}

// ─────────────────────────────────────────────────────────────────────────────
// View
// ─────────────────────────────────────────────────────────────────────────────

/// The `TuiView`-implementing entry point for the TUI prompt input.
pub struct TuiInputView {
    /// The backing code editor in char-cell (terminal) mode. Also owns the
    /// editor session state the input drives: viewport scroll (char-cell
    /// render state) and drag-selection state (selection model).
    model: ModelHandle<CodeEditorModel>,
    /// Shared input-mode state driving NLD and explicit shell-mode handling.
    input_mode: ModelHandle<BlocklistAIInputModel>,
    /// Single authoritative menu mode, mirroring the GUI input's suggestions mode.
    suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
    /// Generalized inline menus used to route prioritized menu actions.
    inline_menus: Vec<TuiInlineMenu>,
    /// Shared editor session state, including the single-entry kill buffer.
    editor_state: TuiEditorState,
    /// Multiline insertion and six-row viewport policy.
    editor_behavior: TuiEditorBehavior,
    /// Mouse state for the shell-mode `!` gutter; created once here (not inline
    /// during render) so mouse tracking survives per-frame element rebuilds.
    prefix_mouse_state: MouseStateHandle,
    /// Whether this view is focused, tracked via `on_focus`/`on_blur` like
    /// the GUI's `EditorView::focused`. Snapshotted into the editor element
    /// so it only consumes typed text while the input is focused.
    focused: bool,
    /// Source of truth for whether a rendered plan can be toggled. Production
    /// construction always provides this; isolated input tests omit it.
    transcript: Option<ViewHandle<TuiTranscriptView>>,
    keyboard_enhancement_supported: bool,
    /// Consults the owner live for whether orchestration tabs are available.
    orchestration_tabs_available: Rc<dyn Fn(&AppContext) -> bool>,
    /// Consults the owner live before an inline-menu Enter can accept an item.
    can_accept_inline_menu: Rc<dyn Fn(&AppContext) -> bool>,
    /// Vim model (shared FSA + event dispatch layer). Always present but only
    /// active when `AppEditorSettings::vim_mode_enabled()` returns `true`.
    /// Wired via `VimSubscriber` to dispatch `VimEvent`s to `VimHandler` impls
    /// on `TuiInputView` (see `input/vim.rs`).
    vim_model: ModelHandle<VimModel>,
    /// Internal yank / delete clipboard for vim operations. Separate from the
    /// OS clipboard so that `p`/`P` work even without clipboard access.
    yank_buffer: String,
    /// Selection shape stored in `yank_buffer`, used to preserve linewise
    /// `dd`/`yy` and Visual-line paste semantics.
    yank_motion_type: MotionType,
}

impl Entity for TuiInputView {
    type Event = TuiInputViewEvent;
}

impl TuiInputView {
    /// Construct a new `TuiInputView` backed by `model` (must be in char-cell
    /// mode). Construction stays crate-internal because `inline_menu` is the
    /// crate-private active-menu adapter; keeping this as the only constructor
    /// prevents menu and non-menu initialization paths from diverging.
    ///
    /// The model carries the terminal width (set via
    /// [`CodeEditorModel::new_tui`]); the view does not keep its own copy.
    ///
    /// `input_mode` is the shared input-mode model backing detected and explicit shell-mode
    /// handling; the view re-renders whenever the mode changes.
    ///
    /// Subscribes to [`CodeEditorModelEvent::ContentChanged`] to trigger re-renders
    /// whenever the buffer changes from outside `handle_action`.
    pub(crate) fn new(
        model: ModelHandle<CodeEditorModel>,
        input_mode: ModelHandle<BlocklistAIInputModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        inline_menus: Vec<TuiInlineMenu>,
        transcript: ViewHandle<TuiTranscriptView>,
        orchestration_tabs_available: impl Fn(&AppContext) -> bool + 'static,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        Self::new_internal(
            model,
            input_mode,
            suggestions_mode,
            inline_menus,
            Some(transcript),
            orchestration_tabs_available,
            ctx,
        )
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(
        model: ModelHandle<CodeEditorModel>,
        input_mode: ModelHandle<BlocklistAIInputModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        inline_menus: Vec<TuiInlineMenu>,
        orchestration_tabs_available: impl Fn(&AppContext) -> bool + 'static,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        Self::new_internal(
            model,
            input_mode,
            suggestions_mode,
            inline_menus,
            None,
            orchestration_tabs_available,
            ctx,
        )
    }

    fn new_internal(
        model: ModelHandle<CodeEditorModel>,
        input_mode: ModelHandle<BlocklistAIInputModel>,
        suggestions_mode: ModelHandle<TuiInputSuggestionsModeModel>,
        inline_menus: Vec<TuiInlineMenu>,
        transcript: Option<ViewHandle<TuiTranscriptView>>,
        orchestration_tabs_available: impl Fn(&AppContext) -> bool + 'static,
        ctx: &mut ViewContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&model, |_, _, event, ctx| {
            if matches!(event, CodeEditorModelEvent::ContentChanged { .. }) {
                ctx.notify();
            }
        });
        // The model only emits on real config changes, and rendering branches
        // on the config (shell-mode gutter/border), so every event re-renders.
        ctx.subscribe_to_model(&input_mode, |_, _, _, ctx| ctx.notify());
        ctx.subscribe_to_model(&suggestions_mode, |_, _, _, ctx| ctx.notify());
        let vim_model = ctx.add_model(|_| VimModel::new());
        // Subscribe to vim events: VimSubscriber blanket impl (TuiInputView: VimHandler)
        // dispatches each VimEvent to the appropriate VimHandler method.
        ctx.subscribe_to_model(&vim_model, Self::handle_vim_event);
        Self {
            model,
            input_mode,
            suggestions_mode,
            inline_menus,
            editor_state: TuiEditorState::default(),
            editor_behavior: TuiEditorBehavior::multiline(6).with_copy_on_mouse_highlight(),
            prefix_mouse_state: MouseStateHandle::default(),
            focused: false,
            transcript,
            keyboard_enhancement_supported: false,
            orchestration_tabs_available: Rc::new(orchestration_tabs_available),
            can_accept_inline_menu: Rc::new(|_| true),
            vim_model,
            yank_buffer: String::new(),
            yank_motion_type: MotionType::Charwise,
        }
    }

    pub(crate) fn with_inline_menu_actions_allowed(
        mut self,
        can_accept_inline_menu: impl Fn(&AppContext) -> bool + 'static,
    ) -> Self {
        self.can_accept_inline_menu = Rc::new(can_accept_inline_menu);
        self
    }

    pub(crate) fn with_keyboard_enhancement_supported(
        mut self,
        keyboard_enhancement_supported: bool,
    ) -> Self {
        self.keyboard_enhancement_supported = keyboard_enhancement_supported;
        self
    }

    fn plan_toggle_available(&self, ctx: &AppContext) -> bool {
        self.transcript
            .as_ref()
            .is_some_and(|transcript| transcript.as_ref(ctx).has_toggleable_plan(ctx))
    }

    /// Whether vim mode is enabled in settings.
    ///
    /// Returns `false` when [`AppEditorSettings`] has not been registered in
    /// the context (e.g. in lightweight test fixtures that don't boot the full
    /// settings stack).
    pub(crate) fn vim_mode_enabled(&self, ctx: &AppContext) -> bool {
        ctx.has_singleton_model::<AppEditorSettings>()
            && AppEditorSettings::as_ref(ctx).vim_mode_enabled()
    }

    /// Reset the vim state machine to insert mode. Called when vim mode is
    /// enabled (so the user starts in insert mode, not whatever mode they
    /// were in previously).
    pub(crate) fn reset_vim_to_insert(&mut self, ctx: &mut ViewContext<Self>) {
        self.vim_model
            .update(ctx, |vim, ctx| vim.force_insert_mode(ctx));
        // force_insert_mode bypasses the normal ChangeMode event path; notify
        // the footer indicator manually so it reflects the new Insert state.
        ctx.emit(TuiInputViewEvent::VimModeChanged);
    }

    /// The current vim mode, or `None` when vim mode is disabled.
    pub(crate) fn vim_mode(&self, ctx: &AppContext) -> Option<VimMode> {
        if self.vim_mode_enabled(ctx) {
            Some(self.vim_model.as_ref(ctx).state().mode)
        } else {
            None
        }
    }

    /// Whether the input is in detected or explicitly locked shell mode.
    pub(crate) fn is_shell_mode(&self, ctx: &AppContext) -> bool {
        input_mode_policy::is_shell_mode(self.input_mode.as_ref(ctx))
    }

    /// Returns a handle to the backing [`CodeEditorModel`].
    pub fn model(&self) -> &ModelHandle<CodeEditorModel> {
        &self.model
    }

    /// Whether the input buffer is empty.
    pub fn is_empty(&self, ctx: &AppContext) -> bool {
        self.model.as_ref(ctx).content().as_ref(ctx).is_empty()
    }

    /// Clears the input buffer, resets to the setting-derived agent mode, and
    /// resets the viewport scroll.
    pub fn clear(&mut self, ctx: &mut ViewContext<Self>) {
        self.model.update(ctx, |m, ctx| m.clear_buffer(ctx));
        self.reset_to_default_agent_mode(ctx);
        if self.vim_mode_enabled(ctx) {
            self.reset_vim_to_insert(ctx);
        }
        // The cursor is back at the buffer start, so following it scrolls the
        // viewport back to the top.
        self.follow_cursor(ctx);
        ctx.notify();
    }

    /// Builds this frame's core editor element: editable, scroll-windowed, and
    /// dispatching [`TuiEditorAction`]s back as [`TuiInputAction`]s. `render`
    /// boxes it (behind the shell-mode `!` gutter when active); tests construct
    /// it directly to exercise mouse dispatch.
    fn render_element(&self, ctx: &AppContext) -> TuiEditorElement {
        let builder = TuiUiBuilder::from_app(ctx);
        let mut styles = TuiEditorStyles::default();
        if let Some(range) = self
            .inline_menus
            .iter()
            .find_map(|inline_menu| inline_menu.input_highlight_range(ctx))
        {
            styles
                .text_overrides
                .push((range, builder.slash_command_text_style()));
        }
        let mut element = TuiEditorElement::new(&self.model, ctx)
            .editable()
            .with_view_focused(self.focused)
            .with_viewport_rows(self.editor_behavior.viewport_rows())
            .with_styles(styles)
            .on_action(|action, event_ctx| {
                event_ctx.dispatch_typed_action(TuiInputAction::Editor(action))
            });
        if let VimMode::Visual(motion_type) = self.vim_model.as_ref(ctx).state().mode {
            let ranges = self
                .model
                .as_ref(ctx)
                .vim_visual_selection_ranges(motion_type, ctx);
            element = element.with_selection_ranges(ranges);
        }
        if let Some(hint_text) = self
            .inline_menus
            .iter()
            .find_map(|inline_menu| inline_menu.input_argument_hint_text(ctx))
        {
            element = element.with_trailing_ghost_text(hint_text, builder.dim_text_style());
        }
        // Empty-buffer placeholder hints depend on state that changes without
        // this view re-rendering (transcript emptiness flips when blocks land
        // via history events or PTY wakeups), so the hint is resolved by a
        // provider on every layout pass instead of being snapshotted here.
        // Shell mode teaches how to exit; agent mode adapts to the transcript
        // state.
        let input_mode = self.input_mode.clone();
        let transcript = self.transcript.clone();
        let orchestration_tabs_available = self.orchestration_tabs_available.clone();
        let suggestions_mode = self.suggestions_mode.clone();
        element.with_placeholder_ghost_text(move |app| {
            // Suppress the hint while the read-only shortcuts/status overlay
            // (opened by `?` / `/status`) is showing: the sheet already lists the
            // shortcuts, so the ghosted "? for shortcuts" line under it is both
            // redundant and self-contradictory. Mirrors the pin, whose
            // `TuiTerminalSessionStateModel::hint_text` does the same check.
            if suggestions_mode
                .as_ref(app)
                .mode()
                .read_only_menu()
                .is_some()
            {
                return None;
            }
            let hint = if input_mode_policy::is_shell_mode(input_mode.as_ref(app)) {
                input_hints::SHELL_HINT.to_owned()
            } else {
                // Inputs constructed without a transcript (isolated tests)
                // count as zero-state.
                let transcript_is_empty = transcript
                    .as_ref()
                    .is_none_or(|transcript| transcript.as_ref(app).is_empty());
                input_hints::agent_input_hint(
                    transcript_is_empty,
                    orchestration_tabs_available(app),
                )
            };
            Some((hint, TuiUiBuilder::from_app(app).muted_text_style()))
        })
    }
    /// Collapses the current text selection to its head without changing text.
    pub(crate) fn clear_selection(&mut self, ctx: &mut ViewContext<Self>) {
        let head = self
            .model
            .as_ref(ctx)
            .buffer_selection_model()
            .as_ref(ctx)
            .first_selection_head();
        self.model.update(ctx, |model, ctx| {
            model.select_at(head, false, ctx);
            model.end_selection(ctx);
        });
        ctx.notify();
    }

    /// The editor element for this frame, boxed for the render tree.
    fn render_input(&self, ctx: &AppContext) -> Box<dyn TuiElement> {
        self.render_element(ctx).finish()
    }
    pub(crate) fn set_text(&mut self, text: &str, ctx: &mut ViewContext<Self>) {
        let text = self.editor_behavior.normalize_text(text);
        self.model.update(ctx, |m, ctx| {
            m.clear_buffer(ctx);
            m.user_insert(text, ctx);
        });
        self.follow_cursor(ctx);
        ctx.notify();
    }

    pub(crate) fn insert_typeahead_text(
        &mut self,
        previously_inserted: CharOffset,
        text: &str,
        ctx: &mut ViewContext<Self>,
    ) {
        self.model.update(ctx, |model, ctx| {
            model.replace_first_n_characters(previously_inserted, text, ctx);
            let end = model.content().as_ref(ctx).max_charoffset();
            model.cursor_at(end, ctx);
        });
        self.follow_cursor(ctx);
        ctx.notify();
    }

    /// Inserts a paste payload after the parent declines to consume it as
    /// structured input.
    pub(crate) fn insert_pasted_text(&mut self, text: &str, ctx: &mut ViewContext<Self>) {
        apply_editor_action(
            &self.model,
            &TuiEditorAction::PasteText(text.to_owned()),
            self.editor_behavior,
            ctx,
        );
        self.follow_cursor(ctx);
        ctx.notify();
    }

    /// Composes the input row: a mode-specific prompt gutter in two columns
    /// (glyph plus one column of right padding), then the editor filling the
    /// remaining width. The gutter is outside the editable area; clicking it
    /// places the cursor at the start of the buffer.
    ///
    /// `!` marks shell mode, `>` marks agent mode. The fork previously rendered
    /// the gutter only in shell mode and dropped straight to the bare editor in
    /// agent mode, so the agent prompt had no gutter at all (#465). Restored
    /// from the pin, which selects the glyph rather than gating the whole row.
    fn prompt_row(&self, ctx: &AppContext) -> Box<dyn TuiElement> {
        let builder = TuiUiBuilder::from_app(ctx);
        let (prefix_text, prefix_style) = if self.is_shell_mode(ctx) {
            ("!", builder.shell_command_accent_style())
        } else {
            (">", builder.accent_text_style())
        };
        let prefix = TuiHoverable::new(
            self.prefix_mouse_state.clone(),
            TuiContainer::new(TuiText::new(prefix_text).with_style(prefix_style).finish())
                .with_padding_right(1)
                .finish(),
        )
        .on_click(|event_ctx, _| {
            event_ctx.dispatch_typed_action(TuiInputAction::SetCursor {
                offset: CharOffset::from(1),
            });
        });
        TuiFlex::row()
            .child(prefix.finish())
            .flex_child(self.render_input(ctx))
            .finish()
    }
}

impl TuiView for TuiInputView {
    fn ui_name() -> &'static str {
        "TuiInputView"
    }

    fn render(&self, ctx: &AppContext) -> Box<dyn TuiElement> {
        self.prompt_row(ctx)
    }

    fn keymap_context(&self, ctx: &AppContext) -> keymap::Context {
        // In vim mode, escape is handled only when vim actually needs it:
        // - Non-Normal modes (Insert->Normal, Visual->Normal, Replace->Normal)
        // - Normal mode with pending input (clear the partial command)
        // In Normal mode with no pending input, escape is a no-op for vim;
        // passing it through allows session-level bindings (e.g.
        // orchestration focus-main, cancel-restore) to fire instead.
        let vim_mode_enabled = self.vim_mode_enabled(ctx);
        let vim_state = self.vim_model.as_ref(ctx).state();
        input_keymap_context(
            // An open read-only sheet (`?` shortcuts, `/status`) is the FIRST branch
            // `handle_escape` takes, but it is not an `active_inline_menu` -- those sheets
            // have no selection model and so are not registered in `inline_menus`. Without
            // this term the flag stays unset, the `escape` binding's context predicate does
            // not match, `HandleEscape` is never dispatched, and that top-priority branch is
            // unreachable from a real keypress: `?` then Escape leaves the sheet open and
            // fires a session-level Escape binding instead. The existing coverage all
            // dispatches `HandleEscape` directly, which bypasses the keymap and hides this.
            self.suggestions_mode
                .as_ref(ctx)
                .mode()
                .read_only_menu()
                .is_some()
                || self.active_inline_menu(ctx).is_some()
                || self.is_shell_mode(ctx)
                || (vim_mode_enabled
                    && (!matches!(vim_state.mode, VimMode::Normal)
                        || !vim_state.showcmd.is_empty())),
            self.plan_toggle_available(ctx),
            self.keyboard_enhancement_supported,
        )
    }

    fn on_focus(&mut self, focus_ctx: &FocusContext, ctx: &mut ViewContext<Self>) {
        if focus_ctx.is_self_focused() {
            self.focused = true;
            ctx.notify();
        }
    }

    fn on_blur(&mut self, blur_ctx: &BlurContext, ctx: &mut ViewContext<Self>) {
        if blur_ctx.is_self_blurred() {
            self.focused = false;
            // Losing focus dismisses any open inline menu the same way Escape
            // does (issue #387): otherwise the up-arrow history menu's preview
            // — including its input-type switch into shell mode — would
            // silently survive a focus change to another view.
            if let Some(inline_menu) = self.active_inline_menu(ctx) {
                inline_menu.dismiss(ctx);
            }
            ctx.notify();
        }
    }
}

fn input_keymap_context(
    input_handles_escape: bool,
    plan_toggle_available: bool,
    keyboard_enhancement_supported: bool,
) -> keymap::Context {
    let mut context = keymap::Context::default();
    context.set.insert(TuiInputView::ui_name());
    if input_handles_escape {
        context.set.insert(INPUT_HANDLES_ESCAPE_FLAG);
    }
    if plan_toggle_available {
        context.set.insert(PLAN_TOGGLE_AVAILABLE_FLAG);
    }
    if keyboard_enhancement_supported {
        context.set.insert(KEYBOARD_ENHANCEMENT_AVAILABLE_FLAG);
    }
    context
}
impl TypedActionView for TuiInputView {
    type Action = TuiInputAction;

    fn handle_action(&mut self, action: &TuiInputAction, ctx: &mut ViewContext<Self>) {
        if self.handle_inline_menu_action(action, ctx) {
            return;
        }
        let outcome = match action {
            TuiInputAction::Editor(editor_action) => {
                if let TuiEditorAction::PasteText(text) = editor_action {
                    self.close_read_only_menu(ctx);
                    ctx.emit(TuiInputViewEvent::Pasted(text.clone()));
                    return;
                }
                // Any other editor action closes the shared read-only menu
                // (shortcuts/status) if it is open, mirroring how typing
                // dismisses the other suggestion menus.
                let closed_menu = self.close_read_only_menu(ctx);
                // Route every typed character through the shared Vim FSA when
                // enabled. Insert-mode routing is required for insert counts
                // and dot-repeat; prompt-specific insertion policy lives in
                // `VimHandler::insert_char`.
                if let TuiEditorAction::InsertChar(c) = *editor_action {
                    if self.vim_mode_enabled(ctx) {
                        let old_mode = self.vim_model.as_ref(ctx).state().mode;
                        self.vim_model
                            .update(ctx, |vim, ctx| vim.typed_character(c, ctx));
                        if self.vim_model.as_ref(ctx).state().mode != old_mode {
                            ctx.emit(TuiInputViewEvent::VimModeChanged);
                        }
                        return;
                    }
                }
                // A second `?` that merely closed the shortcuts sheet is
                // swallowed rather than inserted literally -- `?` is a toggle
                // trigger while the buffer is empty, not ordinary text.
                if closed_menu == Some(TuiReadOnlyMenuKind::Shortcuts)
                    && matches!(editor_action, TuiEditorAction::InsertChar('?'))
                {
                    return;
                }
                // A `?` typed at the very start of an empty, otherwise-idle
                // input opens the keyboard-shortcuts sheet instead of
                // inserting (matching the GUI's typed-only trigger).
                if matches!(editor_action, TuiEditorAction::InsertChar('?'))
                    && self.plain_text(ctx).is_empty()
                    && self.is_cursor_at_start(ctx)
                    && matches!(
                        self.suggestions_mode.as_ref(ctx).mode(),
                        TuiInputSuggestionsMode::Closed
                    )
                {
                    self.suggestions_mode.update(ctx, |mode, ctx| {
                        mode.set_mode(
                            TuiInputSuggestionsMode::ReadOnlyMenu(TuiReadOnlyMenuKind::Shortcuts),
                            ctx,
                        );
                    });
                    return;
                }
                // A `!` typed at the very start of the input enters shell mode
                // instead of inserting (matching the GUI's typed-only trigger).
                if matches!(editor_action, TuiEditorAction::InsertChar('!'))
                    && !self.is_shell_mode(ctx)
                    && self.is_cursor_at_start(ctx)
                    && !self
                        .input_mode
                        .as_ref(ctx)
                        .is_terminal_use_active_or_pending()
                {
                    self.enter_shell_mode(ctx);
                    TuiEditorInteractionOutcome::FollowCursor
                } else {
                    apply_editor_action(&self.model, editor_action, self.editor_behavior, ctx)
                }
            }
            TuiInputAction::Submit => {
                self.close_read_only_menu(ctx);
                self.submit(ctx);
                TuiEditorInteractionOutcome::FollowCursor
            }
            TuiInputAction::HandleEscape => {
                self.handle_escape(ctx);
                TuiEditorInteractionOutcome::FollowCursor
            }
            TuiInputAction::EditorCommand(command) => {
                self.close_read_only_menu(ctx);
                if matches!(*command, TuiEditorCommand::SelectUp) && self.can_focus_above(ctx) {
                    ctx.emit(TuiInputViewEvent::MoveFocusUp);
                    return;
                }
                let vim_keystroke = match *command {
                    TuiEditorCommand::Backspace
                        if !self.is_cursor_at_start(ctx)
                            || (!self.is_shell_mode(ctx) && !self.plain_text(ctx).is_empty()) =>
                    {
                        Some("backspace")
                    }
                    TuiEditorCommand::DeleteForward => Some("delete"),
                    TuiEditorCommand::InsertNewline => Some("shift-enter"),
                    TuiEditorCommand::Backspace
                    | TuiEditorCommand::DeleteWordBackward
                    | TuiEditorCommand::DeleteWordForward
                    | TuiEditorCommand::MoveLeft
                    | TuiEditorCommand::MoveRight
                    | TuiEditorCommand::MoveUp
                    | TuiEditorCommand::MoveDown
                    | TuiEditorCommand::MoveWordLeft
                    | TuiEditorCommand::MoveWordRight
                    | TuiEditorCommand::MoveToLineStart
                    | TuiEditorCommand::MoveToLineEnd
                    | TuiEditorCommand::SelectLeft
                    | TuiEditorCommand::SelectRight
                    | TuiEditorCommand::SelectUp
                    | TuiEditorCommand::SelectDown
                    | TuiEditorCommand::SelectWordLeft
                    | TuiEditorCommand::SelectWordRight
                    | TuiEditorCommand::SelectToLineStart
                    | TuiEditorCommand::SelectToLineEnd
                    | TuiEditorCommand::SelectAll
                    | TuiEditorCommand::Copy
                    | TuiEditorCommand::Cut
                    | TuiEditorCommand::Paste
                    | TuiEditorCommand::KillToLineEnd
                    | TuiEditorCommand::KillToLineStart
                    | TuiEditorCommand::Yank
                    | TuiEditorCommand::Undo
                    | TuiEditorCommand::Redo => None,
                };
                if self.vim_mode_enabled(ctx) {
                    if let Some(keystroke) = vim_keystroke {
                        let old_mode = self.vim_model.as_ref(ctx).state().mode;
                        let keystroke =
                            Keystroke::parse(keystroke).expect("static Vim keystroke is valid");
                        self.vim_model
                            .update(ctx, |vim, ctx| vim.keypress(&keystroke, ctx));
                        if self.vim_model.as_ref(ctx).state().mode != old_mode {
                            ctx.emit(TuiInputViewEvent::VimModeChanged);
                        }
                        return;
                    }
                }
                // Only open the conversation list from normal agent input; in
                // `!` shell mode the `!` prefix is not part of `plain_text`, so
                // an empty shell command would otherwise trip this branch and
                // open the picker while the input stayed shell-mode.
                if matches!(*command, TuiEditorCommand::MoveLeft)
                    && !self.is_shell_mode(ctx)
                    && self.plain_text(ctx).is_empty()
                    && self.is_cursor_at_start(ctx)
                {
                    self.open_inline_menu(TuiInputSuggestionsMode::ConversationMenu, ctx);
                    TuiEditorInteractionOutcome::FollowCursor
                } else if matches!(*command, TuiEditorCommand::MoveUp)
                    && self.single_cursor_on_first_row(ctx)
                {
                    // Issue #387: agent mode lists prompts and commands, shell
                    // mode lists commands only — both open the same combined
                    // menu, which does its own mode-based filtering.
                    self.open_inline_menu(TuiInputSuggestionsMode::PromptAndCommandHistory, ctx);
                    TuiEditorInteractionOutcome::FollowCursor
                // With nothing left to delete, backspace removes the `!`
                // affordance instead; typed text is preserved.
                } else if matches!(*command, TuiEditorCommand::Backspace)
                    && self.is_shell_mode(ctx)
                    && self.is_cursor_at_start(ctx)
                {
                    self.exit_shell_mode(ctx);
                    TuiEditorInteractionOutcome::FollowCursor
                } else if matches!(*command, TuiEditorCommand::Backspace)
                    && self.plain_text(ctx).is_empty()
                    && self.is_cursor_at_start(ctx)
                {
                    ctx.emit(TuiInputViewEvent::BackspaceAtEmptyInput);
                    TuiEditorInteractionOutcome::FollowCursor
                } else {
                    self.editor_state.apply_command(
                        &self.model,
                        *command,
                        self.editor_behavior,
                        ctx,
                    )
                }
            }
            TuiInputAction::SetCursor { offset } => {
                // Clicking in the input switches to insert mode in vim.
                if self.vim_mode_enabled(ctx)
                    && !matches!(self.vim_model.as_ref(ctx).state().mode, VimMode::Insert)
                {
                    self.vim_model
                        .update(ctx, |vim, ctx| vim.force_insert_mode(ctx));
                    ctx.emit(TuiInputViewEvent::VimModeChanged);
                }
                self.model.update(ctx, |m, ctx| {
                    m.select_at(*offset, false, ctx);
                    m.end_selection(ctx);
                });
                TuiEditorInteractionOutcome::FollowCursor
            }
        };
        let outcome = match outcome {
            TuiEditorInteractionOutcome::Clipboard(action) => {
                match apply_editor_clipboard_action(&self.model, action, ctx) {
                    Ok(true) => ctx.emit(TuiInputViewEvent::ClipboardCopySucceeded),
                    Ok(false) => {}
                    Err(error) => {
                        log::error!("Failed to copy TUI input selection: {error}");
                        ctx.emit(TuiInputViewEvent::ClipboardCopyFailed);
                    }
                }
                TuiEditorInteractionOutcome::FollowCursor
            }
            TuiEditorInteractionOutcome::Paste => {
                if let Err(error) = apply_editor_paste(&self.model, self.editor_behavior, ctx) {
                    log::error!("Failed to paste into TUI input: {error}");
                }
                TuiEditorInteractionOutcome::FollowCursor
            }
            outcome => outcome,
        };
        if outcome == TuiEditorInteractionOutcome::FollowCursor {
            self.follow_cursor(ctx);
        }
        ctx.notify();
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// View-level TUI helpers
// ─────────────────────────────────────────────────────────────────────────────

impl TuiInputView {
    // ── Read helpers ──────────────────────────────────────────────────────────
    fn open_inline_menu(&self, mode: TuiInputSuggestionsMode, ctx: &mut ViewContext<Self>) {
        if let Some(menu) = self.inline_menus.iter().find(|menu| menu.mode() == mode) {
            menu.open(ctx);
        }
    }

    fn plain_text(&self, ctx: &AppContext) -> String {
        let inner = self.model.as_ref(ctx);
        let buffer = inner.content().as_ref(ctx);
        if buffer.is_empty() {
            return String::new();
        }
        buffer.text().into_string()
    }

    /// Captures the buffer text and cursor position for a shell-completion
    /// request, or `None` when the selection is not a single cursor (a
    /// completion request needs one exact position, not a range).
    ///
    /// Unlike the pin, this is not gated on shell mode: this fork's Tab
    /// completion is available in both shell and agent-composer input (see
    /// `TRIGGER_COMPLETIONS_BINDING_NAME`'s context predicate), so gating the
    /// snapshot itself would silently disable completion outside shell mode.
    pub(crate) fn completion_snapshot(
        &self,
        ctx: &AppContext,
    ) -> Option<TuiCompletionInputSnapshot> {
        if !self.model.as_ref(ctx).selection_is_single_cursor(ctx) {
            return None;
        }
        let buffer_text = self.plain_text(ctx);
        let cursor_char_offset = self
            .model
            .as_ref(ctx)
            .buffer_selection_model()
            .as_ref(ctx)
            .first_selection_head()
            .as_usize()
            .saturating_sub(1);
        let cursor_byte_offset =
            byte_offset_for_char_offset(&buffer_text, CharOffset::from(cursor_char_offset))?
                .as_usize();
        Some(TuiCompletionInputSnapshot {
            buffer_text,
            cursor_byte_offset,
        })
    }

    /// Applies an accepted shell completion, replacing `acceptance`'s byte
    /// range with its replacement text (plus a trailing space when
    /// `append_space` is set). Returns `false` when the range is stale
    /// relative to the current buffer.
    pub(crate) fn apply_shell_completion(
        &mut self,
        acceptance: TuiAcceptedCompletion,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        let buffer_text = self.plain_text(ctx);
        let replacement_range = acceptance.span;
        if replacement_range.start > replacement_range.end {
            return false;
        }
        let Some(replacement_start) =
            count_chars_up_to_byte(&buffer_text, ByteOffset::from(replacement_range.start))
        else {
            return false;
        };
        let Some(replacement_end) =
            count_chars_up_to_byte(&buffer_text, ByteOffset::from(replacement_range.end))
        else {
            return false;
        };
        let selection_start = CharOffset::from(replacement_start.as_usize() + 1);
        let selection_end = CharOffset::from(replacement_end.as_usize() + 1);
        let mut replacement = acceptance.replacement;
        if acceptance.append_space {
            replacement.push(' ');
        }
        self.model.update(ctx, |model, ctx| {
            model.select_at(selection_start, false, ctx);
            model.set_last_selection_head(selection_end, ctx);
            model.end_selection(ctx);
            model.user_insert(&replacement, ctx);
        });
        true
    }

    fn cursor_offset(&self, ctx: &AppContext) -> CharOffset {
        self.model
            .as_ref(ctx)
            .selection_model()
            .as_ref(ctx)
            .cursors(ctx)
            .into_iter()
            .next()
            .unwrap_or_default()
    }

    /// The selection as a 1-based gap range, or `None` when the selection is
    /// empty. Rendering reads the selection through the editor element; this
    /// backs cursor-position checks (e.g. shell-mode entry) and tests.
    fn selection_range(&self, ctx: &AppContext) -> Option<Range<CharOffset>> {
        let inner = self.model.as_ref(ctx);
        let sel = inner.buffer_selection_model().as_ref(ctx);
        let head = sel.first_selection_head();
        let tail = sel.first_selection_tail();
        if head == tail {
            None
        } else {
            let start = head.min(tail);
            let end = head.max(tail);
            Some(start..end)
        }
    }

    /// Whether the cursor sits at the very start of the buffer with no active
    /// selection (the position where `!` toggles shell mode).
    fn is_cursor_at_start(&self, ctx: &AppContext) -> bool {
        self.cursor_offset(ctx).as_usize() <= 1 && self.selection_range(ctx).is_none()
    }

    /// Whether Shift+Up should leave the input instead of extending selection.
    fn can_focus_above(&self, ctx: &AppContext) -> bool {
        (self.orchestration_tabs_available)(ctx) && self.single_cursor_on_first_row(ctx)
    }

    /// Whether the single caret sits on the first visual row of the input with
    /// no active selection — the position where Up opens the prompt-history
    /// menu. Accounts for soft-wrapping via the char-cell display lattice,
    /// mirroring the GUI editor view's `single_cursor_on_first_row`.
    fn single_cursor_on_first_row(&self, ctx: &AppContext) -> bool {
        if self.selection_range(ctx).is_some() {
            return false;
        }

        let model = self.model.as_ref(ctx);
        let render = model.render_state().as_ref(ctx);
        let Some(char_cell) = render.char_cell() else {
            return false;
        };

        let cursor_offset = CharOffset::from(self.cursor_offset(ctx).as_usize().saturating_sub(1));
        let hidden = char_cell.hidden_line_ranges(ctx);
        char_cell
            .display_lattice(&hidden)
            .offset_to_display_point(cursor_offset)
            .is_some_and(|point| point.row == 0)
    }

    // ── Scroll ─────────────────────────────────────────────────────────────
    //
    // The scroll offset and its clamping/follow policy live on the char-cell
    // render state (`CharCellState`); these helpers gather the inputs the
    // mechanism needs — the primary cursor and the model-derived hidden line
    // ranges — and apply the input's viewport policy.

    /// Scrolls the viewport the minimal amount needed to keep the cursor
    /// visible.
    fn follow_cursor(&self, ctx: &AppContext) {
        follow_editor_cursor(&self.model, self.editor_behavior, ctx);
    }

    // ── Shell mode ────────────────────────────────────────────────────────────

    /// Locks the shared input mode to shell with the `!` shell-prefix source.
    pub(crate) fn enter_shell_mode(&mut self, ctx: &mut ViewContext<Self>) {
        let is_input_buffer_empty = self.plain_text(ctx).is_empty();
        self.input_mode.clone().update(ctx, |input_mode, ctx| {
            // BYOP: Zap's set_input_config takes no decision-source arg (inert telemetry upstream).
            input_mode.set_input_config(SHELL_LOCKED_CONFIG, is_input_buffer_empty, ctx);
        });
    }

    /// Explicitly forces agent mode for the current buffer; any typed text is
    /// preserved. Clearing or submitting the buffer resumes setting-derived
    /// autodetection.
    pub(crate) fn exit_shell_mode(&mut self, ctx: &mut ViewContext<Self>) {
        let is_input_buffer_empty = self.plain_text(ctx).is_empty();
        self.input_mode.clone().update(ctx, |input_mode, ctx| {
            input_mode.set_input_config(AI_LOCKED_CONFIG, is_input_buffer_empty, ctx);
        });
    }

    /// Restores the setting-derived agent mode (autodetect-driven if enabled,
    /// else the locked AI default), preserving the current input buffer.
    /// Exposed to the crate so callers outside this view (e.g.
    /// `TuiTerminalSessionView::reset_after_agent_control`) can restore the
    /// default mode once an externally-installed AI lock -- attaching the
    /// agent to a running command -- has ended, without going through
    /// [`Self::clear`] and losing unrelated buffer contents.
    pub(crate) fn reset_to_default_agent_mode(&mut self, ctx: &mut ViewContext<Self>) {
        let is_autodetection_enabled = self
            .input_mode
            .as_ref(ctx)
            .is_autodetection_enabled_for_current_context(ctx);
        self.input_mode.clone().update(ctx, |input_mode, ctx| {
            if is_autodetection_enabled {
                input_mode.enable_autodetection(InputType::AI, ctx);
            } else {
                input_mode.set_input_config(AI_LOCKED_CONFIG, true, ctx);
            }
        });
    }

    // ── Submit ────────────────────────────────────────────────────────────────

    /// Emits [`TuiInputViewEvent::Submitted`] without clearing the buffer; the
    /// owner decides whether the submission is accepted and calls [`Self::clear`].
    fn submit(&mut self, ctx: &mut ViewContext<Self>) {
        let text = self.plain_text(ctx);
        ctx.emit(TuiInputViewEvent::Submitted(text));
    }

    pub(crate) fn route_inline_menu_acceptance(
        &mut self,
        accepted: TuiInlineMenuAccepted,
        ctx: &mut ViewContext<Self>,
    ) {
        match accepted {
            TuiInlineMenuAccepted::SlashCommand(action) => {
                ctx.emit(TuiInputViewEvent::AcceptedSlashCommand(action));
            }
            TuiInlineMenuAccepted::Conversation(entry_id) => {
                ctx.emit(TuiInputViewEvent::AcceptedConversation(entry_id));
            }
            TuiInlineMenuAccepted::Model(id) => {
                ctx.emit(TuiInputViewEvent::AcceptedModel(id));
            }
            TuiInlineMenuAccepted::Mcp(action) => {
                ctx.emit(TuiInputViewEvent::AcceptedMcp(action));
            }
            TuiInlineMenuAccepted::PromptAndCommandHistory(row) => {
                ctx.emit(TuiInputViewEvent::AcceptedPromptAndCommandHistory(
                    row.text, row.kind,
                ));
            }
            TuiInlineMenuAccepted::Completion(completion) => {
                ctx.emit(TuiInputViewEvent::AcceptedCompletion(completion));
            }
            TuiInlineMenuAccepted::Profile(profile_id) => {
                ctx.emit(TuiInputViewEvent::AcceptedProfile(profile_id));
            }
            TuiInlineMenuAccepted::Prompt(text) => {
                ctx.emit(TuiInputViewEvent::AcceptedPrompt(text));
            }
            TuiInlineMenuAccepted::Exchange(exchange_id, action) => {
                ctx.emit(TuiInputViewEvent::AcceptedExchange(exchange_id, action));
            }
        }
    }

    fn handle_inline_menu_action(
        &mut self,
        action: &TuiInputAction,
        ctx: &mut ViewContext<Self>,
    ) -> bool {
        if !matches!(
            action,
            TuiInputAction::EditorCommand(TuiEditorCommand::MoveUp | TuiEditorCommand::MoveDown)
                | TuiInputAction::Submit
                | TuiInputAction::HandleEscape
        ) {
            return false;
        }
        let Some(inline_menu) = self.active_inline_menu(ctx) else {
            return false;
        };
        if matches!(action, TuiInputAction::Submit) && !(self.can_accept_inline_menu)(ctx) {
            // The session can render a disabled editor while the shell is still
            // bootstrapping. Consume Enter without accepting a hidden menu item;
            // otherwise the accepted-menu event bypasses the session's normal
            // submission guard and can execute or clear the draft.
            return true;
        }

        match action {
            TuiInputAction::EditorCommand(TuiEditorCommand::MoveUp) => {
                inline_menu.select_previous(ctx);
            }
            TuiInputAction::EditorCommand(TuiEditorCommand::MoveDown) => {
                inline_menu.select_next(ctx);
            }
            TuiInputAction::Submit => {
                if let Some(accepted) = inline_menu.accept(ctx) {
                    self.route_inline_menu_acceptance(accepted, ctx);
                }
            }
            TuiInputAction::HandleEscape => return self.handle_escape(ctx),
            _ => return false,
        }
        ctx.notify();
        true
    }

    /// Handles the input's contextual Escape behavior in explicit priority
    /// order. New input modes should be added after the inline-menu branch so
    /// one Escape always closes the most local surface first.
    fn handle_escape(&mut self, ctx: &mut ViewContext<Self>) -> bool {
        if self.close_read_only_menu(ctx).is_some() {
            return true;
        }
        if let Some(inline_menu) = self.active_inline_menu(ctx) {
            inline_menu.dismiss(ctx);
            ctx.notify();
            return true;
        }

        // In vim mode, Escape transitions between modes (Insert->Normal,
        // Visual/Replace->Normal, Normal->clear pending). This takes priority
        // over shell-mode exit so that `<Esc>` is always a vim command first.
        // Exception: when the FSA is already in Normal mode with no pending
        // input, a second Escape should exit shell mode if active (matching
        // bash/zsh vi-mode behaviour where `<Esc><Esc>` exits shell mode).
        if self.vim_mode_enabled(ctx) {
            let vim_state = self.vim_model.as_ref(ctx).state();
            if matches!(vim_state.mode, VimMode::Normal)
                && vim_state.showcmd.is_empty()
                && self.is_shell_mode(ctx)
            {
                self.exit_shell_mode(ctx);
                return true;
            }
            // Drive the shared FSA via VimModel. VimSubscriber dispatches the
            // resulting VimEvent (Escape or ChangeMode) to our VimHandler impl.
            let escape = Keystroke::parse("escape").expect("escape key is valid");
            let old_mode = vim_state.mode;
            self.vim_model
                .update(ctx, |vim, ctx| vim.keypress(&escape, ctx));
            if self.vim_model.as_ref(ctx).state().mode != old_mode {
                ctx.emit(TuiInputViewEvent::VimModeChanged);
            }
            ctx.notify();
            return true;
        }

        if self.is_shell_mode(ctx) {
            self.exit_shell_mode(ctx);
            return true;
        }
        false
    }

    fn active_inline_menu(&self, ctx: &AppContext) -> Option<TuiInlineMenu> {
        active_inline_menu(
            &self.inline_menus,
            self.suggestions_mode.as_ref(ctx).mode(),
            ctx,
        )
    }

    /// Closes the shared read-only menu (shortcuts/status) if it is open,
    /// returning which kind was closed.
    fn close_read_only_menu(&self, ctx: &mut ViewContext<Self>) -> Option<TuiReadOnlyMenuKind> {
        let mode = self.suggestions_mode.as_ref(ctx).mode();
        let kind = mode.read_only_menu()?;
        self.suggestions_mode.update(ctx, |model, ctx| {
            model.close_if_active(mode, ctx);
        });
        Some(kind)
    }
}

// VimHandler implementation for TuiInputView. Declared as a submodule so it
// can access the private fields of TuiInputView while keeping the main view
// file focused on prompt policy.
#[path = "vim.rs"]
mod vim_impl;

#[cfg(test)]
#[path = "view_tests.rs"]
mod tests;
