use super::*;
use winit::dpi::PhysicalPosition;
use winit::event::DeviceId;

fn touch(phase: TouchPhase, x: f64, y: f64) -> Touch {
    Touch {
        device_id: DeviceId::dummy(),
        phase,
        location: PhysicalPosition::new(x, y),
        force: None,
        id: 1,
    }
}

fn state() -> WindowState {
    WindowState::new(crate::WindowId::new())
}

#[test]
fn long_press_fires_while_the_finger_is_still_down() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let generation = ws.touch_generation;

    // The timer armed at touch-down fires with the finger still down.
    let event = convert_touch_long_press(&mut ws, generation);
    assert!(matches!(
        event,
        Some(crate::event::Event::RightMouseDown { click_count: 1, .. })
    ));
    assert!(matches!(
        ws.last_touch_purpose,
        Some(TouchPurpose::LongPressed)
    ));

    // The release afterwards does not open a second menu or send a click.
    assert!(convert_touch_ended(touch(TouchPhase::Ended, 100., 200.), &mut ws, 1.).is_none());
    assert!(ws.last_touch_purpose.is_none());
    // A duplicate timer event for the same touch is stale.
    assert!(convert_touch_long_press(&mut ws, generation).is_none());
}

#[test]
fn moving_out_of_the_tap_slop_cancels_the_long_press() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let generation = ws.touch_generation;

    // A swipe past the slop reclassifies the touch as a scroll.
    convert_touch_moved(touch(TouchPhase::Moved, 100., 300.), &mut ws, 1., 0.);
    assert!(convert_touch_long_press(&mut ws, generation).is_none());
    assert!(matches!(
        ws.last_touch_purpose,
        Some(TouchPurpose::Scroll(_))
    ));
}

#[test]
fn a_small_wobble_keeps_the_long_press_armed() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let generation = ws.touch_generation;
    assert!(convert_touch_moved(touch(TouchPhase::Moved, 103., 202.), &mut ws, 1., 0.).is_none());
    assert!(convert_touch_long_press(&mut ws, generation).is_some());
}

#[test]
fn releasing_before_the_timer_makes_it_stale() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let generation = ws.touch_generation;

    let up = convert_touch_ended(touch(TouchPhase::Ended, 100., 200.), &mut ws, 1.);
    assert!(matches!(
        up,
        Some(ConvertedEvent::Event(
            crate::event::Event::LeftMouseUp { .. }
        ))
    ));
    assert!(convert_touch_long_press(&mut ws, generation).is_none());
}

#[test]
fn a_timer_from_an_earlier_touch_does_not_fire_on_the_next_one() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let old_generation = ws.touch_generation;
    convert_touch_ended(touch(TouchPhase::Ended, 100., 200.), &mut ws, 1.);
    ws.last_touch_purpose = None;

    convert_touch_started(touch(TouchPhase::Started, 50., 50.), &mut ws, 1.);
    assert_ne!(ws.touch_generation, old_generation);
    assert!(convert_touch_long_press(&mut ws, old_generation).is_none());
    assert!(convert_touch_long_press(&mut ws, ws.touch_generation).is_some());
}

#[test]
fn a_cancelled_touch_cannot_long_press() {
    let mut ws = state();
    convert_touch_started(touch(TouchPhase::Started, 100., 200.), &mut ws, 1.);
    let generation = ws.touch_generation;
    convert_touch_cancelled(touch(TouchPhase::Cancelled, 100., 200.), &mut ws, 1.);
    assert!(convert_touch_long_press(&mut ws, generation).is_none());
}
