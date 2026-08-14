use super::*;

#[test]
fn send_message_persists_a_readable_record() {
    let root = tempfile::tempdir().unwrap();
    let sent = send_message(root.path(), "child-1", "parent-1", "status", "starting up").unwrap();

    assert_eq!(sent.from, "child-1");
    assert_eq!(sent.to, "parent-1");
    assert_eq!(sent.subject, "status");
    assert_eq!(sent.body, "starting up");
    assert!(!sent.message_id.is_empty());

    let listed = list_messages(root.path(), "parent-1", 25).unwrap();
    assert_eq!(listed, vec![sent]);
}

#[test]
fn list_messages_on_empty_mailbox_returns_empty_not_error() {
    let root = tempfile::tempdir().unwrap();
    let listed = list_messages(root.path(), "nobody-has-sent-here", 25).unwrap();
    assert!(listed.is_empty());
}

#[test]
fn list_messages_only_returns_messages_addressed_to_the_requested_run() {
    let root = tempfile::tempdir().unwrap();
    send_message(root.path(), "child-1", "parent-1", "for parent", "body").unwrap();
    send_message(
        root.path(),
        "child-2",
        "someone-else",
        "not for parent",
        "body",
    )
    .unwrap();

    let listed = list_messages(root.path(), "parent-1", 25).unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].subject, "for parent");
}

#[test]
fn list_messages_preserves_send_order() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..5 {
        send_message(
            root.path(),
            "child-1",
            "parent-1",
            &format!("msg-{i}"),
            "body",
        )
        .unwrap();
    }

    let listed = list_messages(root.path(), "parent-1", 25).unwrap();
    let subjects: Vec<_> = listed.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(subjects, vec!["msg-0", "msg-1", "msg-2", "msg-3", "msg-4"]);
}

#[test]
fn list_messages_respects_limit_keeping_the_most_recent() {
    let root = tempfile::tempdir().unwrap();
    for i in 0..5 {
        send_message(
            root.path(),
            "child-1",
            "parent-1",
            &format!("msg-{i}"),
            "body",
        )
        .unwrap();
    }

    let listed = list_messages(root.path(), "parent-1", 2).unwrap();
    let subjects: Vec<_> = listed.iter().map(|m| m.subject.as_str()).collect();
    assert_eq!(subjects, vec!["msg-3", "msg-4"]);
}

#[test]
fn list_messages_skips_malformed_files_instead_of_failing() {
    let root = tempfile::tempdir().unwrap();
    let sent = send_message(root.path(), "child-1", "parent-1", "good", "body").unwrap();

    let dir = inbox_dir(root.path(), "parent-1");
    std::fs::write(dir.join("00000000000000000000-garbage.json"), b"not json").unwrap();

    let listed = list_messages(root.path(), "parent-1", 25).unwrap();
    assert_eq!(listed, vec![sent]);
}

#[test]
fn run_id_with_path_separators_does_not_escape_the_mailbox_root() {
    let root = tempfile::tempdir().unwrap();
    send_message(
        root.path(),
        "child-1",
        "../../etc/passwd",
        "subject",
        "body",
    )
    .unwrap();

    let entries: Vec<_> = std::fs::read_dir(root.path())
        .unwrap()
        .filter_map(|entry| entry.ok())
        .collect();
    assert_eq!(
        entries.len(),
        1,
        "expected exactly one sanitized inbox directory"
    );

    // The sanitized inbox directory must stay inside `root`. Asserted by canonicalising the
    // directory that was actually created and checking containment.
    //
    // This previously read `root/../../etc` and asserted it did not exist. That is
    // `/tmp/<tempdir>/../../etc` = `/etc` whenever TMPDIR is `/tmp` -- so it asserted `/etc`
    // does not exist and failed on any normal machine, while passing only when the temp
    // directory happened to be nested deeper. It also could not have detected a real escape:
    // the path it probed pre-exists, so "created by us" and "already there" were
    // indistinguishable.
    let created = entries[0].path().canonicalize().unwrap();
    let root_canonical = root.path().canonicalize().unwrap();
    assert!(
        created.starts_with(&root_canonical),
        "sanitized inbox escaped the mailbox root: {created:?} is outside {root_canonical:?}"
    );
}

#[test]
#[serial_test::serial(agent_mailbox_root_env)]
fn mailbox_root_env_override_is_honored() {
    // SAFETY: `AGENT_MAILBOX_ROOT_ENV` is process-global; `#[serial]` keeps
    // this from racing other tests that touch the same env var.
    let root = tempfile::tempdir().unwrap();
    unsafe {
        std::env::set_var(AGENT_MAILBOX_ROOT_ENV, root.path());
    }
    let resolved = mailbox_root();
    unsafe {
        std::env::remove_var(AGENT_MAILBOX_ROOT_ENV);
    }
    assert_eq!(resolved, root.path());
}
