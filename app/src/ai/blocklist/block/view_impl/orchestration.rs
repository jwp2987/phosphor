//! Rendering functions for orchestration-related output items (messaging & agent management).
//!
//! `render_send_message` (SendMessageToAgent action rendering, from the pin) is not
//! ported here: it depends on `AIAgentActionType::SendMessageToAgent` and
//! `SendMessageToAgentResult`, neither of which exist in this fork. Adding them is
//! out-of-scope surgery of the same shape as Step 3's `AIAgentActionType::RunAgents`
//! (a new action variant walked by the compiler across every exhaustive match on the
//! enum), not something this file can carry on its own. `render_collapse_chevron` and
//! `render_collapsible_text_body` were used only by `render_send_message`, so they were
//! dropped with it rather than ported dead. Everything kept here (the transcript-row
//! renderer and `render_messages_received_from_agents`) is wired into `output.rs`'s
//! `MessagesReceivedFromAgents` match arm, which the fork's message model already
//! supports.

use markdown_parser::{FormattedText, FormattedTextFragment, FormattedTextLine};
use pathfinder_color::ColorU;
use warpui::elements::{
    ConstrainedBox, Container, CrossAxisAlignment, Empty, Flex, FormattedTextElement, Hoverable,
    ParentElement, Shrinkable, Text,
};
use warpui::platform::Cursor;
use warpui::{AppContext, Element, SingletonEntity};

use super::common::render_scrollable_collapsible_content;
use super::output::Props;
use super::WithContentItemSpacing;
use crate::ai::agent::conversation::AIConversationId;
use crate::ai::agent::{MessageId, ReceivedMessageDisplay};
use crate::ai::blocklist::agent_view::orchestration_avatar::OrchestrationAvatar;
use crate::ai::blocklist::agent_view::orchestration_conversation_links::dispatch_focus_or_open_child_agent_pane;
use crate::ai::blocklist::block::model::AIBlockModelHelper;
use crate::ai::blocklist::block::{
    received_message_collapsible_id, AIBlockAction, CollapsibleExpansionState,
};
use crate::ai::blocklist::inline_action::inline_action_icons::icon_size;
use crate::ai::blocklist::orchestration_topology::{
    orchestrator_agent_id_for_conversation, resolve_orchestration_participant,
    OrchestrationParticipantKind,
};
use crate::ai::blocklist::BlocklistAIHistoryModel;
use crate::appearance::Appearance;
use crate::report_error;
use crate::ui_components::blended_colors;
use crate::ui_components::icons::Icon;

const ORCHESTRATION_COLLAPSED_MAX_HEIGHT: f32 = 200.;
#[derive(Clone, Debug, PartialEq, Eq)]
struct OrchestrationParticipant {
    display_name: String,
    avatar: OrchestrationAvatar,
    /// The participant's conversation, when resolved. `None` for the
    /// orchestrator and unknown agents (avatar stays non-clickable).
    conversation_id: Option<AIConversationId>,
}

impl OrchestrationParticipant {
    fn orchestrator() -> Self {
        Self {
            display_name: "Orchestrator".to_string(),
            avatar: OrchestrationAvatar::Orchestrator,
            conversation_id: None,
        }
    }

    fn is_orchestrator(&self) -> bool {
        matches!(&self.avatar, OrchestrationAvatar::Orchestrator)
    }
}

#[cfg(test)]
fn agent_display_name_from_id(
    agent_id: &str,
    orchestrator_agent_id: Option<&str>,
    app: &AppContext,
) -> String {
    participant_for_agent_id(agent_id, orchestrator_agent_id, app).display_name
}

fn participant_for_agent_id(
    agent_id: &str,
    orchestrator_agent_id: Option<&str>,
    app: &AppContext,
) -> OrchestrationParticipant {
    let participant = resolve_orchestration_participant(
        BlocklistAIHistoryModel::as_ref(app),
        agent_id,
        orchestrator_agent_id,
    );
    let display_name = participant.kind.display_name().to_string();
    let avatar = match &participant.kind {
        OrchestrationParticipantKind::Orchestrator => OrchestrationAvatar::Orchestrator,
        OrchestrationParticipantKind::Agent { .. } | OrchestrationParticipantKind::Unknown => {
            OrchestrationAvatar::agent(display_name.clone())
        }
    };
    OrchestrationParticipant {
        display_name,
        avatar,
        conversation_id: match &participant.kind {
            OrchestrationParticipantKind::Orchestrator | OrchestrationParticipantKind::Unknown => {
                None
            }
            OrchestrationParticipantKind::Agent { .. } => participant.conversation_id,
        },
    }
}

fn transcript_metadata(recipients: &[OrchestrationParticipant], subject: &str) -> Option<String> {
    let recipients = recipients
        .iter()
        .filter(|participant| !participant.is_orchestrator())
        .map(|participant| participant.display_name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    match (recipients.is_empty(), subject.is_empty()) {
        (true, true) => None,
        (true, false) => Some(subject.to_string()),
        (false, true) => Some(format!("to {recipients}")),
        (false, false) => Some(format!("to {recipients} • {subject}")),
    }
}

struct TranscriptRowData<'a> {
    participant: &'a OrchestrationParticipant,
    recipients: &'a [OrchestrationParticipant],
    subject: &'a str,
    body: &'a str,
    message_id: &'a MessageId,
    is_streaming: bool,
}

fn render_transcript_row(
    data: TranscriptRowData<'_>,
    props: Props,
    app: &AppContext,
) -> Box<dyn Element> {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let font_family = appearance.ui_font_family();
    let font_size = appearance.monospace_font_size();
    let metadata_color = blended_colors::text_disabled(theme, theme.surface_2());
    let body_color: ColorU = theme.main_text_color(theme.background()).into();
    let collapsible_state = if data.body.is_empty() {
        None
    } else {
        props.collapsible_block_states.get(data.message_id)
    };

    let name = FormattedTextFragment::bold(&data.participant.display_name);
    let header_row_element: Box<dyn Element> = if let Some(state) = collapsible_state {
        // Wrap the name + chevron in one clickable element so either toggles
        // the section. Text is non-selectable + non-interactive so clicks
        // register and the pointing-hand cursor isn't reset by the text.
        let header = render_formatted_text_element(vec![name], app)
            .set_selectable(false)
            .disable_mouse_interaction()
            .finish();
        let text_color = theme.foreground();
        let icon_sz = icon_size(app);
        let is_expanded = matches!(
            state.expansion_state,
            CollapsibleExpansionState::Expanded { .. }
        );
        let chevron_icon = if is_expanded {
            Icon::ChevronDown
        } else {
            Icon::ChevronRight
        };
        let toggle_mouse_state = state.expansion_toggle_mouse_state.clone();
        let message_id_clone = data.message_id.clone();

        let expandable = Hoverable::new(toggle_mouse_state, move |_| {
            // Make the bold name a Shrinkable child so very long agent names
            // shrink within the available width instead of pushing the chevron
            // past the transcript column.
            Flex::row()
                .with_cross_axis_alignment(CrossAxisAlignment::Center)
                .with_child(Shrinkable::new(1., header).finish())
                .with_child(
                    Container::new(
                        ConstrainedBox::new(chevron_icon.to_warpui_icon(text_color).finish())
                            .with_width(icon_sz)
                            .with_height(icon_sz)
                            .finish(),
                    )
                    .with_margin_left(6.)
                    .finish(),
                )
                .finish()
        })
        .with_cursor(Cursor::PointingHand)
        .on_click(move |ctx, _, _| {
            ctx.dispatch_typed_action(AIBlockAction::ToggleCollapsibleBlockExpanded(
                message_id_clone.clone(),
            ));
        });

        // Wrap the Hoverable in a Shrinkable inside an outer row so it
        // receives a bounded width constraint from the parent column. This
        // lets the inner Shrinkable around the bold name actually shrink
        // when the name is long, while the Hoverable's click bounds still
        // size to its content (bold name + chevron) when it fits.
        Flex::row()
            .with_child(Shrinkable::new(1., expandable.finish()).finish())
            .finish()
    } else {
        let header = render_formatted_text_element(vec![name], app).finish();
        Flex::row()
            .with_cross_axis_alignment(CrossAxisAlignment::Center)
            .with_child(Shrinkable::new(1., header).finish())
            .finish()
    };

    let mut content = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
    content.add_child(header_row_element);
    if let Some(metadata) = transcript_metadata(data.recipients, data.subject) {
        content.add_child(
            Container::new(
                Text::new(metadata, font_family, font_size)
                    .with_color(metadata_color)
                    .with_selectable(true)
                    .finish(),
            )
            .with_margin_top(2.)
            .finish(),
        );
    }
    if !data.body.is_empty() {
        let body_element = Container::new(
            Text::new(data.body.to_string(), font_family, font_size)
                .with_color(body_color)
                .with_selectable(true)
                .finish(),
        )
        .with_margin_top(8.)
        .finish();
        if let Some(body) =
            render_collapsible_body(data.message_id, body_element, data.is_streaming, props)
        {
            content.add_child(body);
        }
    }

    let avatar = data.participant.avatar.render(app);
    let avatar_element: Box<dyn Element> = if let (Some(conversation_id), Some(mouse_state)) = (
        data.participant.conversation_id,
        props
            .state_handles
            .transcript_avatar_handles
            .get(data.message_id),
    ) {
        // Navigate to the child's pane: focus if already open, otherwise
        // open a new pane.
        let mouse_state = mouse_state.clone();
        let self_terminal_view_id = props.terminal_view_id;
        Hoverable::new(mouse_state, move |_| avatar)
            .with_cursor(Cursor::PointingHand)
            .on_click(move |ctx, app, _| {
                dispatch_focus_or_open_child_agent_pane(
                    conversation_id,
                    self_terminal_view_id,
                    ctx,
                    app,
                );
            })
            .finish()
    } else {
        avatar
    };

    Flex::row()
        .with_cross_axis_alignment(CrossAxisAlignment::Start)
        .with_child(
            Container::new(avatar_element)
                .with_margin_right(12.)
                .finish(),
        )
        .with_child(Shrinkable::new(1., content.finish()).finish())
        .finish()
}

pub(super) fn render_messages_received_from_agents(
    messages: &[ReceivedMessageDisplay],
    props: Props,
    app: &AppContext,
) -> Box<dyn Element> {
    if messages.is_empty() {
        return Empty::new().finish();
    }
    let orchestrator_agent_id = props.model.conversation(app).and_then(|conversation| {
        orchestrator_agent_id_for_conversation(BlocklistAIHistoryModel::as_ref(app), conversation)
    });
    let mut column = Flex::column().with_cross_axis_alignment(CrossAxisAlignment::Stretch);
    for (index, msg) in messages.iter().enumerate() {
        let sender =
            participant_for_agent_id(&msg.sender_agent_id, orchestrator_agent_id.as_deref(), app);
        let recipients = msg
            .addresses
            .iter()
            .map(|agent_id| {
                participant_for_agent_id(agent_id, orchestrator_agent_id.as_deref(), app)
            })
            .collect::<Vec<_>>();
        let row_message_id = received_message_collapsible_id(&msg.message_id);
        let row = render_transcript_row(
            TranscriptRowData {
                participant: &sender,
                recipients: &recipients,
                subject: &msg.subject,
                body: &msg.message_body,
                message_id: &row_message_id,
                is_streaming: false,
            },
            props,
            app,
        );
        let mut row_container = Container::new(row);
        if index > 0 {
            row_container = row_container.with_margin_top(12.);
        }
        column.add_child(row_container.finish());
    }

    column.finish().with_agent_output_item_spacing(app).finish()
}

/// Renders the collapsible body content with max height and scroll, or None if collapsed.
fn render_collapsible_body(
    message_id: &MessageId,
    body: Box<dyn Element>,
    is_streaming: bool,
    props: Props,
) -> Option<Box<dyn Element>> {
    let Some(state) = props.collapsible_block_states.get(message_id) else {
        report_error!(
            "Missing collapsible state for orchestration message",
            extra: { "message_id" => ?message_id }
        );
        return None;
    };
    render_scrollable_collapsible_content(
        message_id,
        state,
        body,
        is_streaming,
        ORCHESTRATION_COLLAPSED_MAX_HEIGHT,
    )
}

/// Builds a `FormattedTextElement` from a list of mixed plain/bold fragments.
fn render_formatted_text_element(
    fragments: Vec<FormattedTextFragment>,
    app: &AppContext,
) -> FormattedTextElement {
    let appearance = Appearance::as_ref(app);
    let theme = appearance.theme();
    let formatted_text = FormattedText::new(vec![FormattedTextLine::Line(fragments)]);
    FormattedTextElement::new(
        formatted_text,
        appearance.monospace_font_size(),
        appearance.ui_font_family(),
        appearance.ui_font_family(),
        blended_colors::text_main(theme, theme.background()),
        Default::default(),
    )
    .set_selectable(true)
}

#[cfg(test)]
#[path = "orchestration_tests.rs"]
mod tests;
