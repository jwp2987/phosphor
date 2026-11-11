//! The index sync tests that run no shell: these run on every platform. The ones that drive a
//! local `sh` are in `index_sync_tests.rs` (unix only).

use super::*;

#[test]
fn ranked_paths_keeps_best_first_and_drops_repeats() {
    let hit = |p: &str, s: f32| Hit {
        path: p.into(),
        start_line: 0,
        end_line: 0,
        score: s,
    };
    let hits = [hit("/r/a", 0.9), hit("/r/b", 0.8), hit("/r/a", 0.7)];
    assert_eq!(ranked_paths(&hits), ["/r/a", "/r/b"]);
    assert_eq!(absolute_path("/r/", "x/y"), "/r/x/y");
}
