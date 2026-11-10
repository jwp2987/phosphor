use anyhow::anyhow;

use super::*;

/// #1977: a default keychain that cannot be opened is one transition (so one log line and one
/// notice) however many lookups fail, and a keychain that opens again clears it.
#[test]
fn a_missing_default_keychain_is_recorded_once_and_recovers() {
    let _guard = availability::test_guard();
    availability::reset_for_test();
    let changes = availability::subscribe();

    let first = note_keychain_open::<(), _>(Err(anyhow!("A default keychain could not be found.")));
    assert!(matches!(first, Err(Error::Unknown(_))));
    assert_eq!(
        availability::current(),
        availability::Availability::Unavailable
    );
    let second = note_keychain_open::<(), _>(Err(anyhow!("still none")));
    assert!(second.is_err());
    assert_eq!(
        changes.try_recv(),
        Ok(availability::Availability::Unavailable)
    );
    assert!(
        changes.try_recv().is_err(),
        "two failed lookups publish one transition"
    );

    let opened = note_keychain_open::<u8, anyhow::Error>(Ok(7));
    assert_eq!(opened.ok(), Some(7));
    assert_eq!(
        availability::current(),
        availability::Availability::Available
    );
    availability::reset_for_test();
}
