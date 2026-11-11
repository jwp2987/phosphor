use super::*;

fn unsettled(count: usize, ids: u64) -> TurnFacts {
    TurnFacts {
        unsettled: count,
        unsettled_ids: ids,
        ..Default::default()
    }
}

/// A healthy turn at every point of its life has no fault: nothing held, a stream in flight
/// with the previous reply's calls not answered yet, a pane that is closing.
#[test]
fn a_turn_that_something_still_moves_has_no_fault() {
    assert_eq!(judge(&TurnFacts::default()), None);
    let mid_stream = TurnFacts {
        stream_in_flight: true,
        ..unsettled(2, 7)
    };
    assert_eq!(
        judge(&mid_stream),
        None,
        "while a stream is in flight the calls are not queued yet"
    );
    let closing = TurnFacts {
        suspended_for_close: true,
        parked: 1,
        ..unsettled(1, 7)
    };
    assert_eq!(judge(&closing), None, "a close discards results on purpose");
}

/// #1946: a cleanly finished reply's call that nothing holds and nothing answered, with no
/// stream in flight, is a fault; so is an action parked with no batch ahead (#1859), and that
/// one is reported first (its heal only queues).
#[test]
fn calls_nothing_holds_and_parked_actions_are_faults() {
    assert_eq!(
        judge(&unsettled(2, 7)),
        Some(TurnFault::UnsettledCalls { count: 2, ids: 7 })
    );
    let parked = TurnFacts {
        parked: 3,
        ..Default::default()
    };
    assert_eq!(judge(&parked), Some(TurnFault::ParkedActions { count: 3 }));
    let both = TurnFacts {
        parked: 1,
        ..unsettled(2, 7)
    };
    assert_eq!(judge(&both), Some(TurnFault::ParkedActions { count: 1 }));
    // A parked action is reported with a stream in flight too: its batch is done, the stream
    // has nothing to do with it.
    let parked_mid_stream = TurnFacts {
        parked: 1,
        stream_in_flight: true,
        ..Default::default()
    };
    assert_eq!(
        judge(&parked_mid_stream),
        Some(TurnFault::ParkedActions { count: 1 })
    );
}

/// The bounded, event-based condition: a fault is healed only when the same fault was seen on
/// `CONFIRM_ROUNDS` consecutive hops, and exactly once.
#[test]
fn a_fault_is_confirmed_after_the_same_fault_on_consecutive_hops() {
    let fault = TurnFault::UnsettledCalls { count: 1, ids: 9 };
    let mut watch = StuckWatch::default();
    assert!(watch.schedule(), "the first event schedules a hop");
    assert!(!watch.schedule(), "one hop is pending at a time");
    for _ in 1..CONFIRM_ROUNDS {
        assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
        assert!(
            !watch.schedule(),
            "a recheck is the pending hop: an event in between schedules no second one"
        );
    }
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Confirmed(fault));
    // Healed once. If the heal changed nothing, the fault is not healed again.
    assert!(watch.schedule());
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Clear);
    assert!(watch.schedule(), "nothing is left pending after a clear");
}

/// A state the lifecycle only passes through is never healed: a hop with no fault, or with a
/// different one, starts the count again.
#[test]
fn a_fault_that_goes_away_or_changes_is_not_confirmed() {
    let fault = TurnFault::UnsettledCalls { count: 1, ids: 9 };
    let other = TurnFault::UnsettledCalls { count: 1, ids: 10 };
    let mut watch = StuckWatch::default();
    assert!(CONFIRM_ROUNDS >= 3, "the test below needs two rechecks");
    assert!(watch.schedule());
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
    // Gone on the third hop (the calls were queued): clear, and the count starts over.
    assert_eq!(watch.on_hop(None), WatchStep::Clear);
    assert!(watch.schedule());
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
    // Another reply's calls: a different fault, counted from one.
    assert_eq!(watch.on_hop(Some(other)), WatchStep::Recheck);
    for _ in 2..CONFIRM_ROUNDS {
        assert_eq!(watch.on_hop(Some(other)), WatchStep::Recheck);
    }
    assert_eq!(watch.on_hop(Some(other)), WatchStep::Confirmed(other));
}

/// A fault healed once is healed again when it comes back after a fault-free hop: the `healed`
/// mark only stops healing the same unchanged state twice.
#[test]
fn a_healed_fault_that_returns_later_is_counted_again() {
    let fault = TurnFault::ParkedActions { count: 1 };
    let mut watch = StuckWatch::default();
    for _ in 1..CONFIRM_ROUNDS {
        assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
    }
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Confirmed(fault));
    assert_eq!(watch.on_hop(None), WatchStep::Clear);
    for _ in 1..CONFIRM_ROUNDS {
        assert_eq!(watch.on_hop(Some(fault)), WatchStep::Recheck);
    }
    assert_eq!(watch.on_hop(Some(fault)), WatchStep::Confirmed(fault));
}

/// The kill switch: on by default and for anything that does not say off.
#[test]
fn the_kill_switch_is_on_unless_the_environment_says_off() {
    assert!(self_heal_enabled_from(None));
    assert!(self_heal_enabled_from(Some("1")));
    assert!(self_heal_enabled_from(Some("")));
    assert!(self_heal_enabled_from(Some("yes")));
    for off in ["0", "off", "OFF", "false", " False ", "no"] {
        assert!(!self_heal_enabled_from(Some(off)), "{off:?} turns it off");
    }
}

/// The id hash is the same for the same set of calls in any order, and differs for another set.
#[test]
fn the_id_hash_names_a_set_of_calls() {
    let a = AIAgentActionId::from("call_a".to_owned());
    let b = AIAgentActionId::from("call_b".to_owned());
    let c = AIAgentActionId::from("call_c".to_owned());
    assert_eq!(ids_hash([&a, &b]), ids_hash([&b, &a]));
    assert_ne!(ids_hash([&a, &b]), ids_hash([&a, &c]));
    assert_ne!(ids_hash([&a]), ids_hash([&a, &b]));
}

#[test]
fn fault_names_are_the_log_names() {
    assert_eq!(
        TurnFault::UnsettledCalls { count: 1, ids: 0 }.name(),
        "unsettled_calls"
    );
    assert_eq!(
        TurnFault::ParkedActions { count: 1 }.name(),
        "parked_actions"
    );
    assert_eq!(
        TurnFault::BlockedQuestionWithoutCard { ids: 0 }.name(),
        "blocked_question_without_card"
    );
}

/// #2103: a pane close erases the finished results of the reply it stops, on purpose (#848).
/// Those calls then have no status and no result, and they are discarded, not lost: the reply
/// is not judged for unsettled calls, whether the pane is still closed or the close was undone.
/// Round 14 (r14a): the paced agent fuzz ran ctrl_c, close_tab, stop on a waiting card, and the
/// heal logged `hop=agent_turn_stuck fault=unsettled_calls` and cancelled the call a second
/// time. An action that is parked is a fault all the same: the close parks nothing.
///
/// Red on the old code: `TurnFacts` had no such fact, and `judge` returned `UnsettledCalls`.
#[test]
fn a_reply_a_close_stopped_is_not_judged_for_unsettled_calls() {
    let after_the_undo = TurnFacts {
        reply_discarded_for_close: true,
        ..unsettled(2, 7)
    };
    assert_eq!(
        judge(&after_the_undo),
        None,
        "the close discarded these calls' results: nothing is lost"
    );
    let parked_too = TurnFacts {
        parked: 1,
        ..after_the_undo
    };
    assert_eq!(
        judge(&parked_too),
        Some(TurnFault::ParkedActions { count: 1 }),
        "a parked action is still a fault"
    );
    // The control: the same calls in a reply no close stopped.
    assert_eq!(
        judge(&unsettled(2, 7)),
        Some(TurnFault::UnsettledCalls { count: 2, ids: 7 })
    );
}
