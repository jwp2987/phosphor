use warp_core::ui::Icon;
use warpui::assets::asset_cache::AssetSource;
use warpui::elements::MouseStateHandle;

use super::{ChipHorizontalAlignment, MessageItem};

#[test]
fn chip_constructor_creates_enabled_chip_with_items() {
    let item = MessageItem::chip(
        vec![
            MessageItem::text("hello"),
            MessageItem::keystroke(Default::default()),
        ],
        |_ctx| {},
        MouseStateHandle::default(),
    );

    match item {
        MessageItem::Chip {
            items, disabled, ..
        } => {
            assert_eq!(items.len(), 2);
            assert!(!disabled);
        }
        _ => panic!("expected chip variant"),
    }
}

#[test]
fn icon_constructor_creates_icon_without_color_override() {
    let item = MessageItem::icon(Icon::NewConversation);

    match item {
        MessageItem::Icon { icon, color } => {
            assert!(matches!(icon, Icon::NewConversation));
            assert!(color.is_none());
        }
        _ => panic!("expected icon variant"),
    }
}

#[test]
fn set_color_sets_icon_color_override() {
    let color = pathfinder_color::ColorU {
        r: 1,
        g: 2,
        b: 3,
        a: 255,
    };
    let mut item = MessageItem::icon(Icon::MessagePlusSquare);
    item.set_color(color);

    match item {
        MessageItem::Icon {
            color: Some(item_color),
            ..
        } => assert_eq!(item_color, color),
        _ => panic!("expected icon variant with color override"),
    }
}

#[test]
fn image_constructor_creates_image_with_correct_dimensions() {
    let item = MessageItem::image(
        AssetSource::Bundled {
            path: "bundled/svg/test.svg",
        },
        16.,
        16.,
    );

    match item {
        MessageItem::Image {
            source,
            width,
            height,
        } => {
            assert!(matches!(
                source,
                AssetSource::Bundled {
                    path: "bundled/svg/test.svg"
                }
            ));
            assert_eq!(width, 16.);
            assert_eq!(height, 16.);
        }
        _ => panic!("expected image variant"),
    }
}

#[test]
fn with_is_disabled_marks_chip_as_disabled() {
    let item = MessageItem::chip(
        vec![MessageItem::text("hello")],
        |_ctx| {},
        MouseStateHandle::default(),
    )
    .with_is_disabled(true);

    match item {
        MessageItem::Chip { disabled, .. } => assert!(disabled),
        _ => panic!("expected chip variant"),
    }
}

#[test]
fn chip_constructor_defaults_to_left_alignment() {
    let item = MessageItem::chip(
        vec![MessageItem::text("hello")],
        |_ctx| {},
        MouseStateHandle::default(),
    );

    match item {
        MessageItem::Chip {
            horizontal_alignment,
            ..
        } => assert_eq!(horizontal_alignment, ChipHorizontalAlignment::Left),
        _ => panic!("expected chip variant"),
    }
}

#[test]
fn with_horizontal_alignment_sets_right_alignment() {
    let item = MessageItem::chip(
        vec![MessageItem::text("hello")],
        |_ctx| {},
        MouseStateHandle::default(),
    )
    .with_horizontal_alignment(ChipHorizontalAlignment::Right);

    match item {
        MessageItem::Chip {
            horizontal_alignment,
            ..
        } => assert_eq!(horizontal_alignment, ChipHorizontalAlignment::Right),
        _ => panic!("expected chip variant"),
    }
}

#[test]
fn set_is_disabled_propagates_to_nested_interactive_items() {
    let mut item = MessageItem::Chip {
        items: vec![MessageItem::clickable(
            vec![MessageItem::text("nested")],
            |_ctx| {},
            MouseStateHandle::default(),
        )],
        action: std::sync::Arc::new(|_ctx| {}),
        mouse_state: MouseStateHandle::default(),
        disabled: false,
        horizontal_alignment: ChipHorizontalAlignment::default(),
    };
    item.set_is_disabled(true);

    match item {
        MessageItem::Chip {
            disabled, items, ..
        } => {
            assert!(disabled);
            match &items[0] {
                MessageItem::Clickable { disabled, .. } => assert!(*disabled),
                _ => panic!("expected nested clickable item"),
            }
        }
        _ => panic!("expected chip variant"),
    }
}

/// #1962: a message bar is re-rendered only when the row it draws changes; the comparison is the
/// message's fingerprint.
#[test]
fn equal_messages_have_equal_fingerprints_and_any_visible_change_differs() {
    use super::Message;
    use pathfinder_color::ColorU;
    use warpui::keymap::Keystroke;

    let enter = || Keystroke {
        key: "enter".to_owned(),
        ..Default::default()
    };
    let base = || {
        Message::new(vec![
            MessageItem::keystroke(enter()),
            MessageItem::text(" new agent conversation"),
        ])
    };
    let fingerprint = base().display_fingerprint();
    assert!(fingerprint.is_some());
    assert_eq!(fingerprint, base().display_fingerprint());

    // Other text, another keystroke, a color, an added item, an icon: each is a different row.
    let mut other_text = base();
    other_text.append_text(" with `ls` attached");
    let other_key = Message::new(vec![
        MessageItem::keystroke(Keystroke {
            ctrl: true,
            ..enter()
        }),
        MessageItem::text(" new agent conversation"),
    ]);
    let colored = base().with_color(ColorU::new(200, 0, 200, 166));
    let mut longer = base();
    longer.items.push(MessageItem::keystroke(enter()));
    let with_icon = Message::new(vec![
        MessageItem::icon(Icon::NewConversation),
        MessageItem::text(" new agent conversation"),
    ]);
    for changed in [other_text, other_key, colored, longer, with_icon] {
        assert_ne!(fingerprint, changed.display_fingerprint());
    }
    // Item boundaries are part of it: "ab" + "c" is not "a" + "bc".
    assert_ne!(
        Message::new(vec![MessageItem::text("ab"), MessageItem::text("c")]).display_fingerprint(),
        Message::new(vec![MessageItem::text("a"), MessageItem::text("bc")]).display_fingerprint()
    );
}

#[test]
fn a_message_with_a_click_handler_or_an_image_has_no_fingerprint() {
    use super::Message;

    let chip = MessageItem::chip(
        vec![MessageItem::text("hello")],
        |_ctx| {},
        MouseStateHandle::default(),
    );
    assert!(
        Message::new(vec![MessageItem::text("a"), chip])
            .display_fingerprint()
            .is_none()
    );
    let image = MessageItem::image(
        AssetSource::Bundled {
            path: "bundled/svg/test.svg",
        },
        16.,
        16.,
    );
    assert!(Message::new(vec![image]).display_fingerprint().is_none());
}

/// The agent message bar's hints are clickable groups with a fixed action each; the bar vouches
/// for those slots, and they are then compared by slot, disabled flag and what they show.
#[test]
fn a_clickable_group_in_a_vouched_slot_is_compared_by_what_it_shows() {
    use super::Message;

    let help_slot = MouseStateHandle::default();
    let commands_slot = MouseStateHandle::default();
    let other_slot = MouseStateHandle::default();
    let slots = [&help_slot, &commands_slot];
    let hint = |slot: &MouseStateHandle, text: &'static str, disabled: bool| {
        MessageItem::clickable(vec![MessageItem::text(text)], |_ctx| {}, slot.clone())
            .with_is_disabled(disabled)
    };
    let row = |disabled: bool| {
        Message::new(vec![
            hint(&help_slot, "? for help", disabled),
            hint(&commands_slot, "/ for commands", disabled),
        ])
    };

    let enabled = row(false).display_fingerprint_with(&slots);
    assert!(enabled.is_some());
    // The same hints again (typing more into a non-empty buffer): the same row.
    assert_eq!(enabled, row(false).display_fingerprint_with(&slots));
    // The first typed character disables the hints: a different row.
    assert_ne!(enabled, row(true).display_fingerprint_with(&slots));
    // Other text in a slot, or the same group in the other slot: a different row.
    assert_ne!(
        enabled,
        Message::new(vec![
            hint(&help_slot, "? to hide help", false),
            hint(&commands_slot, "/ for commands", false),
        ])
        .display_fingerprint_with(&slots)
    );
    assert_ne!(
        Message::new(vec![hint(&help_slot, "x", false)]).display_fingerprint_with(&slots),
        Message::new(vec![hint(&commands_slot, "x", false)]).display_fingerprint_with(&slots)
    );
    // A group the bar does not vouch for (its click handler may capture anything), also when it
    // is nested or the slot list is empty: never comparable.
    assert!(
        Message::new(vec![hint(&other_slot, "run as shell command", false)])
            .display_fingerprint_with(&slots)
            .is_none()
    );
    assert!(row(false).display_fingerprint().is_none());
}

#[test]
fn a_row_event_renders_only_when_the_row_differs_or_cannot_be_judged() {
    use super::row_event_needs_render;

    assert!(!row_event_needs_render(Some("row"), Some("row"), true));
    assert!(row_event_needs_render(Some("row"), Some("other"), true));
    assert!(row_event_needs_render(None, Some("row"), true));
    assert!(row_event_needs_render(Some("row"), None, true));
    assert!(row_event_needs_render(None, None, true));
    // Kill switch: every such event renders.
    assert!(row_event_needs_render(Some("row"), Some("row"), false));
}
