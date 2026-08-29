use warp_multi_agent_api as api;

use super::*;
use crate::ai::agent_events::AgentRunEvent;

fn make_run_event(event_type: &str, run_id: &str, ref_id: Option<&str>) -> AgentRunEvent {
    AgentRunEvent {
        event_type: event_type.to_string(),
        run_id: run_id.to_string(),
        ref_id: ref_id.map(|value| value.to_string()),
        execution_id: None,
        occurred_at: "2026-01-01T00:00:00Z".to_string(),
        sequence: 1,
    }
}

#[test]
fn classify_child_agent_started_on_self_is_child_started() {
    let event = make_run_event(EVENT_CHILD_AGENT_STARTED, "parent-run", Some("child-run"));
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::ChildStarted {
            child_run_id: "child-run".to_string(),
        }
    );
}

#[test]
fn classify_child_agent_started_without_ref_id_is_opaque() {
    // A discovery event with no child run id carries nothing actionable.
    let event = make_run_event(EVENT_CHILD_AGENT_STARTED, "parent-run", None);
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::Opaque
    );
}

#[test]
fn classify_run_session_linked_on_child_is_session_linked() {
    let event = make_run_event(EVENT_RUN_SESSION_LINKED, "child-run", Some("session-uuid"));
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::ChildSessionLinked {
            child_run_id: "child-run".to_string(),
            session_uuid: "session-uuid".to_string(),
        }
    );
}

#[test]
fn classify_run_in_progress_on_child_is_child_lifecycle() {
    let event = make_run_event("run_in_progress", "child-run", None);
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::ChildLifecycle {
            child_run_id: "child-run".to_string(),
            kind: api::LifecycleEventType::InProgress,
        }
    );
}

#[test]
fn classify_new_message_on_self_is_parent_self() {
    let event = make_run_event("new_message", "parent-run", Some("msg-1"));
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::ParentSelf(event.clone())
    );
}

#[test]
fn classify_lifecycle_on_self_is_parent_self() {
    // The parent's own lifecycle events belong to the parent (ParentSelf),
    // not the child tracker.
    let event = make_run_event("run_in_progress", "parent-run", None);
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::ParentSelf(event.clone())
    );
}

#[test]
fn classify_unknown_type_is_opaque() {
    let event = make_run_event("some_unknown_event", "child-run", None);
    assert_eq!(
        classify_family_event(&event, "parent-run"),
        FamilyEvent::Opaque
    );
}

/// Covers every row of [`lifecycle_event_type_from_wire`]'s table. The seven
/// ported `classify_*` cases only reach `run_in_progress`, so without this a
/// mutation of any other row -- swapping `Succeeded` and `Failed`, or mapping
/// `run_blocked` to `Cancelled` -- would pass the whole file. Upstream covers
/// these rows indirectly through `convert_lifecycle_events` tests, which did
/// not come across with this port.
#[test]
#[allow(deprecated)]
fn lifecycle_wire_table_maps_every_recognised_type() {
    let cases: &[(&str, api::LifecycleEventType)] = &[
        // Canonical types, aligned with task states.
        ("run_in_progress", api::LifecycleEventType::InProgress),
        ("run_succeeded", api::LifecycleEventType::Succeeded),
        ("run_failed", api::LifecycleEventType::Failed),
        ("run_errored", api::LifecycleEventType::Errored),
        ("run_cancelled", api::LifecycleEventType::Cancelled),
        ("run_blocked", api::LifecycleEventType::Blocked),
        // Legacy types, folded onto their canonical replacements. These must
        // keep resolving: an older peer can still emit them on the wire.
        ("run_started", api::LifecycleEventType::InProgress),
        ("run_idle", api::LifecycleEventType::Succeeded),
        ("run_restarted", api::LifecycleEventType::InProgress),
    ];

    for (wire, expected) in cases {
        assert_eq!(
            lifecycle_event_type_from_wire(wire),
            Some(*expected),
            "wire type {wire} must map to {expected:?}"
        );
    }
}

#[test]
fn lifecycle_wire_table_rejects_non_lifecycle_types() {
    // `new_message` is a real wire type, but it is not a lifecycle transition:
    // it must not be absorbed into the table, or a child's inbox event would
    // classify as ChildLifecycle.
    assert_eq!(lifecycle_event_type_from_wire(EVENT_NEW_MESSAGE), None);
    assert_eq!(
        lifecycle_event_type_from_wire(EVENT_CHILD_AGENT_STARTED),
        None
    );
    assert_eq!(
        lifecycle_event_type_from_wire(EVENT_RUN_SESSION_LINKED),
        None
    );
    assert_eq!(lifecycle_event_type_from_wire("run_"), None);
    assert_eq!(lifecycle_event_type_from_wire(""), None);
}
