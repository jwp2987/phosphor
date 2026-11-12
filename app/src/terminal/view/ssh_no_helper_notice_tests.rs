//! #1960 item 2: the notice appears once per no-helper session and never for a helper session.

use super::*;
use crate::terminal::model::session::{IsLegacySSHSession, ssh_no_helper_notice_due};
use std::path::PathBuf;

fn ssh(single_connection: bool) -> IsLegacySSHSession {
    IsLegacySSHSession::Yes {
        socket_path: PathBuf::new(),
        external_control_master: false,
        raw_bootstrap: false,
        single_connection,
        bootstrap_in_command: false,
    }
}

#[test]
fn a_no_helper_ssh_session_is_due_and_a_helper_session_or_local_shell_is_not() {
    assert!(ssh_no_helper_notice_due(&ssh(true), false));
    assert!(ssh_no_helper_notice_due(&ssh(false), false));
    assert!(!ssh_no_helper_notice_due(&ssh(true), true));
    assert!(!ssh_no_helper_notice_due(&ssh(false), true));
    assert!(!ssh_no_helper_notice_due(&IsLegacySSHSession::No, false));
}

#[test]
fn the_notice_is_taken_once_per_session() {
    let mut shown = HashSet::new();
    let a = SessionId::from(1_u64);
    let b = SessionId::from(2_u64);
    assert!(take_notice_slot(&mut shown, a));
    assert!(!take_notice_slot(&mut shown, a));
    assert!(take_notice_slot(&mut shown, b));
}

#[test]
fn the_notice_text_is_one_line_in_the_localisation_files() {
    for (locale, ftl) in [
        ("en", include_str!("../../../i18n/en/warp.ftl")),
        ("ja", include_str!("../../../i18n/ja/warp.ftl")),
        ("zh-CN", include_str!("../../../i18n/zh-CN/warp.ftl")),
    ] {
        let line = ftl
            .lines()
            .find(|l| l.starts_with("terminal-ssh-no-helper-notice ="))
            .unwrap_or_else(|| panic!("{locale}: key missing"));
        assert!(
            line.len() > "terminal-ssh-no-helper-notice =".len() + 5,
            "{locale}"
        );
    }
}
