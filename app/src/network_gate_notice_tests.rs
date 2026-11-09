use super::*;
use http_client::gate::Outcome;

fn denied(host: &str, hash: u64) -> GateEvent {
    GateEvent {
        seq: 1,
        purpose: "models-dev",
        host: host.to_owned(),
        port: Some(443),
        host_hash: hash,
        outcome: Outcome::Denied,
    }
}

#[test]
fn a_repeat_denial_has_the_same_toast_id_so_it_replaces() {
    let a = denied("x.example", 7);
    let mut b = denied("x.example", 7);
    b.seq = 99;
    assert_eq!(toast_object_id(&a), toast_object_id(&b));
    assert_ne!(
        toast_object_id(&a),
        toast_object_id(&denied("y.example", 8))
    );
    assert!(toast_object_id(&a).starts_with("network-gate-denied-models-dev-"));
}

#[test]
fn only_a_denial_with_a_host_offers_the_allow_link() {
    assert_eq!(
        allow_text(&denied("x.example", 7)).as_deref(),
        Some("x.example:443")
    );
    // No host (an unparseable URL, or the redirect cap): no origin to allow.
    assert!(allow_text(&denied("", 0)).is_none());
}
