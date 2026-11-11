use super::{
    Input, InputAction, InputDropTargetData, InputSuggestionsMode,
    common::{
        add_command_xray_overlay, add_input_suggestions_overlays, add_voltron_overlay,
        add_workflow_info_overlay, wrap_input_with_terminal_padding_and_focus_handler,
    },
    suggestion_chip::ChipLine,
};
use crate::{
    BlocklistAIHistoryModel,
    ai::blocklist::{
        InputType,
        agent_view::{
            AgentViewState, agent_view_bg_fill,
            shortcuts::{AgentShortcutsViewContext, render_agent_shortcuts_view},
        },
    },
    appearance::Appearance,
    context_chips::spacing::{self},
    features::FeatureFlag,
    settings::InputModeSettings,
    terminal::{
        cli_agent_sessions::CLIAgentSessionsModel, settings::TerminalSettings, view::TerminalAction,
    },
    util::key_view_skip,
};
use warp_core::settings::Setting;
use warpui::{
    AppContext, SingletonEntity as _, ViewContext,
    elements::{
        Border, Container, DropTarget, Element, Flex, Hoverable, ParentElement, SavePosition, Stack,
    },
    presenter::ChildView,
};

/// What [`Input::render_agent_input`] builds the agent input from, besides the theme and the
/// settings (their changes re-render it on their own): every fact it branches on. The child views
/// it mounts (editor, footer, status bar, menus, panel) render themselves.
///
/// The agent input used to be rendered again for every edit. One more character in a non-empty
/// buffer changes none of these facts, so that render built the same elements (#1962); an edit
/// now renders the input only when the facts differ from those of its last render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct AgentInputDisplayFacts {
    /// The top border is colored for a non-empty (or locked) shell-mode buffer.
    pub(super) editor_is_empty: bool,
    pub(super) slash_command_detected: bool,
    pub(super) ai_input_enabled: bool,
    pub(super) input_type_is_ai: bool,
    pub(super) input_type_locked: bool,
    /// Which menu, if any, is mounted (the kind; a menu's contents are its own view's).
    pub(super) suggestions_mode: std::mem::Discriminant<InputSuggestionsMode>,
    pub(super) shows_prompt_banner: bool,
    /// Bits of the width of the prompt-suggestion chip on the editor's first line; 0 without a
    /// chip. The chip gets what the typed text leaves free (#2138), so while one shows an edit
    /// that changes the first line's length renders the input again.
    pub(super) prompt_chip_width_bits: u32,
    pub(super) attachment_chips: usize,
    pub(super) shows_workflow_info: bool,
    pub(super) voltron_open: bool,
    pub(super) pane_focused: bool,
    pub(super) active_session: bool,
    pub(super) shows_command_x_ray: bool,
    pub(super) shortcut_view_open: bool,
    pub(super) inline_agent_view: bool,
    pub(super) shows_queued_prompts_panel: bool,
    /// Bits of the pane height, which caps the editor's height.
    pub(super) pane_height_bits: u32,
}

/// Whether an edit needs the agent input rendered: when the facts it would be built from now
/// differ from those its last render was built from, or it has not been rendered as the agent
/// input since another input was shown (`rendered` is `None`). With the #1962 skips off
/// (`skip_enabled` false) every edit renders it, as before.
pub(super) fn agent_input_edit_needs_render(
    rendered: Option<AgentInputDisplayFacts>,
    now: AgentInputDisplayFacts,
    skip_enabled: bool,
) -> bool {
    !skip_enabled || rendered != Some(now)
}

impl Input {
    /// The facts [`Self::render_agent_input`] would be built from now.
    pub(super) fn agent_input_display_facts(&self, app: &AppContext) -> AgentInputDisplayFacts {
        let ai_input_model = self.ai_input_model.as_ref(app);
        AgentInputDisplayFacts {
            editor_is_empty: self.editor.as_ref(app).is_empty(app),
            slash_command_detected: self
                .slash_command_model
                .as_ref(app)
                .state()
                .is_detected_command(),
            ai_input_enabled: ai_input_model.is_ai_input_enabled(),
            input_type_is_ai: matches!(ai_input_model.input_type(), InputType::AI),
            input_type_locked: ai_input_model.is_input_type_locked(),
            suggestions_mode: std::mem::discriminant(
                self.suggestions_mode_model.as_ref(app).mode(),
            ),
            shows_prompt_banner: self
                .prompt_suggestions_banner_state
                .as_ref()
                .is_some_and(|state| !state.should_hide),
            prompt_chip_width_bits: self
                .suggestion_chip_size(ChipLine::default(), app)
                .map_or(0, |size| size.max_width.to_bits()),
            attachment_chips: self.attachment_chips.len(),
            shows_workflow_info: self
                .workflows_state
                .selected_workflow_state
                .as_ref()
                .is_some_and(|state| state.should_show_more_info_view),
            voltron_open: self.is_voltron_open,
            pane_focused: self.is_pane_focused(app),
            active_session: self.is_active_session(app),
            shows_command_x_ray: self.command_x_ray_description.is_some(),
            shortcut_view_open: self
                .agent_shortcut_view_model
                .as_ref(app)
                .is_shortcut_view_open(),
            inline_agent_view: self.agent_view_controller.as_ref(app).is_inline(),
            shows_queued_prompts_panel: self
                .queued_prompts_panel
                .as_ref()
                .is_some_and(|panel| panel.as_ref(app).should_render(app)),
            pane_height_bits: self.size_info(app).pane_height_px().as_f32().to_bits(),
        }
    }

    /// Whether `render` takes the [`Self::render_agent_input`] path (the same conditions).
    pub(super) fn renders_agent_input(&self, app: &AppContext) -> bool {
        !CLIAgentSessionsModel::as_ref(app).is_input_open(self.terminal_view_id)
            && FeatureFlag::AgentView.is_enabled()
            && self.agent_view_controller.as_ref(app).is_active()
    }

    /// Whether an edit made while the agent input is shown needs the input rendered again; see
    /// [`agent_input_edit_needs_render`]. Any other input (the CLI-agent input) is rendered for
    /// every edit, as before.
    pub(super) fn agent_view_edit_needs_render(&self, app: &AppContext) -> bool {
        !self.renders_agent_input(app)
            || agent_input_edit_needs_render(
                self.agent_input_rendered_facts.get(),
                self.agent_input_display_facts(app),
                key_view_skip::enabled(),
            )
    }

    /// Renders the agent input again if a fact it is built from changed outside an edit's own
    /// handler: the slash-command model and the queued-prompts panel settle after the edit that
    /// moved them, and a replaced buffer is not an edit. Until #1962 the re-render on every edit
    /// covered these.
    pub(super) fn rerender_agent_input_if_facts_changed(&mut self, ctx: &mut ViewContext<Self>) {
        if self.renders_agent_input(ctx)
            && self.agent_input_rendered_facts.get() != Some(self.agent_input_display_facts(ctx))
        {
            warpui::input_order::log_notify_cause(
                "Input",
                "agent::rerender_agent_input_if_facts_changed",
            );
            ctx.notify();
        }
    }

    /// Renders the input when there is an active `AgentView`.
    ///
    /// Only used when `FeatureFlag::AgentView` is enabled.
    pub(super) fn render_agent_input(&self, app: &AppContext) -> Box<dyn Element> {
        // #1962: remember what this render is built from.
        self.agent_input_rendered_facts
            .set(Some(self.agent_input_display_facts(app)));
        let appearance = Appearance::as_ref(app);
        let menu_positioning = self.menu_positioning(app);

        let _model = self.model.lock();

        // We should likely rework this stack to not need to use `with_constrain_absolute_children`,
        // by reworking the positioning of the children to not depend on this.
        let mut stack = Stack::new().with_constrain_absolute_children();

        let input_mode = *InputModeSettings::as_ref(app).input_mode.value();

        let mut column = Flex::column();

        // #2138: the editor's first line holds the buffer only; a prompt suggestion is a chip on it.
        let chip_line = ChipLine::default();

        if let Some(banner) = self.render_input_banner(
            appearance, app, input_mode, /*is_compact_mode=*/ false, chip_line,
        ) {
            column.add_child(
                Container::new(banner)
                    .with_margin_top(spacing::UDI_CHIP_MARGIN)
                    .finish(),
            );
        }

        let ai_input_model = self.ai_input_model.as_ref(app);

        if FeatureFlag::ImageAsContext.is_enabled()
            && matches!(ai_input_model.input_type(), InputType::AI)
        {
            if let Some(images) = self.render_attachment_chips(appearance) {
                column.add_child(
                    Container::new(images)
                        .with_margin_top(spacing::UDI_CHIP_MARGIN)
                        .finish(),
                );
            }
        }

        let terminal_spacing = TerminalSettings::as_ref(app)
            .terminal_input_spacing(appearance.line_height_ratio(), app);
        column.add_child(
            Container::new(
                self.render_input_box(/*show_vim_status=*/ false, chip_line, appearance, app),
            )
            .with_margin_top(
                terminal_spacing.prompt_to_editor_padding
                    * spacing::UDI_PROMPT_BOTTOM_PADDING_FACTOR,
            )
            .finish(),
        );
        column.add_child(
            SavePosition::new(
                ChildView::new(&self.agent_input_footer).finish(),
                &self.prompt_save_position_id(),
            )
            .finish(),
        );

        stack.add_child(wrap_input_with_terminal_padding_and_focus_handler(
            self.is_active_session(app),
            column.finish(),
            false,
        ));

        if let Some(selected_workflow_state) = self.workflows_state.selected_workflow_state.as_ref()
        {
            if selected_workflow_state.should_show_more_info_view {
                add_workflow_info_overlay(
                    &mut stack,
                    selected_workflow_state,
                    self.size_info(app).pane_height_px().as_f32(),
                    menu_positioning,
                );
            }
        }

        if self.is_voltron_open && self.is_pane_focused(app) {
            add_voltron_overlay(&mut stack, &self.voltron_view, menu_positioning);
        }

        if self.is_pane_focused(app) {
            add_input_suggestions_overlays(self, &mut stack, appearance, menu_positioning, app);
        }

        if let Some(token_description) = &self.command_x_ray_description {
            add_command_xray_overlay(
                self,
                &mut stack,
                token_description,
                appearance,
                menu_positioning,
                app,
            );
        }

        let drop_target = DropTarget::new(
            SavePosition::new(stack.finish(), &self.status_free_input_save_position_id()).finish(),
            InputDropTargetData::new(self.weak_view_handle.clone()),
        )
        .finish();

        let border_color = if !self.ai_input_model.as_ref(app).is_ai_input_enabled()
            && !self.suggestions_mode_model.as_ref(app).is_slash_commands()
            && !self.slash_command_model.as_ref(app).state().is_detected_command()
            // If NLD, don't color the border if the input is empty, because the current
            // classification is necessarily stale (intentionally inherited from the last
            // classification prior to clearing the input)
            && (!self.editor.as_ref(app).is_empty(app)
                || self.ai_input_model.as_ref(app).is_input_type_locked())
        {
            appearance.theme().ansi_fg_blue()
        } else {
            styles::default_border_color(appearance.theme())
        };

        let mut input = Container::new(
            Hoverable::new(self.hoverable_handle.clone(), |_| drop_target)
                .on_hover(|is_hovered, ctx, _app, _position| {
                    ctx.dispatch_typed_action(InputAction::SetUDIHovered(is_hovered));
                })
                .on_middle_click(|ctx, _app, _position| {
                    ctx.dispatch_typed_action(TerminalAction::MiddleClickOnInput)
                })
                .finish(),
        )
        .with_border(Border::top(1.).with_border_color(border_color))
        .with_padding_bottom(4.);

        if self.agent_view_controller.as_ref(app).is_inline() {
            input = input.with_background(agent_view_bg_fill(app));
        }

        let input = input.finish();

        let mut column = Flex::column();

        if self
            .suggestions_mode_model
            .as_ref(app)
            .is_inline_model_selector()
        {
            column.add_child(ChildView::new(&self.inline_model_selector_view).finish());
        } else if FeatureFlag::InlineProfileSelector.is_enabled()
            && self
                .suggestions_mode_model
                .as_ref(app)
                .is_profile_selector()
        {
            column.add_child(ChildView::new(&self.inline_profile_selector_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_slash_commands() {
            column.add_child(ChildView::new(&self.inline_slash_commands_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_prompts_menu() {
            column.add_child(ChildView::new(&self.inline_prompts_menu_view).finish());
        } else if self
            .suggestions_mode_model
            .as_ref(app)
            .is_conversation_menu()
        {
            column.add_child(ChildView::new(&self.inline_conversation_menu_view).finish());
        } else if FeatureFlag::ListSkills.is_enabled()
            && self.suggestions_mode_model.as_ref(app).is_skill_menu()
        {
            column.add_child(ChildView::new(&self.inline_skill_selector_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_user_query_menu() {
            column.add_child(ChildView::new(&self.user_query_menu_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_rewind_menu() {
            column.add_child(ChildView::new(&self.rewind_menu_view).finish());
        } else if self
            .suggestions_mode_model
            .as_ref(app)
            .is_inline_history_menu()
        {
            column.add_child(ChildView::new(&self.inline_history_menu_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_repos_menu() {
            column.add_child(ChildView::new(&self.inline_repos_menu_view).finish());
        } else if self.suggestions_mode_model.as_ref(app).is_plan_menu() {
            column.add_child(ChildView::new(&self.inline_plan_menu_view).finish());
        }

        if self
            .agent_shortcut_view_model
            .as_ref(app)
            .is_shortcut_view_open()
        {
            let agent_view_controller = self.agent_view_controller.as_ref(app);
            let (is_ambient_agent, has_submitted_first_prompt) =
                match agent_view_controller.agent_view_state() {
                    AgentViewState::Active {
                        conversation_id,
                        origin,
                        ..
                    } => {
                        let is_ambient_agent = origin.is_ambient_agent();
                        let has_submitted_first_prompt = if is_ambient_agent {
                            BlocklistAIHistoryModel::as_ref(app)
                                .conversation(conversation_id)
                                .is_some_and(|c| c.initial_user_query().is_some())
                        } else {
                            true
                        };
                        (is_ambient_agent, has_submitted_first_prompt)
                    }
                    // When inactive, show all shortcuts (treat as not-cloud and not in the zero-state).
                    AgentViewState::Inactive => (false, true),
                };

            column.add_child(render_agent_shortcuts_view(
                AgentShortcutsViewContext {
                    is_ambient_agent,
                    has_submitted_first_prompt,
                },
                app,
            ));
        }
        column.add_child(ChildView::new(&self.agent_status_view).finish());
        // The multi-prompt queued-prompts panel renders between the warping indicator and the
        // input editor, and only when it has something to show for the active conversation.
        if let Some(panel) = self.queued_prompts_panel.as_ref()
            && panel.as_ref(app).should_render(app)
        {
            column.add_child(ChildView::new(panel).finish());
        }
        column.add_child(input);

        let mut outer_stack = Stack::new().with_constrain_absolute_children();
        outer_stack.add_child(column.finish());

        SavePosition::new(outer_stack.finish(), &self.save_position_id()).finish()
    }
}

pub mod styles {
    use pathfinder_color::ColorU;
    use warp_core::ui::theme::WarpTheme;

    use crate::ui_components::blended_colors;

    pub fn default_border_color(theme: &WarpTheme) -> ColorU {
        blended_colors::neutral_2(theme)
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
