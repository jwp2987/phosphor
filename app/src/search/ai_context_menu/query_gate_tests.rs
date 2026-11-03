use super::QueryGate;

#[test]
fn enter_during_debounce_is_stale_and_waits() {
    let mut gate = QueryGate::default();
    gate.request("fo".into());
    // Debounce has not fired: displayed results answer an older query.
    assert!(gate.is_stale());
    gate.defer_select();
    assert!(
        !gate.take_ready_select(false),
        "must not act on stale results"
    );

    gate.applied("fo", true);
    assert!(
        !gate.take_ready_select(false),
        "applied but mixer has not answered yet"
    );

    gate.results_reported();
    assert!(gate.take_ready_select(false));
    assert!(!gate.take_ready_select(false), "the Enter fires once");
}

#[test]
fn newer_keystroke_cancels_waiting_enter() {
    let mut gate = QueryGate::default();
    gate.request("f".into());
    gate.defer_select();
    gate.request("fo".into());
    gate.applied("fo", true);
    gate.results_reported();
    assert!(!gate.take_ready_select(false));
}

#[test]
fn superseded_apply_keeps_gate_stale() {
    let mut gate = QueryGate::default();
    gate.request("f".into());
    gate.request("fo".into());
    gate.applied("f", true);
    assert!(gate.is_stale());
    gate.results_reported();
    assert!(gate.is_stale(), "the newer query is still outstanding");
}

#[test]
fn same_query_apply_does_not_wait_for_results() {
    let mut gate = QueryGate::default();
    gate.request("fo".into());
    gate.defer_select();
    gate.applied("fo", false);
    assert!(gate.take_ready_select(false));
}

#[test]
fn loading_mixer_holds_the_enter() {
    let mut gate = QueryGate::default();
    gate.request("fo".into());
    gate.defer_select();
    gate.applied("fo", true);
    gate.results_reported();
    assert!(!gate.take_ready_select(true));
    assert!(gate.take_ready_select(false));
}

#[test]
fn give_up_only_cancels_its_own_wait() {
    let mut gate = QueryGate::default();
    gate.request("a".into());
    let first = gate.defer_select();
    gate.cancel_select();
    let second = gate.defer_select();
    assert!(!gate.give_up(first));
    assert!(gate.select_waiting());
    assert!(gate.give_up(second));
    assert!(!gate.select_waiting());
}

#[test]
fn idle_gate_is_not_stale() {
    let gate = QueryGate::default();
    assert!(!gate.is_stale());
}

#[test]
fn give_up_unsticks_missing_results_signal() {
    let mut gate = QueryGate::default();
    gate.request("a".into());
    gate.applied("a", true);
    let id = gate.defer_select();
    assert!(gate.give_up(id));
    assert!(!gate.is_stale(), "later Enters must not be blocked forever");
}
