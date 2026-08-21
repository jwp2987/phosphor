use crate::ai::agent::conversation::AIConversationId;
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::ai::blocklist::agent_view::avatar_disc::{
    render_agent_avatar_disc, render_orchestrator_avatar_disc,
};
use crate::ai::blocklist::usage::render_context_window_usage_icon;
use crate::ai::blocklist::usage::rollup::{
    AgentAvatar, OrchestrationCreditRollup, PerAgentCreditEntry, compute_orchestration_rollup,
    orchestration_headline_credits,
};
use crate::ai::blocklist::view_util::format_credits;
use crate::appearance::Appearance;
use crate::persistence::model::{
    FULL_TERMINAL_USE_CATEGORY, ModelTokenUsage, PRIMARY_AGENT_CATEGORY,
    token_usage_category_display_name,
};
use crate::ui_components::blended_colors;
use std::cmp::Ordering;
use std::collections::HashMap;
use warp_core::ui::Icon;
use warp_core::ui::theme::color::internal_colors;
use warpui::elements::ConstrainedBox;
use warpui::platform::Cursor;
use warpui::text_layout::ClipConfig;
use warpui::{
    AppContext, Element, Entity, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        Border, Container, CornerRadius, CrossAxisAlignment, Empty, Flex, Hoverable, MainAxisSize,
        MouseStateHandle, ParentElement, Radius, Text,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayMode {
    Settings,
    Footer,
}

/// Typed actions dispatched by widgets inside [`ConversationUsageView`]: the
/// "View details" / "Hide details" toggle and the "Show N more" affordance
/// for the orchestration credit rollup's per-agent breakdown. Ported from
/// the pin's `ConversationUsageViewAction` (`02b53fcd8`), minus
/// `ToggleContextWindowExpanded` — the per-segment context-window breakdown
/// it drives is a separate, unrelated feature this fork doesn't have.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConversationUsageViewAction {
    /// Flip the "View details" / "Hide details" toggle.
    ToggleDetailsExpanded,
    /// Reveal the truncated rows beyond the first
    /// [`PER_AGENT_BREAKDOWN_TRUNCATION_CAP`] in the per-agent breakdown.
    ShowAllAgentRows,
}

pub struct ConversationUsageInfo {
    pub credits_spent: f32,
    // Credits spent over the last block, where the block comprises
    // all agent outputs since the most recent user input.
    pub credits_spent_for_last_block: Option<f32>,
    pub tool_calls: i32,
    pub models: Vec<ModelTokenUsage>,
    pub context_window_usage: f32,
    pub files_changed: i32,
    pub lines_added: i32,
    pub lines_removed: i32,
    pub commands_executed: i32,
}

/// Timing information for the last set of agent responses
/// (all blocks since the last user input, as this is the granularity
/// at which we show the usage footer)
pub struct TimingInfo {
    /// Time to first token for the last block (in milliseconds)
    pub time_to_first_token_ms: i64,
    /// Total response time for the last block (in milliseconds)
    pub total_agent_response_time_ms: i64,
    /// Wall-to-wall response time (in milliseconds) from sending the user query
    /// to the last token in the last set of responses.
    pub wall_to_wall_response_time_ms: Option<i64>,
}

/// View to hold a conversation usage info block.
/// This is used for both the usage footer and the usage history page in settings.
pub struct ConversationUsageView {
    pub usage_info: ConversationUsageInfo,
    /// The display mode for this view.
    pub display_mode: DisplayMode,
    /// Optional timing information for the last set of responses (only shown in the footer version of this view).
    pub timing_info: Option<TimingInfo>,
    full_terminal_use_tooltip_mouse_state: MouseStateHandle,
    /// Orchestration credit rollup context. When `Some`, the parent
    /// conversation may be an orchestrator with locally-loaded descendants;
    /// the rollup itself is recomputed at render time from
    /// `parent_conversation_id` (via [`Self::rollup`]) so descendant credit
    /// updates are picked up on the view's next render. `None` for views
    /// built with [`Self::new`] (settings-mode / no-rollup callers), so the
    /// "View details" toggle simply never renders for them — matching
    /// today's behavior exactly.
    parent_conversation_id: Option<AIConversationId>,
    /// Local UI state: whether the "View details" toggle is currently
    /// expanded. Resets to `false` whenever this rich-content view is
    /// dropped and recreated (it is not persisted).
    details_expanded: bool,
    /// Local UI state: whether the user clicked "Show N more" to reveal the
    /// rows beyond the first [`PER_AGENT_BREAKDOWN_TRUNCATION_CAP`]. Resets
    /// on view rebuild for the same reason as `details_expanded`.
    show_all_clicked: bool,
    /// Mouse state for the "View details" / "Hide details" toggle link.
    details_toggle_mouse_state: MouseStateHandle,
    /// Mouse state for the "Show N more" link.
    show_more_mouse_state: MouseStateHandle,
}

impl ConversationUsageView {
    pub fn new(
        usage_info: ConversationUsageInfo,
        display_mode: DisplayMode,
        timing_info: Option<TimingInfo>,
        full_terminal_use_tooltip_mouse_state: MouseStateHandle,
    ) -> Self {
        Self {
            usage_info,
            display_mode,
            timing_info,
            full_terminal_use_tooltip_mouse_state,
            parent_conversation_id: None,
            details_expanded: false,
            show_all_clicked: false,
            details_toggle_mouse_state: MouseStateHandle::default(),
            show_more_mouse_state: MouseStateHandle::default(),
        }
    }

    /// Constructs the view in `DisplayMode::Footer` with the orchestration
    /// credit rollup wired in. `parent_conversation_id` is the conversation
    /// this usage footer belongs to; if it turns out to have locally-loaded
    /// descendants with spent credits, the footer grows a "View details"
    /// toggle over the per-agent breakdown (see [`Self::rollup`]).
    pub fn new_footer_with_rollup(
        usage_info: ConversationUsageInfo,
        timing_info: Option<TimingInfo>,
        full_terminal_use_tooltip_mouse_state: MouseStateHandle,
        parent_conversation_id: AIConversationId,
    ) -> Self {
        Self {
            usage_info,
            display_mode: DisplayMode::Footer,
            timing_info,
            full_terminal_use_tooltip_mouse_state,
            parent_conversation_id: Some(parent_conversation_id),
            details_expanded: false,
            show_all_clicked: false,
            details_toggle_mouse_state: MouseStateHandle::default(),
            show_more_mouse_state: MouseStateHandle::default(),
        }
    }

    /// Returns the current orchestration credit rollup for this view, or
    /// `None` when the view isn't a footer view, the parent conversation
    /// isn't known, or the parent has no locally-loaded descendants with
    /// non-zero credits. Self-gating: a conversation with no descendants
    /// (the common case) short-circuits before any rollup-specific UI is
    /// built, so no feature flag is needed.
    fn rollup(&self, app: &AppContext) -> Option<OrchestrationCreditRollup> {
        if self.display_mode != DisplayMode::Footer {
            return None;
        }
        let parent_id = self.parent_conversation_id?;
        let history = BlocklistAIHistoryModel::as_ref(app);
        compute_orchestration_rollup(parent_id, history)
    }

    /// The number shown against the "Credits spent (total)" label.
    ///
    /// When an orchestration rollup applies this is the rollup total --
    /// the orchestrator plus every locally-loaded descendant -- so the
    /// headline agrees with the per-agent drill-down rendered immediately
    /// beneath it by [`Self::append_per_agent_rows`]. Showing
    /// `usage_info.credits_spent` here instead would make the headline
    /// smaller than the list under it whenever a child agent has spent
    /// anything, which is what the pin avoids by computing the same value
    /// (`42effe840:app/src/ai/blocklist/usage/conversation_usage_view.rs:329-332`).
    ///
    /// Without a rollup (no descendants, or nobody has spent anything) it
    /// falls back to this conversation's own spend, which is then also the
    /// whole tree's spend. The "Credits spent (last response)" row above it
    /// stays bound to the orchestrator's own last block, matching the pin.
    ///
    /// Both limbs are read **live** from the history model, via the same
    /// [`orchestration_headline_credits`] the collapsed pill calls, so the two
    /// surfaces cannot disagree. `self.usage_info.credits_spent` is not usable
    /// for the fallback: `terminal/view.rs::handle_usage_footer_toggled`
    /// snapshots it when the footer is *opened*, so a conversation that spent
    /// more while the footer stayed open showed the frozen number here and the
    /// live one in the pill directly above it — two totals for one
    /// conversation on screen at the same time. The snapshot survives only as
    /// the last resort for `DisplayMode::Settings` views (built by
    /// [`Self::new`], no `parent_conversation_id`), where `usage_info` is
    /// historical data rather than a live conversation and is authoritative.
    fn headline_total_credits(
        &self,
        app: &AppContext,
        rollup: Option<&OrchestrationCreditRollup>,
    ) -> f32 {
        self.parent_conversation_id
            .and_then(|parent_id| {
                orchestration_headline_credits(
                    parent_id,
                    BlocklistAIHistoryModel::as_ref(app),
                    rollup,
                )
            })
            .unwrap_or(self.usage_info.credits_spent)
    }

    /// Helper to collect models grouped by category.
    /// Returns a HashMap mapping category name to list of (model_id, is_byok) tuples.
    /// Custom-endpoint rows share the `is_byok` external-key icon bucket with BYOK
    /// rows, since both represent the user's own credentials rather than Zap's.
    /// Handles both category-based fields and legacy warp_tokens/byok_tokens/
    /// custom_endpoint_tokens fields.
    fn collect_models_by_category(&self) -> HashMap<String, Vec<(String, bool)>> {
        let mut entries_by_category: HashMap<String, Vec<(String, bool)>> = HashMap::new();

        // Collect from category-based fields
        for model in &self.usage_info.models {
            for (category, &tokens) in &model.warp_token_usage_by_category {
                if tokens > 0 {
                    entries_by_category
                        .entry(category.clone())
                        .or_default()
                        .push((model.model_id.clone(), false));
                }
            }
            for (category, &tokens) in &model.byok_token_usage_by_category {
                if tokens > 0 {
                    entries_by_category
                        .entry(category.clone())
                        .or_default()
                        .push((model.model_id.clone(), true));
                }
            }
            for (category, &tokens) in &model.custom_endpoint_token_usage_by_category {
                if tokens > 0 {
                    entries_by_category
                        .entry(category.clone())
                        .or_default()
                        .push((model.model_id.clone(), true));
                }
            }
        }

        // Fallback to legacy fields for backwards compatibility
        if entries_by_category.is_empty() {
            for model in &self.usage_info.models {
                if model.warp_tokens > 0 {
                    entries_by_category
                        .entry(PRIMARY_AGENT_CATEGORY.to_string())
                        .or_default()
                        .push((model.model_id.clone(), false));
                }
                if model.byok_tokens > 0 {
                    entries_by_category
                        .entry(PRIMARY_AGENT_CATEGORY.to_string())
                        .or_default()
                        .push((model.model_id.clone(), true));
                }
                if model.custom_endpoint_tokens > 0 {
                    entries_by_category
                        .entry(PRIMARY_AGENT_CATEGORY.to_string())
                        .or_default()
                        .push((model.model_id.clone(), true));
                }
            }
        }

        entries_by_category
    }

    fn render_unified_layout(&self, app: &AppContext, appearance: &Appearance) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font_size = appearance.ui_font_subheading();
        let text_color = blended_colors::text_main(theme, theme.surface_2());

        let rollup = self.rollup(app);
        let total_credits_value = self.headline_total_credits(app, rollup.as_ref());

        let mut labels: Vec<Box<dyn Element>> = vec![];
        let mut values: Vec<Box<dyn Element>> = vec![];

        // Usage summary
        labels.push(render_section_header(
            "USAGE SUMMARY".to_string(),
            appearance,
        ));
        values.push(render_section_header("".to_string(), appearance));

        if self.display_mode == DisplayMode::Footer
            && self.usage_info.credits_spent_for_last_block.is_some()
        {
            let last_block_credits = self.usage_info.credits_spent_for_last_block.unwrap();
            labels.push(render_label_text(
                "Credits spent (last response)",
                appearance,
            ));
            values.push(render_value_text(
                format_credits(last_block_credits),
                appearance,
            ));

            labels.push(render_label_text("Credits spent (total)", appearance));
            values.push(self.render_total_credits_value_row(
                total_credits_value,
                rollup.as_ref(),
                appearance,
            ));
        } else {
            labels.push(render_label_text("Credits spent", appearance));
            values.push(self.render_total_credits_value_row(
                total_credits_value,
                rollup.as_ref(),
                appearance,
            ));
        }

        // Per-agent breakdown rows render immediately beneath the credits
        // row so they read as a drill-down of that value, not as a separate
        // section appended elsewhere in the card.
        self.append_per_agent_rows(&mut labels, &mut values, rollup.as_ref(), appearance);

        labels.push(render_label_text("Tool calls", appearance));
        values.push(render_value_text(
            format_value_text(self.usage_info.tool_calls, "call"),
            appearance,
        ));

        let entries_by_category = self.collect_models_by_category();
        let mut categories: Vec<_> = entries_by_category.keys().cloned().collect();
        categories.sort_by(|a, b| match (a.as_str(), b.as_str()) {
            (PRIMARY_AGENT_CATEGORY, _) => Ordering::Less,
            (_, PRIMARY_AGENT_CATEGORY) => Ordering::Greater,
            _ => a.cmp(b),
        });

        for category in categories {
            let models = entries_by_category.get(&category).unwrap();
            if models.is_empty() {
                break;
            }

            let label_text = if category == PRIMARY_AGENT_CATEGORY && entries_by_category.len() == 1
            {
                "Models".to_string()
            } else {
                format!("Models ({})", token_usage_category_display_name(&category))
            };

            // For FULL_TERMINAL_USE_CATEGORY, add an info icon with tooltip
            if category == FULL_TERMINAL_USE_CATEGORY {
                let label_element = render_label_text(&label_text, appearance);

                let hoverable_icon = appearance
                    .ui_builder()
                    .info_button_with_tooltip(
                        font_size * 0.85,
                        "You can change which model is used for full terminal use in the AI settings page",
                        self.full_terminal_use_tooltip_mouse_state.clone(),
                    )
                    .finish();

                labels.push(
                    Flex::row()
                        .with_cross_axis_alignment(CrossAxisAlignment::Center)
                        .with_child(label_element)
                        .with_child(Container::new(hoverable_icon).with_margin_left(4.).finish())
                        .finish(),
                );
            } else {
                labels.push(render_label_text(&label_text, appearance));
            }

            // Build comma-separated list of models, with BYOK indicator using Icon::Key
            let mut model_elements: Vec<Box<dyn Element>> = vec![];
            let mut sorted_models: Vec<_> = models.iter().collect();
            sorted_models.sort_by(|a, b| a.0.cmp(&b.0));

            for (i, (model_id, is_byok)) in sorted_models.iter().enumerate() {
                if i > 0 {
                    model_elements.push(
                        Text::new(", ".to_string(), appearance.ui_font_family(), font_size)
                            .with_color(text_color)
                            .finish(),
                    );
                }

                if *is_byok {
                    model_elements.push(
                        ConstrainedBox::new(Icon::Key.to_warpui_icon(text_color.into()).finish())
                            .with_width(font_size)
                            .with_height(font_size)
                            .finish(),
                    );
                    model_elements.push(
                        Container::new(
                            Text::new((*model_id).clone(), appearance.ui_font_family(), font_size)
                                .with_color(text_color)
                                .finish(),
                        )
                        .with_margin_left(4.)
                        .finish(),
                    );
                } else {
                    model_elements.push(
                        Text::new((*model_id).clone(), appearance.ui_font_family(), font_size)
                            .with_color(text_color)
                            .finish(),
                    );
                }
            }

            values.push(
                Flex::row()
                    .with_cross_axis_alignment(CrossAxisAlignment::Center)
                    .with_children(model_elements)
                    .finish(),
            );
        }

        labels.push(render_label_text("Context window used", appearance));
        let context_usage_str =
            format!("{}%", (self.usage_info.context_window_usage * 100.).round());
        let context_window_element = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_spacing(4.)
            .with_child(
                Text::new(context_usage_str, appearance.ui_font_family(), font_size)
                    .with_color(text_color)
                    .finish(),
            )
            .with_child(
                ConstrainedBox::new(render_context_window_usage_icon(
                    self.usage_info.context_window_usage,
                    theme,
                    None,
                ))
                .with_width(font_size)
                .with_height(font_size)
                .finish(),
            )
            .finish();
        values.push(context_window_element);

        // Space between sections
        labels.push(
            Container::new(Empty::new().finish())
                .with_margin_top(12.)
                .finish(),
        );
        values.push(
            Container::new(Empty::new().finish())
                .with_margin_top(12.)
                .finish(),
        );

        // Tool call summary
        labels.push(render_section_header(
            "TOOL CALL SUMMARY".to_string(),
            appearance,
        ));
        values.push(render_section_header("".to_string(), appearance));

        labels.push(render_label_text("Files changed", appearance));
        values.push(render_value_text(
            format_value_text(self.usage_info.files_changed, "file"),
            appearance,
        ));

        labels.push(render_label_text("Diffs applied", appearance));
        let diffs_element = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(
                Text::new(
                    format!("+ {}", self.usage_info.lines_added),
                    appearance.ui_font_family(),
                    font_size,
                )
                .with_color(theme.ansi_fg_green())
                .finish(),
            )
            .with_child(
                Container::new(
                    ConstrainedBox::new(Empty::new().finish())
                        .with_width(4.)
                        .with_height(4.)
                        .finish(),
                )
                .with_corner_radius(CornerRadius::with_all(Radius::Percentage(50.)))
                .with_background(internal_colors::neutral_6(theme))
                .with_margin_left(8.)
                .with_margin_right(8.)
                .finish(),
            )
            .with_child(
                Text::new(
                    format!("- {}", self.usage_info.lines_removed),
                    appearance.ui_font_family(),
                    font_size,
                )
                .with_color(theme.ansi_fg_red())
                .finish(),
            )
            .finish();
        values.push(diffs_element);

        labels.push(render_label_text("Commands executed", appearance));
        values.push(render_value_text(
            format_value_text(self.usage_info.commands_executed, "command"),
            appearance,
        ));

        // Last response time
        if self.display_mode == DisplayMode::Footer {
            if let Some(timing) = &self.timing_info {
                if timing.time_to_first_token_ms != 0
                    || timing.total_agent_response_time_ms != 0
                    || timing.wall_to_wall_response_time_ms.is_some()
                {
                    // Space between sections
                    labels.push(
                        Container::new(Empty::new().finish())
                            .with_margin_top(12.)
                            .finish(),
                    );
                    values.push(
                        Container::new(Empty::new().finish())
                            .with_margin_top(12.)
                            .finish(),
                    );

                    // Section header
                    labels.push(render_section_header(
                        "LAST RESPONSE TIME".to_string(),
                        appearance,
                    ));
                    values.push(render_section_header("".to_string(), appearance));

                    labels.push(render_label_text("Time to first token", appearance));
                    values.push(render_value_text(
                        format!(
                            "{:.1} seconds",
                            timing.time_to_first_token_ms as f64 / 1000.0
                        ),
                        appearance,
                    ));

                    labels.push(render_label_text("Total agent response time", appearance));
                    values.push(render_value_text(
                        format!(
                            "{:.1} seconds",
                            timing.total_agent_response_time_ms as f64 / 1000.0
                        ),
                        appearance,
                    ));

                    if let Some(wall_ms) = timing.wall_to_wall_response_time_ms {
                        if wall_ms != 0 {
                            labels.push(render_label_text(
                                "Total time (including tool calls)",
                                appearance,
                            ));
                            values.push(render_value_text(
                                format!("{:.1} seconds", wall_ms as f64 / 1000.0),
                                appearance,
                            ));
                        }
                    }
                }
            }
        }

        Container::new(
            Flex::row()
                .with_spacing(8.)
                .with_child(Flex::column().with_children(labels).finish())
                .with_child(Flex::column().with_children(values).finish())
                .finish(),
        )
        .with_uniform_margin(16.)
        .finish()
    }

    /// Renders the "Credits spent (total)" value cell. When an
    /// orchestration credit rollup applies, the cell is a row with the
    /// value followed by a "View details" / "Hide details" toggle;
    /// otherwise it's just the value, unchanged from before this toggle
    /// existed.
    fn render_total_credits_value_row(
        &self,
        total_credits: f32,
        rollup: Option<&OrchestrationCreditRollup>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let value_text = render_value_text(format_credits(total_credits), appearance);
        if rollup.is_none() {
            return value_text;
        }

        let toggle = render_toggle_link(
            self.details_toggle_mouse_state.clone(),
            self.details_expanded,
            "Hide details",
            "View details",
            ConversationUsageViewAction::ToggleDetailsExpanded,
            appearance,
        );
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min)
            .with_spacing(8.)
            .with_child(value_text)
            .with_child(toggle)
            .finish()
    }

    /// Pushes the per-agent credit breakdown rows (and, if the list is
    /// larger than the truncation cap and not yet fully expanded, a
    /// "Show N more" link) into the two-column layout when the rollup is
    /// active and the user has expanded the details. Pushed in two-column
    /// (label, value) pairs so they slot into the existing flex layout.
    fn append_per_agent_rows(
        &self,
        labels: &mut Vec<Box<dyn Element>>,
        values: &mut Vec<Box<dyn Element>>,
        rollup: Option<&OrchestrationCreditRollup>,
        appearance: &Appearance,
    ) {
        let Some(rollup) = rollup else {
            return;
        };
        if !self.details_expanded {
            return;
        }
        let total_entries = rollup.per_agent.len();
        let shown_entries: usize =
            if total_entries > PER_AGENT_BREAKDOWN_TRUNCATION_CAP && !self.show_all_clicked {
                PER_AGENT_BREAKDOWN_TRUNCATION_CAP
            } else {
                total_entries
            };
        for entry in rollup.per_agent.iter().take(shown_entries) {
            let (label_el, value_el) = self.render_per_agent_row(entry, appearance);
            labels.push(label_el);
            values.push(value_el);
        }
        if total_entries > shown_entries {
            let hidden_count = total_entries - shown_entries;
            // "Show N more" sits on a row of its own. Push a value-side
            // placeholder that mirrors the link's natural line height so the
            // right column stays in lock-step with the left and the
            // subsequent "Tool calls" / value row pair doesn't slip out of
            // alignment.
            labels.push(self.render_show_more_link(hidden_count, appearance));
            values.push(render_value_text_placeholder(appearance));
        }
    }

    /// Renders the avatar + label cell for a per-agent breakdown row, plus
    /// the credit value cell, returned as a `(label, value)` pair so the
    /// caller can append them to the existing two-column flex layout.
    fn render_per_agent_row(
        &self,
        entry: &PerAgentCreditEntry,
        appearance: &Appearance,
    ) -> (Box<dyn Element>, Box<dyn Element>) {
        let theme = appearance.theme();
        let bg = theme.surface_2();
        let font_size = appearance.ui_font_subheading();
        const ROW_AVATAR_SIZE: f32 = 16.;
        let avatar = match entry.avatar {
            AgentAvatar::Orchestrator => {
                render_orchestrator_avatar_disc(ROW_AVATAR_SIZE, theme, appearance)
            }
            AgentAvatar::Child => {
                render_agent_avatar_disc(&entry.display_name, ROW_AVATAR_SIZE, theme, appearance)
            }
        };
        let name_text = Text::new(
            entry.display_name.clone(),
            appearance.ui_font_family(),
            font_size,
        )
        .with_color(blended_colors::text_disabled(theme, bg))
        .soft_wrap(false)
        .with_clip(ClipConfig::ellipsis())
        .finish();
        let name_element = ConstrainedBox::new(name_text)
            .with_max_width(PER_AGENT_LABEL_MAX_WIDTH)
            .finish();
        let label = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min)
            .with_spacing(8.)
            .with_child(avatar)
            .with_child(name_element)
            .finish();
        let value = Text::new(
            format_credits(entry.credits_spent),
            appearance.ui_font_family(),
            font_size,
        )
        .with_color(blended_colors::text_sub(theme, bg))
        .finish();
        (label, value)
    }

    /// Renders the "Show N more" link row shown beneath the first
    /// [`PER_AGENT_BREAKDOWN_TRUNCATION_CAP`] per-agent rows when the
    /// breakdown has more entries than that. Clicking the link replaces the
    /// truncated list with the full list on the next render. Uses the same
    /// hyperlink-blue color as the "View details" toggle so the two
    /// affordances visually match.
    fn render_show_more_link(
        &self,
        hidden_count: usize,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let font_size = appearance.ui_font_subheading();
        let link_color = theme.ansi_fg_blue();
        let label = format!("Show {hidden_count} more");
        Hoverable::new(self.show_more_mouse_state.clone(), move |_hover_state| {
            Text::new(label.clone(), appearance.ui_font_family(), font_size)
                .with_color(link_color)
                .with_selectable(false)
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(|ctx, _, _| {
            ctx.dispatch_typed_action(ConversationUsageViewAction::ShowAllAgentRows);
        })
        .finish()
    }

    /// Render the card container with display mode-specific styling.
    fn render_card_container(
        &self,
        content: Box<dyn Element>,
        appearance: &Appearance,
    ) -> Box<dyn Element> {
        let theme = appearance.theme();
        let mut card_container = Container::new(content).with_background(theme.surface_2());

        if let DisplayMode::Footer = self.display_mode {
            card_container = card_container
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(8.)))
                .with_border(Border::all(1.0).with_border_fill(theme.outline()))
                .with_uniform_margin(16.);
        } else {
            card_container =
                card_container.with_corner_radius(CornerRadius::with_bottom(Radius::Pixels(6.)));
        }

        let mut res = Flex::column()
            .with_main_axis_size(MainAxisSize::Min)
            .with_cross_axis_alignment(CrossAxisAlignment::Stretch);

        if let DisplayMode::Footer = self.display_mode {
            res = res.with_child(
                // Top divider
                Container::new(Empty::new().finish())
                    .with_border(Border::top(2.0).with_border_fill(theme.outline()))
                    .with_overdraw_bottom(0.)
                    .finish(),
            );
        }

        res.with_child(card_container.finish()).finish()
    }
}

impl View for ConversationUsageView {
    fn ui_name() -> &'static str {
        "ConversationUsageView"
    }

    fn render(&self, app: &AppContext) -> Box<dyn Element> {
        let appearance = Appearance::as_ref(app);

        self.render_card_container(self.render_unified_layout(app, appearance), appearance)
    }
}

impl Entity for ConversationUsageView {
    type Event = ();
}

impl TypedActionView for ConversationUsageView {
    type Action = ConversationUsageViewAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            ConversationUsageViewAction::ToggleDetailsExpanded => {
                self.details_expanded = !self.details_expanded;
                // Collapsing the breakdown resets the "Show N more"
                // expansion so the user lands back on the truncated list
                // the next time they expand.
                if !self.details_expanded {
                    self.show_all_clicked = false;
                }
                ctx.notify();
            }
            ConversationUsageViewAction::ShowAllAgentRows => {
                self.show_all_clicked = true;
                ctx.notify();
            }
        }
    }
}

/// Render the main header for a usage section.
fn render_section_header(header_label: String, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let background = theme.surface_2();

    Container::new(
        Text::new(
            header_label,
            appearance.overline_font_family(),
            appearance.overline_font_size(),
        )
        .with_color(blended_colors::text_disabled(theme, background))
        .finish(),
    )
    .with_margin_bottom(4.)
    .finish()
}

/// Format a value and a label into one usage string,
/// making the label plural if the value is not 1.
fn format_value_text(value: i32, label: &str) -> String {
    format!("{} {}{}", value, label, if value == 1 { "" } else { "s" })
}

/// Helper to build a text element with consistent styling for labels.
fn render_label_text(text: &str, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let font_size = appearance.ui_font_subheading();

    Text::new(text.to_string(), appearance.ui_font_family(), font_size)
        .with_color(blended_colors::text_sub(theme, theme.surface_2()))
        .finish()
}

/// Helper to build a text element with consistent styling for values.
fn render_value_text(text: String, appearance: &Appearance) -> Box<dyn Element> {
    let theme = appearance.theme();
    let font_size = appearance.ui_font_subheading();
    let text_color = blended_colors::text_main(theme, theme.surface_2());

    Text::new(text, appearance.ui_font_family(), font_size)
        .with_color(text_color)
        .finish()
}

/// Renders a hyperlink-styled expand/collapse toggle with a chevron.
fn render_toggle_link(
    mouse_state: MouseStateHandle,
    expanded: bool,
    expanded_label: &'static str,
    collapsed_label: &'static str,
    action: ConversationUsageViewAction,
    appearance: &Appearance,
) -> Box<dyn Element> {
    let theme = appearance.theme();
    let font_size = appearance.ui_font_subheading();
    let link_color = theme.ansi_fg_blue();
    let icon_size = font_size;
    let (label, icon) = if expanded {
        (expanded_label, Icon::ChevronUp)
    } else {
        (collapsed_label, Icon::ChevronDown)
    };
    Hoverable::new(mouse_state, move |_hover_state| {
        let text_element = Text::new(label.to_string(), appearance.ui_font_family(), font_size)
            .with_color(link_color)
            .with_selectable(false)
            .finish();
        let icon_element = ConstrainedBox::new(icon.to_warpui_icon(link_color.into()).finish())
            .with_width(icon_size)
            .with_height(icon_size)
            .finish();
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Min)
            .with_spacing(4.)
            .with_child(text_element)
            .with_child(icon_element)
            .finish()
    })
    .with_cursor(Cursor::PointingHand)
    .on_click(move |ctx, _, _| {
        ctx.dispatch_typed_action(action.clone());
    })
    .finish()
}

/// Renders a single-space text element used to keep the value column's row
/// count in lock-step with the label column when a row (like "Show N more")
/// only has content on the label side. An `Empty` element would also keep
/// the slot count matched, but `Empty` has zero height, so the value column
/// would collapse by one line and the next label/value pair would slip out
/// of alignment.
fn render_value_text_placeholder(appearance: &Appearance) -> Box<dyn Element> {
    let font_size = appearance.ui_font_subheading();
    Text::new(" ".to_string(), appearance.ui_font_family(), font_size).finish()
}

/// Maximum rendered width of an agent name in a per-agent breakdown row.
const PER_AGENT_LABEL_MAX_WIDTH: f32 = 110.;

/// Maximum number of rows shown in the per-agent breakdown before the
/// "Show N more" affordance truncates the list.
const PER_AGENT_BREAKDOWN_TRUNCATION_CAP: usize = 5;

#[cfg(test)]
mod tests {
    use super::*;
    use warpui::{App, ViewHandle};

    fn placeholder_usage_info() -> ConversationUsageInfo {
        ConversationUsageInfo {
            credits_spent: 0.0,
            credits_spent_for_last_block: None,
            tool_calls: 0,
            models: Vec::new(),
            context_window_usage: 0.0,
            files_changed: 0,
            lines_added: 0,
            lines_removed: 0,
            commands_executed: 0,
        }
    }

    /// Pin `02b53fcd8:app/src/ai/blocklist/usage/conversation_usage_view_tests.rs::
    /// custom_endpoint_models_use_the_external_key_icon_bucket`, adapted to this
    /// fork's simpler `ConversationUsageInfo` (no orchestration-rollup fields).
    #[test]
    fn custom_endpoint_models_use_the_external_key_icon_bucket() {
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![ModelTokenUsage {
                    model_id: "Friendly alias".to_string(),
                    custom_endpoint_tokens: 6,
                    custom_endpoint_token_usage_by_category: HashMap::from([(
                        PRIMARY_AGENT_CATEGORY.to_string(),
                        6,
                    )]),
                    ..Default::default()
                }],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        assert_eq!(
            view.collect_models_by_category()
                .get(PRIMARY_AGENT_CATEGORY),
            Some(&vec![("Friendly alias".to_string(), true)])
        );
    }

    #[test]
    fn legacy_custom_endpoint_tokens_use_the_external_key_icon_bucket() {
        // Backwards compatibility: rows persisted before per-category tracking
        // existed only set the flat `custom_endpoint_tokens` counter.
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![ModelTokenUsage {
                    model_id: "legacy-custom-endpoint".to_string(),
                    custom_endpoint_tokens: 3,
                    ..Default::default()
                }],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        assert_eq!(
            view.collect_models_by_category()
                .get(PRIMARY_AGENT_CATEGORY),
            Some(&vec![("legacy-custom-endpoint".to_string(), true)])
        );
    }

    /// Minimal root view that embeds a [`ConversationUsageView`] via
    /// `add_typed_action_view`, mirroring the real call site
    /// (`terminal/view.rs`'s usage-footer construction) so this test
    /// exercises the same registration path production code uses — not just
    /// `handle_action` in isolation. `App::add_window` requires its root
    /// view to implement `TypedActionView` itself; this host never receives
    /// any action of its own, hence the unit `Action` type.
    struct ConversationUsageViewTestHost {
        usage_view: ViewHandle<ConversationUsageView>,
    }

    impl Entity for ConversationUsageViewTestHost {
        type Event = ();
    }

    impl View for ConversationUsageViewTestHost {
        fn ui_name() -> &'static str {
            "ConversationUsageViewTestHost"
        }

        fn render(&self, _app: &AppContext) -> Box<dyn Element> {
            warpui::elements::ChildView::new(&self.usage_view).finish()
        }
    }

    impl TypedActionView for ConversationUsageViewTestHost {
        type Action = ();

        fn handle_action(&mut self, _action: &Self::Action, _ctx: &mut ViewContext<Self>) {}
    }

    /// Defect-fix regression test (found by the app/ai pin-test sweep,
    /// `docs/sweep/app-ai.md`): `ConversationUsageView::handle_action` used
    /// to be a literal no-op (`fn handle_action(&mut self, _action: &Self::Action,
    /// _ctx: &mut ViewContext<Self>) {}` with `type Action = ()`), so the
    /// "View details" and "Show N more" affordances could never do anything
    /// — there wasn't even a field to hold the expanded state. This
    /// constructs a real orchestrator + child conversation (so
    /// `ConversationUsageView::rollup` returns `Some`, matching the only
    /// condition under which the pin renders these affordances at all),
    /// embeds the view the same way production does (`add_typed_action_view`,
    /// not plain `add_view` — see the comment at the `terminal/view.rs` call
    /// site), and dispatches the two real actions through `handle_action`
    /// directly, the same way `number_shortcut_buttons_tests.rs` tests
    /// `TypedActionView` state changes elsewhere in this crate.
    #[test]
    fn handle_action_toggles_details_and_show_more_and_resets_on_collapse() {
        App::test((), |mut app| async move {
        // `start_new_child_conversation` persists the new child conversation, which reads
        // `GeneralSettings::persist_conversations` and then the sqlite-backed
        // `GlobalResourceHandlesProvider`. Register both so the persist path has the
        // singletons it legitimately needs.
        crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
            app.add_singleton_model(|_| Appearance::mock());
            let terminal_view_id = warpui::EntityId::new();
            let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

            let orchestrator_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });
            let child_id = history.update(&mut app, |history, ctx| {
                history.start_new_child_conversation(
                    terminal_view_id,
                    "ChildAgent".to_string(),
                    orchestrator_id,
                    None,
                    ctx,
                )
            });
            history.update(&mut app, |history, _| {
                history
                    .conversation_mut(&orchestrator_id)
                    .expect("orchestrator conversation is loaded")
                    .set_credits_spent_for_test(1.0);
                history
                    .conversation_mut(&child_id)
                    .expect("child conversation is loaded")
                    .set_credits_spent_for_test(5.0);
            });

            let (_window_id, host) =
                app.add_window(warpui::platform::WindowStyle::NotStealFocus, move |ctx| {
                    let usage_view = ctx.add_typed_action_view(move |_| {
                        ConversationUsageView::new_footer_with_rollup(
                            placeholder_usage_info(),
                            None,
                            MouseStateHandle::default(),
                            orchestrator_id,
                        )
                    });
                    ConversationUsageViewTestHost { usage_view }
                });
            let usage_view = host.read(&app, |host, _| host.usage_view.clone());

            usage_view.read(&app, |view, _| {
                assert!(!view.details_expanded, "starts collapsed");
                assert!(!view.show_all_clicked, "starts un-expanded");
            });

            // "View details" click.
            usage_view.update(&mut app, |view, ctx| {
                view.handle_action(&ConversationUsageViewAction::ToggleDetailsExpanded, ctx);
            });
            usage_view.read(&app, |view, _| {
                assert!(
                    view.details_expanded,
                    "ToggleDetailsExpanded should expand the breakdown"
                );
            });

            // "Show N more" click.
            usage_view.update(&mut app, |view, ctx| {
                view.handle_action(&ConversationUsageViewAction::ShowAllAgentRows, ctx);
            });
            usage_view.read(&app, |view, _| {
                assert!(
                    view.show_all_clicked,
                    "ShowAllAgentRows should reveal the truncated rows"
                );
            });

            // With the breakdown expanded, actually render the view once: a
            // smoke check that `append_per_agent_rows` / `render_per_agent_row`
            // (and the avatar-disc helpers they call) run without panicking
            // now that they're reachable, not just that the state flips.
            usage_view.read(&app, |view, app| {
                let _ = view.render(app);
            });

            // Collapsing ("Hide details") resets show_all_clicked so the user
            // lands back on the truncated list next time they expand.
            usage_view.update(&mut app, |view, ctx| {
                view.handle_action(&ConversationUsageViewAction::ToggleDetailsExpanded, ctx);
            });
            usage_view.read(&app, |view, _| {
                assert!(!view.details_expanded, "second click collapses");
                assert!(
                    !view.show_all_clicked,
                    "collapsing should reset show_all_clicked"
                );
            });
        });
    }

    /// Ported from the pin (`42effe840:app/src/ai/blocklist/usage/
    /// conversation_usage_view_tests.rs::show_all_agent_rows_is_independent_of_details_expanded`).
    ///
    /// The sibling test above walks the affordances in the order a user
    /// clicks them (expand, then "Show N more"), which means it would still
    /// pass if `ShowAllAgentRows` had been written to depend on
    /// `details_expanded` -- e.g. as an early return, or by folding the two
    /// flags into one. This one dispatches `ShowAllAgentRows` first, from the
    /// collapsed state, and pins both halves of the independence: the flag
    /// flips anyway, and it does not implicitly expand the breakdown. The
    /// render path is what gates on `details_expanded` (`:594`); the handler
    /// (`:766`) must not.
    ///
    /// Fixture matches the pin's `initialize_test_app` + `build_view`: the
    /// only singleton the view touches on construction and `ctx.notify()` is
    /// `Appearance`, and `add_window` registers the root view through
    /// `add_typed_action_view` internally, so standing the window up is also
    /// the compile-time proof that `ConversationUsageView: TypedActionView`.
    #[test]
    fn show_all_agent_rows_is_independent_of_details_expanded() {
        App::test((), |mut app| async move {
            app.add_singleton_model(|_| Appearance::mock());
            let (_window_id, view) = app.add_window(
                warpui::platform::WindowStyle::NotStealFocus,
                |_ctx: &mut warpui::ViewContext<ConversationUsageView>| {
                    ConversationUsageView::new(
                        placeholder_usage_info(),
                        DisplayMode::Footer,
                        None,
                        MouseStateHandle::default(),
                    )
                },
            );

            // `ShowAllAgentRows` on its own should flip `show_all_clicked`
            // even when the user hasn't expanded the breakdown yet (the
            // render path won't show rows until expanded, but the handler
            // itself shouldn't care about ordering).
            view.update(&mut app, |view, ctx| {
                view.handle_action(&ConversationUsageViewAction::ShowAllAgentRows, ctx);
            });
            view.read(&app, |view, _| {
                assert!(
                    view.show_all_clicked,
                    "ShowAllAgentRows should flip show_all_clicked regardless of expanded state"
                );
                assert!(
                    !view.details_expanded,
                    "ShowAllAgentRows must not implicitly expand details"
                );
            });
        });
    }

    // -----------------------------------------------------------------------
    // Pure helpers and constructors — no `ViewContext` needed.
    // -----------------------------------------------------------------------

    /// The usage footer renders "1 file changed" / "2 files changed" from this
    /// one helper, so its pluralisation rule is the whole rule.
    #[test]
    fn format_value_text_pluralizes_everything_except_exactly_one() {
        assert_eq!(format_value_text(1, "file"), "1 file");
        assert_eq!(format_value_text(0, "file"), "0 files");
        assert_eq!(format_value_text(2, "file"), "2 files");
        assert_eq!(format_value_text(11, "command"), "11 commands");
        // Negative counts are not expected, but the rule is "== 1", not
        // "abs() == 1": -1 must not read as singular.
        assert_eq!(format_value_text(-1, "line"), "-1 lines");
    }

    /// `new` is the settings/no-rollup constructor and `new_footer_with_rollup`
    /// is the footer one; the difference between them is exactly which
    /// conversation the rollup is computed for, so pin it directly rather than
    /// only through the render path.
    #[test]
    fn the_two_constructors_differ_only_in_display_mode_and_rollup_wiring() {
        let settings_view = ConversationUsageView::new(
            placeholder_usage_info(),
            DisplayMode::Settings,
            None,
            MouseStateHandle::default(),
        );
        assert_eq!(settings_view.display_mode, DisplayMode::Settings);
        assert!(
            settings_view.parent_conversation_id.is_none(),
            "`new` must never wire up a rollup"
        );
        assert!(!settings_view.details_expanded);
        assert!(!settings_view.show_all_clicked);

        let parent_id = AIConversationId::new();
        let footer_view = ConversationUsageView::new_footer_with_rollup(
            placeholder_usage_info(),
            None,
            MouseStateHandle::default(),
            parent_id,
        );
        assert_eq!(
            footer_view.display_mode,
            DisplayMode::Footer,
            "`new_footer_with_rollup` forces footer mode regardless of caller"
        );
        assert_eq!(footer_view.parent_conversation_id, Some(parent_id));
        assert!(!footer_view.details_expanded);
        assert!(!footer_view.show_all_clicked);
    }

    /// `rollup` is gated twice: footer mode, and a known parent conversation.
    /// Both gates are load-bearing — the settings usage page shows historical
    /// usage for one conversation and must never sprout the footer's
    /// per-agent orchestration breakdown — so exercise them against a history
    /// model that would otherwise produce a rollup.
    #[test]
    fn rollup_is_gated_on_footer_mode_and_a_known_parent_conversation() {
        App::test((), |mut app| async move {
            crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
            app.add_singleton_model(|_| Appearance::mock());
            let terminal_view_id = warpui::EntityId::new();
            let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

            let orchestrator_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });
            let child_id = history.update(&mut app, |history, ctx| {
                history.start_new_child_conversation(
                    terminal_view_id,
                    "ChildAgent".to_string(),
                    orchestrator_id,
                    None,
                    ctx,
                )
            });
            history.update(&mut app, |history, _| {
                history
                    .conversation_mut(&child_id)
                    .expect("child conversation is loaded")
                    .set_credits_spent_for_test(7.0);
            });

            history.read(&app, |_, app_ctx| {
                // Wired up: this is the case that must produce a breakdown.
                let footer = ConversationUsageView::new_footer_with_rollup(
                    placeholder_usage_info(),
                    None,
                    MouseStateHandle::default(),
                    orchestrator_id,
                );
                let rollup = footer
                    .rollup(app_ctx)
                    .expect("a footer view over a spending orchestration tree rolls up");
                assert_eq!(rollup.total_credits, 7.0);

                // Footer mode but no parent conversation: the `new` path.
                let footer_without_parent = ConversationUsageView::new(
                    placeholder_usage_info(),
                    DisplayMode::Footer,
                    None,
                    MouseStateHandle::default(),
                );
                assert!(
                    footer_without_parent.rollup(app_ctx).is_none(),
                    "no parent conversation means nothing to roll up"
                );

                // Settings mode: gated even though the same history model is
                // in place.
                let settings = ConversationUsageView::new(
                    placeholder_usage_info(),
                    DisplayMode::Settings,
                    None,
                    MouseStateHandle::default(),
                );
                assert!(
                    settings.rollup(app_ctx).is_none(),
                    "the settings usage page must never show the footer breakdown"
                );
            });
        });
    }

    /// Regression test for the headline "Credits spent (total)" figure.
    ///
    /// The rollup total was computed at the top of `render_unified_layout`
    /// and then used only to decide whether to attach the "View details"
    /// toggle: both `render_total_credits_value_row` call sites passed
    /// `self.usage_info.credits_spent`, which `terminal/view.rs` fills in
    /// from the orchestrator conversation alone. So the headline read the
    /// orchestrator's own spend while the drill-down immediately beneath it
    /// listed the orchestrator *and* its children -- a total smaller than
    /// the list it heads. The pin computes the headline from the rollup
    /// (`42effe840:.../conversation_usage_view.rs:329-332`).
    ///
    /// The assertion is the requirement, not the old behaviour: the
    /// headline must equal the sum of the drill-down rows, computed from
    /// the rendered breakdown itself rather than from a hardcoded number.
    #[test]
    fn headline_total_credits_covers_the_children_listed_in_the_drill_down() {
        App::test((), |mut app| async move {
            crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
            app.add_singleton_model(|_| Appearance::mock());
            let terminal_view_id = warpui::EntityId::new();
            let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

            let orchestrator_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });
            let first_child_id = history.update(&mut app, |history, ctx| {
                history.start_new_child_conversation(
                    terminal_view_id,
                    "FirstChild".to_string(),
                    orchestrator_id,
                    None,
                    ctx,
                )
            });
            let second_child_id = history.update(&mut app, |history, ctx| {
                history.start_new_child_conversation(
                    terminal_view_id,
                    "SecondChild".to_string(),
                    orchestrator_id,
                    None,
                    ctx,
                )
            });
            history.update(&mut app, |history, _| {
                history
                    .conversation_mut(&orchestrator_id)
                    .expect("orchestrator conversation is loaded")
                    .set_credits_spent_for_test(1.0);
                history
                    .conversation_mut(&first_child_id)
                    .expect("first child conversation is loaded")
                    .set_credits_spent_for_test(5.0);
                history
                    .conversation_mut(&second_child_id)
                    .expect("second child conversation is loaded")
                    .set_credits_spent_for_test(2.0);
            });

            history.read(&app, |history, app_ctx| {
                // Exactly what `TerminalView::handle_usage_footer_toggled`
                // builds: `credits_spent` is the orchestrator's own spend,
                // with no knowledge of any descendant.
                let orchestrator_own_credits = history
                    .conversation(&orchestrator_id)
                    .expect("orchestrator conversation is loaded")
                    .credits_spent();
                let view = ConversationUsageView::new_footer_with_rollup(
                    ConversationUsageInfo {
                        credits_spent: orchestrator_own_credits,
                        ..placeholder_usage_info()
                    },
                    None,
                    MouseStateHandle::default(),
                    orchestrator_id,
                );

                let rollup = view
                    .rollup(app_ctx)
                    .expect("orchestrator with spending children rolls up");
                let drill_down_sum: f32 = rollup
                    .per_agent
                    .iter()
                    .map(|entry| entry.credits_spent)
                    .sum();
                let headline = view.headline_total_credits(app_ctx, Some(&rollup));

                assert_eq!(
                    headline,
                    drill_down_sum,
                    "the headline total must equal the rows listed beneath it \
                     (orchestrator {orchestrator_own_credits}, drill-down rows {:?})",
                    rollup
                        .per_agent
                        .iter()
                        .map(|entry| (entry.display_name.clone(), entry.credits_spent))
                        .collect::<Vec<_>>()
                );
                assert_eq!(headline, 8.0, "1 self + 5 + 2 children");
                assert!(
                    headline > orchestrator_own_credits,
                    "the children's spend must not be dropped from the headline"
                );
                assert_eq!(format_credits(headline), "8 credits");

                // No rollup (a plain conversation with no descendants): the
                // fallback is still this conversation's own spend.
                assert_eq!(
                    view.headline_total_credits(app_ctx, None),
                    orchestrator_own_credits
                );
            });
        });
    }

    /// Regression test for the footer headline going stale while the footer
    /// is open.
    ///
    /// `headline_total_credits`' no-rollup limb used to return
    /// `self.usage_info.credits_spent`, which
    /// `TerminalView::handle_usage_footer_toggled` snapshots from the
    /// conversation at the moment the footer is *opened*. The collapsed pill
    /// directly above it (`block/view_impl/output.rs`'s
    /// `usage_pill_headline_credits`) re-derives its number on every render.
    /// So a conversation that spent more with the footer open put two
    /// different totals for one conversation on screen simultaneously — the
    /// rollup fix moved the "headline disagrees with what is under it" defect
    /// into the fallback limb rather than removing it. Both surfaces now route
    /// through `rollup::orchestration_headline_credits`, reading live.
    ///
    /// The childless conversation here is deliberate: it is the *only* shape
    /// that reaches the fallback limb, and it is also the overwhelmingly
    /// common one.
    #[test]
    fn headline_total_credits_tracks_live_spend_while_the_footer_is_open() {
        App::test((), |mut app| async move {
            crate::test_util::settings::initialize_history_persistence_for_tests(&mut app);
            app.add_singleton_model(|_| Appearance::mock());
            let terminal_view_id = warpui::EntityId::new();
            let history = app.add_singleton_model(|_| BlocklistAIHistoryModel::new_for_test());

            let conversation_id = history.update(&mut app, |history, ctx| {
                history.start_new_conversation(terminal_view_id, false, false, ctx)
            });
            history.update(&mut app, |history, _| {
                history
                    .conversation_mut(&conversation_id)
                    .expect("conversation is loaded")
                    .set_credits_spent_for_test(1.0);
            });

            // Exactly the snapshot `handle_usage_footer_toggled` captures when
            // the user opens the footer.
            let view = ConversationUsageView::new_footer_with_rollup(
                ConversationUsageInfo {
                    credits_spent: 1.0,
                    ..placeholder_usage_info()
                },
                None,
                MouseStateHandle::default(),
                conversation_id,
            );

            // The conversation keeps spending while the footer stays open.
            history.update(&mut app, |history, _| {
                history
                    .conversation_mut(&conversation_id)
                    .expect("conversation is loaded")
                    .set_credits_spent_for_test(4.0);
            });

            history.read(&app, |history, app_ctx| {
                let rollup = view.rollup(app_ctx);
                assert!(
                    rollup.is_none(),
                    "a childless conversation has no rollup; this is the fallback limb"
                );

                // What the collapsed pill renders, re-derived every frame.
                let pill_credits = history
                    .conversation(&conversation_id)
                    .expect("conversation is loaded")
                    .credits_spent();
                assert_eq!(pill_credits, 4.0);

                let headline = view.headline_total_credits(app_ctx, rollup.as_ref());
                assert_eq!(
                    headline, pill_credits,
                    "the footer headline must equal the pill above it, not the \
                     value frozen when the footer was opened"
                );
                assert_ne!(
                    headline, view.usage_info.credits_spent,
                    "the open-time snapshot must not reach the screen once the \
                     conversation has spent more"
                );
            });
        });
    }

    // -----------------------------------------------------------------------
    // `collect_models_by_category` — the model/category/key-icon grouping the
    // usage panel renders from.
    // -----------------------------------------------------------------------

    /// A model used with both the user's own key and without it appears twice
    /// in its category, once per key bucket — the two rows carry different
    /// icons, so collapsing them would lose which spend was BYOK.
    #[test]
    fn one_model_used_with_and_without_an_external_key_yields_two_rows() {
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![ModelTokenUsage {
                    model_id: "claude-x".to_string(),
                    warp_token_usage_by_category: HashMap::from([(
                        PRIMARY_AGENT_CATEGORY.to_string(),
                        10,
                    )]),
                    byok_token_usage_by_category: HashMap::from([(
                        PRIMARY_AGENT_CATEGORY.to_string(),
                        20,
                    )]),
                    ..Default::default()
                }],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        let by_category = view.collect_models_by_category();
        let mut rows = by_category
            .get(PRIMARY_AGENT_CATEGORY)
            .expect("the primary agent category is present")
            .clone();
        rows.sort();
        assert_eq!(
            rows,
            vec![
                ("claude-x".to_string(), false),
                ("claude-x".to_string(), true),
            ]
        );
    }

    /// Categories are kept apart: full-terminal-use spend must not be folded
    /// into the primary agent's row, or the panel would attribute one
    /// category's tokens to another.
    #[test]
    fn each_category_keeps_its_own_rows() {
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![ModelTokenUsage {
                    model_id: "model-a".to_string(),
                    warp_token_usage_by_category: HashMap::from([
                        (PRIMARY_AGENT_CATEGORY.to_string(), 5),
                        (FULL_TERMINAL_USE_CATEGORY.to_string(), 7),
                    ]),
                    ..Default::default()
                }],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        let by_category = view.collect_models_by_category();
        assert_eq!(by_category.len(), 2);
        assert_eq!(
            by_category.get(PRIMARY_AGENT_CATEGORY),
            Some(&vec![("model-a".to_string(), false)])
        );
        assert_eq!(
            by_category.get(FULL_TERMINAL_USE_CATEGORY),
            Some(&vec![("model-a".to_string(), false)])
        );
    }

    /// A category recorded with zero tokens is not a row. Only `> 0` entries
    /// are collected, so a model that was selected but never billed for a
    /// category does not appear under it.
    #[test]
    fn zero_token_categories_do_not_produce_rows() {
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![ModelTokenUsage {
                    model_id: "unused".to_string(),
                    warp_token_usage_by_category: HashMap::from([(
                        PRIMARY_AGENT_CATEGORY.to_string(),
                        0,
                    )]),
                    ..Default::default()
                }],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        assert!(view.collect_models_by_category().is_empty());
    }

    /// **Documents a limitation of the legacy fallback, it does not endorse
    /// it.** The pre-per-category `*_tokens` counters are only consulted when
    /// the per-category map came out completely empty. So a conversation whose
    /// usage rows straddle the schema change — one model recorded with
    /// categories, another with only the flat legacy counter — silently drops
    /// the legacy model from the panel entirely.
    ///
    /// The fork's `collect_models_by_category` is byte-identical to the pin's
    /// (`42effe840:app/src/ai/blocklist/usage/conversation_usage_view.rs`), so
    /// this is pinned rather than changed here; making the fallback per-model
    /// instead of whole-view is a behaviour change that wants its own issue.
    #[test]
    fn a_legacy_only_model_is_dropped_when_any_other_model_has_category_data() {
        let view = ConversationUsageView::new(
            ConversationUsageInfo {
                models: vec![
                    ModelTokenUsage {
                        model_id: "new-schema".to_string(),
                        warp_token_usage_by_category: HashMap::from([(
                            PRIMARY_AGENT_CATEGORY.to_string(),
                            4,
                        )]),
                        ..Default::default()
                    },
                    ModelTokenUsage {
                        model_id: "legacy-only".to_string(),
                        byok_tokens: 99,
                        ..Default::default()
                    },
                ],
                ..placeholder_usage_info()
            },
            DisplayMode::Footer,
            None,
            MouseStateHandle::default(),
        );

        assert_eq!(
            view.collect_models_by_category()
                .get(PRIMARY_AGENT_CATEGORY),
            Some(&vec![("new-schema".to_string(), false)]),
            "the legacy-only model is currently invisible in this panel"
        );
    }
}
