use super::*;

#[test]
fn mac_with_setting_on_quits_when_last_window_closes() {
    assert!(should_quit_app(false, true, 1, CloseReason::Requested));
    assert!(should_quit_app(false, true, 0, CloseReason::AlreadyClosed));
}

#[test]
fn mac_with_setting_off_keeps_running() {
    assert!(!should_quit_app(false, false, 1, CloseReason::Requested));
    assert!(!should_quit_app(
        false,
        false,
        0,
        CloseReason::AlreadyClosed
    ));
}

#[test]
fn linux_and_windows_quit_regardless_of_setting() {
    assert!(should_quit_app(true, false, 1, CloseReason::Requested));
    assert!(should_quit_app(true, false, 0, CloseReason::AlreadyClosed));
}

#[test]
fn closing_one_of_several_windows_never_quits() {
    for windows in 2..5 {
        assert!(!should_quit_app(
            true,
            true,
            windows,
            CloseReason::Requested
        ));
    }
    assert!(!should_quit_app(true, true, 1, CloseReason::AlreadyClosed));
}

#[test]
fn content_transfer_never_quits() {
    assert!(!should_quit_app(
        true,
        true,
        1,
        CloseReason::ContentTransferred
    ));
    assert!(!should_quit_app(
        true,
        true,
        0,
        CloseReason::ContentTransferred
    ));
}

#[test]
fn mac_default_setting_is_on() {
    // #1154: the old default (false) left the app running with no window.
    assert!(crate::terminal::general_settings::QUIT_ON_LAST_WINDOW_CLOSED_DEFAULT);
}
