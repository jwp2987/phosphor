use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

pub(super) static FRAME_MARKS: AtomicUsize = AtomicUsize::new(0);

/// The counter is process-global and tests run in parallel; serialise the ones that read it.
static COUNTER_LOCK: Mutex<()> = Mutex::new(());

#[test]
fn zone_and_mark_compile_in_any_position_and_do_not_disturb_control_flow() {
    // With the feature off these expand to nothing; with it on `init` must come first.
    super::init();
    let value = {
        crate::zone!("test_zone");
        crate::mark!("test_mark");
        41 + 1
    };
    assert_eq!(value, 42);
}

#[test]
fn present_frame_marks_exactly_once_per_present() {
    let _guard = COUNTER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = FRAME_MARKS.load(Ordering::SeqCst);
    let mut presented = 0;
    for _ in 0..3 {
        super::present_frame(|| presented += 1);
    }
    assert_eq!(presented, 3);
    assert_eq!(FRAME_MARKS.load(Ordering::SeqCst) - before, 3);
}

#[test]
fn present_frame_returns_the_present_result_and_still_marks_on_error() {
    let _guard = COUNTER_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let before = FRAME_MARKS.load(Ordering::SeqCst);
    let result: Result<(), &str> = super::present_frame(|| Err("lost surface"));
    assert_eq!(result, Err("lost surface"));
    assert_eq!(FRAME_MARKS.load(Ordering::SeqCst) - before, 1);
}

#[cfg(not(feature = "profiling"))]
#[test]
fn feature_off_macros_expand_to_nothing() {
    // An expansion that evaluated anything or produced a value would change this block's type.
    #[allow(clippy::let_unit_value)]
    let unit: () = {
        crate::zone!("nothing");
        crate::mark!("nothing");
    };
    assert_eq!(unit, ());
}
