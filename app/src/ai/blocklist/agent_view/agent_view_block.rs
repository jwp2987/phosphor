use pathfinder_color::ColorU;
use settings::Setting;
use warp_core::ui::{Icon, appearance::Appearance};
use warpui::{
    Element, Entity, ModelHandle, SingletonEntity, TypedActionView, View, ViewContext,
    elements::{
        ConstrainedBox, Container, CrossAxisAlignment, Empty, Flex, Hoverable, MainAxisSize,
        MouseStateHandle, ParentElement, Shrinkable, Text,
    },
    fonts::{Properties, Style, Weight::Bold},
    platform::Cursor,
    prelude::{Border, CornerRadius, Radius},
    text_layout::ClipConfig,
};

use crate::{
    BlocklistAIHistoryModel,
    ai::{
        agent::conversation::{AIConversationId, ConversationStatus},
        blocklist::BlocklistAIHistoryEvent,
    },
    terminal::BlockListSettings,
    ui_components::blended_colors,
};

use super::{AgentViewController, AgentViewEntryOrigin};

#[derive(Default)]
struct StateHandles {
    block: MouseStateHandle,
}

pub struct AgentViewEntryBlockParams {
    pub conversation_id: AIConversationId,
    pub is_new: bool,
    pub is_restored: bool,
    pub origin: AgentViewEntryOrigin,
    pub agent_view_controller: ModelHandle<AgentViewController>,
}

/// Rich content block rendered in the terminal mode blocklist to represent an Agent View entry for
/// a given conversation.
pub struct AgentViewEntryBlock {
    conversation_id: AIConversationId,
    agent_view_controller: ModelHandle<AgentViewController>,
    is_new: bool,
    is_restored: bool,
    origin: AgentViewEntryOrigin,
    /// Cached title for rendering when conversation no longer exists (i.e. after deletion).
    cached_title: Option<String>,
    /// What this row last saw on a streaming delta; see [`streaming_delta_needs_render`].
    last_streaming_signature: Option<EntrySignature>,
    state_handles: StateHandles,
}

impl AgentViewEntryBlock {
    pub fn new(params: AgentViewEntryBlockParams, ctx: &mut ViewContext<Self>) -> Self {
        let AgentViewEntryBlockParams {
            conversation_id,
            is_new,
            is_restored,
            origin,
            agent_view_controller,
        } = params;
        let history_model = BlocklistAIHistoryModel::handle(ctx);
        ctx.subscribe_to_model(&history_model, |me, _, event, ctx| match event {
            BlocklistAIHistoryEvent::UpdatedStreamingExchange {
                conversation_id, ..
            } if *conversation_id == me.conversation_id => {
                // One of these arrives per streamed delta, but this row renders only the
                // conversation's title, status and passive-ness; re-render when one changed
                // (#1282).
                let signature = EntrySignature::of(&me.conversation_id, ctx);
                if !streaming_delta_needs_render(&mut me.last_streaming_signature, signature) {
                    return;
                }
                ctx.notify();
            }
            BlocklistAIHistoryEvent::UpdatedConversationStatus {
                conversation_id, ..
            }
            // A regenerated or renamed title (e.g. `start_title_generation`) otherwise sits
            // unpainted here until some other event happens to repaint the row -- this block
            // renders `conversation.title()` directly and had nothing subscribing to its
            // changes. See issue #691.
            | BlocklistAIHistoryEvent::UpdatedConversationMetadata {
                conversation_id, ..
            } if *conversation_id == me.conversation_id => {
                ctx.notify();
            }
            BlocklistAIHistoryEvent::DeletedConversation {
                conversation_id,
                conversation_title,
                ..
            } if *conversation_id == me.conversation_id => {
                me.cached_title = conversation_title.clone();
                ctx.notify();
            }
            _ => (),
        });
        ctx.subscribe_to_model(&agent_view_controller, |_, _, _, ctx| ctx.notify());

        Self {
            conversation_id,
            agent_view_controller,
            is_new,
            is_restored,
            origin,
            cached_title: Default::default(),
            state_handles: Default::default(),
            last_streaming_signature: None,
        }
    }
}

/// What a streaming delta could change in [`AgentViewEntryBlock::render`]: the title, the
/// status and whether the conversation is entirely passive.
#[derive(Clone, PartialEq, Eq)]
struct EntrySignature {
    title: Option<String>,
    status: ConversationStatus,
    passive: bool,
}

impl EntrySignature {
    fn of(conversation_id: &AIConversationId, app: &warpui::AppContext) -> Option<Self> {
        let conversation = BlocklistAIHistoryModel::as_ref(app).conversation(conversation_id)?;
        Some(Self {
            title: conversation.title(),
            status: conversation.status().clone(),
            passive: conversation.is_entirely_passive(),
        })
    }
}

/// Records `current` and returns whether it differs from the signature the row last saw.
/// Always true when the signature is unknown or the conversation is gone.
fn streaming_delta_needs_render(
    last: &mut Option<EntrySignature>,
    current: Option<EntrySignature>,
) -> bool {
    let changed = current.is_none() || *last != current;
    *last = current;
    changed
}

#[cfg(test)]
mod streaming_signature_tests {
    use super::*;

    fn sig(title: &str, status: ConversationStatus) -> Option<EntrySignature> {
        Some(EntrySignature {
            title: Some(title.to_owned()),
            status,
            passive: false,
        })
    }

    #[test]
    fn deltas_with_unchanged_inputs_do_not_render() {
        let mut last = None;
        assert!(streaming_delta_needs_render(
            &mut last,
            sig("t", ConversationStatus::InProgress)
        ));
        assert!(!streaming_delta_needs_render(
            &mut last,
            sig("t", ConversationStatus::InProgress)
        ));
        assert!(streaming_delta_needs_render(
            &mut last,
            sig("t2", ConversationStatus::InProgress)
        ));
        assert!(streaming_delta_needs_render(
            &mut last,
            sig("t2", ConversationStatus::Success)
        ));
        assert!(streaming_delta_needs_render(&mut last, None));
    }
}

pub fn render_block_container(
    origin: AgentViewEntryOrigin,
    content: Box<dyn Element>,
    background: ColorU,
    appearance: &Appearance,
    are_block_dividers_enabled: bool,
) -> Box<dyn Element> {
    let border = if are_block_dividers_enabled {
        Border::top(1.).with_border_fill(appearance.theme().outline())
    } else {
        Border::new(1.)
            .with_sides(true, false, true, false)
            .with_border_fill(appearance.theme().outline())
    };

    let mut container = Container::new(content).with_background(background);

    if matches!(origin, AgentViewEntryOrigin::LongRunningCommand) {
        container = container
            .with_uniform_padding(12.)
            .with_horizontal_margin(16.)
            .with_margin_bottom(16.)
            .with_margin_top(8.)
            .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)));
    } else {
        container = container
            .with_horizontal_padding(20.)
            .with_vertical_padding(18.)
            .with_border(border);
    }

    container.finish()
}

fn render_subtext(text: String, appearance: &Appearance) -> Box<dyn Element> {
    Container::new(
        Text::new(
            text,
            appearance.ui_font_family(),
            appearance.monospace_font_size() - 2.,
        )
        .with_color(blended_colors::text_disabled(
            appearance.theme(),
            appearance.theme().background(),
        ))
        .with_style(Properties {
            style: Style::Italic,
            ..Default::default()
        })
        .finish(),
    )
    .with_margin_left(8.)
    .finish()
}

fn render_deleted_state(
    origin: AgentViewEntryOrigin,
    cached_title: Option<String>,
    appearance: &Appearance,
    are_block_dividers_enabled: bool,
) -> Box<dyn Element> {
    let disabled_color =
        blended_colors::text_disabled(appearance.theme(), appearance.theme().background());

    let row = Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Center)
        .with_main_axis_size(MainAxisSize::Max)
        .with_child(
            Text::new(
                cached_title.unwrap_or_else(|| crate::t!("conversation-deleted")),
                appearance.ui_font_family(),
                appearance.monospace_font_size(),
            )
            .with_color(disabled_color)
            .with_style(Properties {
                weight: Bold,
                ..Default::default()
            })
            .finish(),
        )
        .with_child(render_subtext(crate::t!("common-deleted"), appearance))
        .finish();

    render_block_container(
        origin,
        row,
        blended_colors::fg_overlay_1(appearance.theme()).into(),
        appearance,
        are_block_dividers_enabled,
    )
}

impl View for AgentViewEntryBlock {
    fn ui_name() -> &'static str {
        "EnterAgentBlock"
    }

    fn render(&self, app: &warpui::AppContext) -> Box<dyn warpui::Element> {
        if self.agent_view_controller.as_ref(app).is_fullscreen() {
            return Empty::new().finish();
        }

        let appearance = Appearance::as_ref(app);
        let are_block_dividers_enabled =
            *BlockListSettings::as_ref(app).show_block_dividers.value();

        let history_model = BlocklistAIHistoryModel::as_ref(app);
        let Some(conversation) = history_model.conversation(&self.conversation_id) else {
            // If the agent_view_block's conversation no longer exists,
            // we assume that it has been deleted.
            return render_deleted_state(
                self.origin,
                self.cached_title.clone(),
                appearance,
                are_block_dividers_enabled,
            );
        };

        if conversation.is_entirely_passive() {
            return Empty::new().finish();
        }

        fn with_opacity(mut color: ColorU, opacity: u8) -> ColorU {
            color.a = opacity;
            color
        }

        let status_icon = conversation.status().render_icon(appearance);
        let status_icon_bg = match conversation.status() {
            ConversationStatus::InProgress => {
                with_opacity(appearance.theme().ansi_fg_magenta(), 25)
            }
            ConversationStatus::Success => with_opacity(appearance.theme().ansi_fg_green(), 25),
            ConversationStatus::Error => with_opacity(appearance.theme().ansi_fg_red(), 25),
            // Recovery pending: match the yellow accent used by its status icon.
            ConversationStatus::TransientError => {
                with_opacity(appearance.theme().ansi_fg_yellow(), 25)
            }
            ConversationStatus::Cancelled => {
                with_opacity(blended_colors::neutral_5(appearance.theme()), 25)
            }
            ConversationStatus::Blocked { .. } => {
                with_opacity(appearance.theme().ansi_fg_yellow(), 25)
            }
            // Quiescent but still working; keep the in-progress accent.
            ConversationStatus::WaitingForEvents => {
                with_opacity(appearance.theme().ansi_fg_magenta(), 25)
            }
        };

        let mut row = Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_main_axis_size(MainAxisSize::Max)
            .with_child(
                Container::new(
                    ConstrainedBox::new(status_icon.finish())
                        .with_height(16.)
                        .with_width(16.)
                        .finish(),
                )
                .with_uniform_padding(2.)
                .with_background_color(status_icon_bg)
                .with_corner_radius(CornerRadius::with_all(Radius::Pixels(4.)))
                .with_margin_right(8.)
                .finish(),
            )
            .with_child(
                Shrinkable::new(
                    1.,
                    Text::new(
                        conversation
                            .title()
                            .unwrap_or_else(|| crate::t!("conversation-untitled")),
                        appearance.ui_font_family(),
                        appearance.monospace_font_size(),
                    )
                    .with_color(blended_colors::text_main(
                        appearance.theme(),
                        appearance.theme().background(),
                    ))
                    .with_style(Properties {
                        weight: Bold,
                        ..Default::default()
                    })
                    .soft_wrap(false)
                    .with_clip(ClipConfig::ellipsis())
                    .finish(),
                )
                .finish(),
            );

        let subtext = if self.is_restored {
            Some(crate::t!("common-restored"))
        } else if !self.is_new
            && !matches!(
                self.origin,
                AgentViewEntryOrigin::LongRunningCommand
                    | AgentViewEntryOrigin::AgentRequestedNewConversation
            )
        {
            Some(crate::t!("common-continued"))
        } else {
            None
        };

        if let Some(subtext) = subtext {
            row.add_child(render_subtext(subtext, appearance));
        }

        row.add_child(
            Container::new(Empty::new().finish())
                .with_margin_right(8.)
                .finish(),
        );
        row.add_child(
            ConstrainedBox::new(
                Icon::ChevronRight
                    .to_warpui_icon(
                        blended_colors::text_sub(
                            appearance.theme(),
                            appearance.theme().background(),
                        )
                        .into(),
                    )
                    .finish(),
            )
            .with_height(20.)
            .with_width(20.)
            .finish(),
        );

        let conversation_id = self.conversation_id;
        let origin = self.origin;
        Hoverable::new(self.state_handles.block.clone(), move |hoverable_state| {
            let background = if hoverable_state.is_hovered() {
                blended_colors::fg_overlay_2(appearance.theme())
            } else {
                blended_colors::fg_overlay_1(appearance.theme())
            };
            render_block_container(
                origin,
                row.finish(),
                background.into(),
                appearance,
                are_block_dividers_enabled,
            )
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(EnterAgentBlockAction::EnterAgentMode { conversation_id });
        })
        .finish()
    }
}

#[derive(Debug, Clone)]
pub enum AgentViewEntryBlockEvent {
    EnterAgentView { conversation_id: AIConversationId },
}

impl Entity for AgentViewEntryBlock {
    type Event = AgentViewEntryBlockEvent;
}

#[derive(Debug, Clone)]
pub enum EnterAgentBlockAction {
    EnterAgentMode { conversation_id: AIConversationId },
}

impl TypedActionView for AgentViewEntryBlock {
    type Action = EnterAgentBlockAction;

    fn handle_action(&mut self, action: &Self::Action, ctx: &mut ViewContext<Self>) {
        match action {
            EnterAgentBlockAction::EnterAgentMode { conversation_id } => {
                ctx.emit(AgentViewEntryBlockEvent::EnterAgentView {
                    conversation_id: *conversation_id,
                });
            }
        }
    }
}
