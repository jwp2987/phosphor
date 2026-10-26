use std::collections::HashSet;

use super::{parse_proc_stat_group_and_session, target_groups};

#[test]
fn parses_group_and_session() {
    let stat = "1234 (ssh) S 1200 1300 1200 34816 1300 4194304 100 0 0 0 0 0 0 0 20 0 1 0 5 1 2";
    assert_eq!(parse_proc_stat_group_and_session(stat), Some((1300, 1200)));
}

#[test]
fn comm_may_contain_spaces_and_parentheses() {
    let stat = "42 (my (odd) prog) R 7 99 55 0 -1";
    assert_eq!(parse_proc_stat_group_and_session(stat), Some((99, 55)));
}

#[test]
fn malformed_stat_is_none() {
    assert_eq!(parse_proc_stat_group_and_session(""), None);
    assert_eq!(parse_proc_stat_group_and_session("1 (x"), None);
    assert_eq!(parse_proc_stat_group_and_session("1 (x) S 1 2"), None);
}

#[test]
fn targets_shell_group_and_job_groups_deduplicated() {
    let session_groups = HashSet::from([500, 700, 700, 800]);
    assert_eq!(
        target_groups(500, session_groups, 9999),
        vec![500, 700, 800]
    );
}

#[test]
fn never_targets_own_group_or_init() {
    let session_groups = HashSet::from([0, 1, 42, 700]);
    assert_eq!(target_groups(500, session_groups, 42), vec![500, 700]);
}

#[test]
fn shell_group_is_targeted_without_a_proc_scan() {
    assert_eq!(target_groups(500, HashSet::new(), 9999), vec![500]);
}
