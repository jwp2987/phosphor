use super::{decide_with, paint_only_unless_disabled, RenderFingerprint, WakeupRedraw};
use crate::terminal::model::terminal_model::BlockIndex;
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
        read_only: false,
        evicted_total: 4,
        active_block_index: BlockIndex(7),
        has_active_gap: false,
        has_block_banner: false,
        filtered_block_count: 0,
        has_block_selection: false,
        overhanging_bottom_block: true,
        in_band_command: false,
    }
}

fn block_list_fingerprint() -> RenderFingerprint {
    let mut fingerprint = alt_screen_fingerprint();
    fingerprint.alt_screen_active = false;
    fingerprint.input_box_visible = true;
    fingerprint
}

#[test]
fn an_unchanged_alt_screen_tree_only_needs_paint() {
    let rendered = alt_screen_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, true, true),
        WakeupRedraw::PaintOnly
    );
}

#[test]
fn the_first_wakeup_before_any_render_is_full() {
    assert_eq!(
        decide_with(None, alt_screen_fingerprint(), true, true),
        WakeupRedraw::Full
    );
}

#[test]
fn an_unchanged_block_list_tree_is_laid_out_and_painted_but_not_re_rendered() {
    let rendered = block_list_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, true, true),
        WakeupRedraw::Relayout
    );
    assert_eq!(
        decide_with(None, rendered, true, true),
        WakeupRedraw::Full
    );
}

#[test]
fn block_list_render_inputs_force_a_re_render() {
    let rendered = block_list_fingerprint();

    let mut block_evicted = rendered;
    block_evicted.evicted_total += 1;
    assert_eq!(
        decide_with(Some(rendered), block_evicted, true, true),
        WakeupRedraw::Full
    );

    let mut new_block = rendered;
    new_block.active_block_index = BlockIndex(8);
    assert_eq!(
        decide_with(Some(rendered), new_block, true, true),
        WakeupRedraw::Full
    );

    let mut command_finished = rendered;
    command_finished.long_running_command = false;
    assert_eq!(
        decide_with(Some(rendered), command_finished, true, true),
        WakeupRedraw::Full
    );

    let mut overhang_gone = rendered;
    overhang_gone.overhanging_bottom_block = false;
    assert_eq!(
        decide_with(Some(rendered), overhang_gone, true, true),
        WakeupRedraw::Full
    );
}

#[test]
fn the_block_list_kill_switch_forces_full_frames() {
    let rendered = block_list_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, true, false),
        WakeupRedraw::Full
    );
    // The two switches are independent.
    let alt = alt_screen_fingerprint();
    assert_eq!(
        decide_with(Some(alt), alt, true, false),
        WakeupRedraw::PaintOnly
    );
    assert_eq!(
        decide_with(Some(rendered), rendered, false, true),
        WakeupRedraw::Relayout
    );
}

#[test]
fn any_changed_render_input_forces_a_re_render() {
    let rendered = alt_screen_fingerprint();

    let mut footer_appeared = rendered;
    footer_appeared.footer_bar_has_content = true;
    assert_eq!(
        decide_with(Some(rendered), footer_appeared, true, true),
        WakeupRedraw::Full
    );

    let mut bg_sampled = rendered;
    bg_sampled.inferred_bg_color = Some(ColorU::new(9, 9, 9, 255));
    assert_eq!(
        decide_with(Some(rendered), bg_sampled, true, true),
        WakeupRedraw::Full
    );

    let mut input_shown = rendered;
    input_shown.input_box_visible = true;
    assert_eq!(
        decide_with(Some(rendered), input_shown, true, true),
        WakeupRedraw::Full
    );

    // Once that tree has been rendered, the next wakeup is paint-only again.
    assert_eq!(
        decide_with(Some(input_shown), input_shown, true, true),
        WakeupRedraw::PaintOnly
    );
}

#[test]
fn the_kill_switch_forces_full_frames() {
    let rendered = alt_screen_fingerprint();
    assert_eq!(
        decide_with(Some(rendered), rendered, false, true),
        WakeupRedraw::Full
    );

    assert!(paint_only_unless_disabled(None));
    assert!(paint_only_unless_disabled(Some("0")));
    assert!(!paint_only_unless_disabled(Some("1")));
}
