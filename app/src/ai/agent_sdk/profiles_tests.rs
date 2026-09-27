//! #637: `agent profile list` must print IDs that `agent run --profile` accepts.
//!
//! `--profile` resolves its argument by matching it against the same strings the list
//! prints (`find_profile_by_cli_id`), so these tests pin the printed form and the match.
//! `SyncId`s are built through its serde form — the same path persisted profiles load
//! through — rather than by importing the ID constructors.

use super::{DEFAULT_PROFILE_CLI_ID, SyncId, cli_id_matches, profile_cli_id};

fn sync_id(raw: &str) -> SyncId {
    serde_json::from_value(serde_json::Value::String(raw.to_string()))
        .expect("sync ID should deserialize")
}

#[test]
fn locally_created_profile_is_listed_by_its_client_id() {
    // Locally created profiles carry a client ID; this is the case that used to print
    // `Unsynced` and be unselectable.
    let raw = format!("Client-{}", uuid::Uuid::new_v4());
    let id = sync_id(&raw);
    assert!(matches!(id, SyncId::ClientId(_)), "{id:?}");

    let printed = profile_cli_id(Some(id));

    assert_eq!(printed, raw);
    assert!(cli_id_matches(&printed, &raw));
    assert!(cli_id_matches(&printed, &format!("  {raw}\n")));
}

#[test]
fn legacy_server_profile_is_listed_by_its_server_id() {
    let raw = "abcdefghijklmnopqrstuv";
    let id = sync_id(raw);
    assert!(matches!(id, SyncId::ServerId(_)), "{id:?}");

    let printed = profile_cli_id(Some(id));

    assert_eq!(printed, raw);
    assert!(cli_id_matches(&printed, raw));
}

#[test]
fn unsynced_default_profile_is_listed_as_default() {
    let printed = profile_cli_id(None);

    assert_eq!(printed, DEFAULT_PROFILE_CLI_ID);
    assert_ne!(printed, "Unsynced");
    assert!(cli_id_matches(&printed, "default"));
}

#[test]
fn other_ids_do_not_match() {
    let printed = profile_cli_id(Some(sync_id(&format!("Client-{}", uuid::Uuid::new_v4()))));
    let other_client_id = format!("Client-{}", uuid::Uuid::new_v4());

    for raw in ["", "   ", "Unsynced", "default", other_client_id.as_str()] {
        assert!(!cli_id_matches(&printed, raw), "{raw:?} matched {printed}");
    }
}
