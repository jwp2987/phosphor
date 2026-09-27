//! #637: `agent profile list` must print IDs that `agent run --profile` accepts.

use super::{DEFAULT_PROFILE_CLI_ID, ProfileSelector, parse_profile_selector, profile_cli_id};
use crate::server::ids::{ClientId, ServerId, SyncId};

#[test]
fn locally_created_profile_id_round_trips_through_the_flag() {
    // Locally created profiles carry a client ID; this is the case that used to print
    // `Unsynced` and be unselectable.
    let sync_id = SyncId::ClientId(ClientId::new());

    let printed = profile_cli_id(Some(sync_id));

    assert!(printed.starts_with("Client-"), "{printed}");
    assert_eq!(
        parse_profile_selector(&printed),
        Some(ProfileSelector::Sync(sync_id))
    );
}

#[test]
fn legacy_server_profile_id_round_trips_through_the_flag() {
    let sync_id = SyncId::ServerId(ServerId::try_from("abcdefghijklmnopqrstuv").unwrap());

    let printed = profile_cli_id(Some(sync_id));

    assert_eq!(printed, "abcdefghijklmnopqrstuv");
    assert_eq!(
        parse_profile_selector(&printed),
        Some(ProfileSelector::Sync(sync_id))
    );
}

#[test]
fn unsynced_default_profile_is_listed_and_selectable_as_default() {
    let printed = profile_cli_id(None);

    assert_eq!(printed, DEFAULT_PROFILE_CLI_ID);
    assert_ne!(printed, "Unsynced");
    assert_eq!(
        parse_profile_selector(&printed),
        Some(ProfileSelector::Default)
    );
    assert_eq!(
        parse_profile_selector("Default"),
        Some(ProfileSelector::Default)
    );
}

#[test]
fn unrecognised_profile_ids_are_rejected() {
    for raw in ["", "Unsynced", "Client-not-a-uuid", "too-short"] {
        assert_eq!(parse_profile_selector(raw), None, "{raw:?}");
    }
}
