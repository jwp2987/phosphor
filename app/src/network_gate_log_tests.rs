use super::*;

fn entry(seq: u64) -> LoggedGateEvent {
    LoggedGateEvent {
        event: GateEvent {
            seq,
            purpose: "test",
            host: format!("h{seq}.example"),
            port: Some(443),
            host_hash: seq,
            outcome: Outcome::Denied,
        },
        at: Local::now(),
    }
}

#[test]
fn keeps_arrival_order() {
    let mut log = GateLogEntries::default();
    for i in 0..5 {
        log.push(entry(i));
    }
    let seqs: Vec<u64> = log.iter().map(|e| e.event.seq).collect();
    assert_eq!(seqs, [0, 1, 2, 3, 4]);
}

#[test]
fn drops_the_oldest_past_the_cap() {
    let mut log = GateLogEntries::default();
    let total = CAPACITY as u64 + 7;
    for i in 0..total {
        log.push(entry(i));
    }
    assert_eq!(log.len(), CAPACITY);
    assert_eq!(log.iter().next().unwrap().event.seq, 7);
    assert_eq!(log.iter().last().unwrap().event.seq, total - 1);
}

#[test]
fn clear_empties_the_log_and_it_fills_again() {
    let mut log = GateLogEntries::default();
    log.push(entry(1));
    log.clear();
    assert_eq!(log.len(), 0);
    log.push(entry(2));
    assert_eq!(log.len(), 1);
}
