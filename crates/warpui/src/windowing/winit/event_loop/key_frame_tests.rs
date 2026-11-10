use super::*;

fn all_true() -> Facts {
    Facts {
        enabled: true,
        redraw_is_throttled: true,
        scene_invalid: true,
        present_never_waits: true,
        can_borrow_app: true,
    }
}

#[test]
fn a_key_with_an_undrawn_invalidation_is_drawn_at_the_end_of_the_iteration() {
    let mut state = KeyFrameNow::default();
    state.note_key();
    assert!(state.take_decision(all_true()));
}

#[test]
fn without_a_key_nothing_bypasses_the_frame_callback() {
    // A streaming reply invalidates the window all the time; those frames keep the throttle.
    let mut state = KeyFrameNow::default();
    assert!(!state.take_decision(all_true()));
}

#[test]
fn a_key_is_offered_once_and_many_keys_in_one_iteration_make_one_frame() {
    let mut state = KeyFrameNow::default();
    state.note_key();
    state.note_key();
    state.note_key();
    assert!(state.take_decision(all_true()));
    // The next iteration has no key: no second frame.
    assert!(!state.take_decision(all_true()));
}

#[test]
fn a_frame_drawn_in_the_same_iteration_satisfies_the_key() {
    // RedrawRequested was not held back (no callback outstanding): the ordinary frame ran.
    let mut state = KeyFrameNow::default();
    state.note_key();
    state.note_frame_drawn();
    assert!(!state.take_decision(all_true()));
}

#[test]
fn every_fact_is_required() {
    let cases: [fn(&mut Facts); 5] = [
        |f| f.enabled = false,
        |f| f.redraw_is_throttled = false,
        |f| f.scene_invalid = false,
        |f| f.present_never_waits = false,
        |f| f.can_borrow_app = false,
    ];
    for (i, unset) in cases.iter().enumerate() {
        let mut state = KeyFrameNow::default();
        state.note_key();
        let mut facts = all_true();
        unset(&mut facts);
        assert!(!state.take_decision(facts), "fact {i} unset must not draw");
        // The key was consumed: it keeps its ordinary RedrawRequested and is not re-offered.
        assert!(!state.take_decision(all_true()), "fact {i}: re-offered");
    }
}

#[test]
fn two_blocked_frames_in_a_row_turn_the_path_off_for_the_window() {
    let mut state = KeyFrameNow::default();
    assert!(!state.note_now_frame(BLOCKED_FRAME));
    assert!(!state.is_disabled());
    assert!(
        state.note_now_frame(BLOCKED_FRAME + Duration::from_millis(12)),
        "the second blocked frame reports the switch-off"
    );
    assert!(state.is_disabled());
    state.note_key();
    assert!(!state.take_decision(all_true()));
    // Reported once only.
    assert!(!state.note_now_frame(BLOCKED_FRAME));
}

#[test]
fn a_fast_frame_between_two_slow_ones_keeps_the_path_on() {
    let mut state = KeyFrameNow::default();
    assert!(!state.note_now_frame(BLOCKED_FRAME));
    assert!(!state.note_now_frame(Duration::from_micros(300)));
    assert!(!state.note_now_frame(BLOCKED_FRAME));
    assert!(!state.is_disabled());
    state.note_key();
    assert!(state.take_decision(all_true()));
}

#[test]
fn only_zero_is_the_kill_switch() {
    assert!(enabled_from_env(None));
    assert!(enabled_from_env(Some("1")));
    assert!(enabled_from_env(Some("")));
    assert!(!enabled_from_env(Some("0")));
    assert!(!enabled_from_env(Some(" 0 ")));
}

#[test]
fn the_hop_detail_is_the_shape_the_harness_parses() {
    // script/realapp/appci/render_hops.py _KEY_FRAME_NOW: `hop=key_frame_now wait_ms=<ms>`.
    assert_eq!(now_frame_detail(Duration::from_micros(300)), "wait_ms=0.3");
    assert_eq!(now_frame_detail(Duration::from_millis(16)), "wait_ms=16.0");
}
