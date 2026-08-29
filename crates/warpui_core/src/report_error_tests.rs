use std::sync::atomic::AtomicBool;

use super::take_once;

// Only the primitive is covered here. The macro's own per-callsite throttling is not
// observable from a test in this crate -- see the "Per-callsite throttling" note on
// `report_error!` in `report_error.rs` for why, and for what is relied on instead.

#[test]
fn take_once_fires_exactly_once() {
    let flag = AtomicBool::new(false);
    assert!(take_once(&flag), "the first call must be allowed through");
    for _ in 0..100 {
        assert!(!take_once(&flag), "every later call must be suppressed");
    }
}
