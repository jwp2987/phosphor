use super::{decide_with, paint_only_unless_disabled, RenderFingerprint, WakeupRedraw};
use warpui::color::ColorU;

fn alt_screen_fingerprint() -> RenderFingerprint {
    RenderFingerprint {
        alt_screen_active: true,
        input_box_visible: false,
        footer_bar_has_content: false,
        remote_server_loading_footer: false,
        shared_session_view_pending: false,
        shared_session_active_viewer: false,
        shared_session_viewer: false,
        loading_conversation_transcript: false,
        agent_in_control_or_tagged_in: false,
        waterfall_gap_mode: false,
        long_running_command: true,
        inferred_bg_color: Some(ColorU::new(1, 2, 3, 255)),
    }
}

#[test]
fn an_unchanged_alt_screen_tree_only_needs_paint() {
    let rendered = alt_screen_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, true),
        WakeupRedraw::PaintOnly
    );
}

#[test]
fn the_first_wakeup_before_any_render_is_full() {
    assert_eq!(
        decide_with(None, alt_screen_fingerprint(), true),
        WakeupRedraw::Full
    );
}

#[test]
fn the_block_list_always_gets_a_full_frame() {
    let mut block_list = alt_screen_fingerprint();
    block_list.alt_screen_active = false;
    assert_eq!(
        decide_with(Some(block_list), block_list, true),
        WakeupRedraw::Full
    );
}

#[test]
fn any_changed_render_input_forces_a_re_render() {
    let rendered = alt_screen_fingerprint();

    let mut footer_appeared = rendered;
    footer_appeared.footer_bar_has_content = true;
    assert_eq!(
        decide_with(Some(rendered), footer_appeared, true),
        WakeupRedraw::Full
    );

    let mut bg_sampled = rendered;
    bg_sampled.inferred_bg_color = Some(ColorU::new(9, 9, 9, 255));
    assert_eq!(
        decide_with(Some(rendered), bg_sampled, true),
        WakeupRedraw::Full
    );

    let mut input_shown = rendered;
    input_shown.input_box_visible = true;
    assert_eq!(
        decide_with(Some(rendered), input_shown, true),
        WakeupRedraw::Full
    );

    // Once that tree has been rendered, the next wakeup is paint-only again.
    assert_eq!(
        decide_with(Some(input_shown), input_shown, true),
        WakeupRedraw::PaintOnly
    );
}

#[test]
fn the_kill_switch_forces_full_frames() {
    let rendered = alt_screen_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, false),
        WakeupRedraw::Full
    );

    assert!(paint_only_unless_disabled(None));
    assert!(paint_only_unless_disabled(Some("0")));
    assert!(!paint_only_unless_disabled(Some("1")));
}
