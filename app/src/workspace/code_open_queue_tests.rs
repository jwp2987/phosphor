use super::*;

fn tab(n: usize) -> EntityId {
    EntityId::from_usize(n)
}

fn payloads(ready: &[ReadyCodeOpen<&'static str>]) -> Vec<&'static str> {
    ready.iter().map(|r| r.payload).collect()
}

#[test]
fn later_request_waits_for_an_earlier_slow_check() {
    let mut queue = CodeOpenQueue::default();
    let a = queue.push_pending(tab(1), "A");
    let b = queue.push_pending(tab(2), "B");
    // B's stat returns first: nothing may open yet.
    assert!(queue.resolve(b, Vec::new()).is_empty());
    // A's arrives: both open, A first, each with the tab it was requested from.
    let ready = queue.resolve(a, Vec::new());
    assert_eq!(payloads(&ready), vec!["A", "B"]);
    assert_eq!(ready[0].target, tab(1));
    assert_eq!(ready[1].target, tab(2));
    assert!(queue.is_empty());
}

#[test]
fn in_order_results_open_immediately() {
    let mut queue = CodeOpenQueue::default();
    let a = queue.push_pending(tab(1), "A");
    let b = queue.push_pending(tab(1), "B");
    assert_eq!(payloads(&queue.resolve(a, Vec::new())), vec!["A"]);
    assert_eq!(payloads(&queue.resolve(b, Vec::new())), vec!["B"]);
}

#[test]
fn unchecked_request_does_not_overtake_a_pending_one() {
    let mut queue = CodeOpenQueue::default();
    let a = queue.push_pending(tab(1), "A");
    queue.push_checked(tab(1), "B");
    assert!(queue.drain_ready().is_empty());
    assert_eq!(payloads(&queue.resolve(a, Vec::new())), vec!["A", "B"]);
}

#[test]
fn unchecked_request_with_nothing_ahead_opens_at_once() {
    let mut queue = CodeOpenQueue::default();
    queue.push_checked(tab(3), "A");
    assert_eq!(payloads(&queue.drain_ready()), vec!["A"]);
}

#[test]
fn giving_up_releases_the_queue_and_a_late_result_is_ignored() {
    let mut queue = CodeOpenQueue::default();
    let a = queue.push_pending(tab(1), "A");
    queue.push_checked(tab(1), "B");
    let ready = queue.give_up(a);
    assert_eq!(payloads(&ready), vec!["A", "B"]);
    assert_eq!(ready[0].outcome, CodeOpenOutcome::GaveUp);
    assert!(queue.resolve(a, Vec::new()).is_empty());
    assert!(queue.give_up(a).is_empty());
}

#[test]
fn give_up_after_the_result_arrived_is_a_no_op() {
    let mut queue = CodeOpenQueue::default();
    let a = queue.push_pending(tab(1), "A");
    assert_eq!(payloads(&queue.resolve(a, Vec::new())), vec!["A"]);
    assert!(queue.give_up(a).is_empty());
}

#[test]
fn target_tab_is_never_switched_to() {
    let tabs = [tab(10), tab(11), tab(12)];
    assert_eq!(resolve_target(&tabs, 1, tab(11)), TargetTab::Active(1));
    // The user moved to tab 2 while the check ran: open in tab 0 in the background.
    assert_eq!(resolve_target(&tabs, 2, tab(10)), TargetTab::Background(0));
    assert_eq!(resolve_target(&tabs, 2, tab(99)), TargetTab::Closed);
}
