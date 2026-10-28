use std::io::Write;

use super::*;

fn touch(dir: &Path, name: &str) -> PathBuf {
    let path = dir.join(name);
    File::create(&path).unwrap();
    path
}

fn write_bytes(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).unwrap();
    file.write_all(bytes).unwrap();
}

fn zip_entry_names(zip_path: &Path) -> Vec<String> {
    let file = File::open(zip_path).unwrap();
    let mut archive = zip::ZipArchive::new(file).unwrap();
    (0..archive.len())
        .map(|index| {
            archive
                .by_index(index)
                .unwrap()
                .name()
                .unwrap()
                .into_owned()
        })
        .collect()
}
fn log_state(base_directory: &Path, frontend: LogFrontend, logfile_name: &str) -> LogState {
    LogState::new(
        true,
        base_directory.to_path_buf(),
        logfile_name.to_owned(),
        frontend,
    )
}

#[test]
fn frontend_resolves_directory_and_rotation_policy() {
    let base = PathBuf::from("/tmp/warp-logs");
    let gui = log_state(&base, LogFrontend::Gui, "warp_dev.log");
    let tui = log_state(&base, LogFrontend::Tui, "warp_dev.log");
    let cli = log_state(&base, LogFrontend::Cli, "warp_dev.log");

    assert_eq!(gui.log_directory, base);
    assert_eq!(gui.max_rotation, MAX_FILES_IN_GUI_ROTATION);
    assert_eq!(tui.log_directory, PathBuf::from("/tmp/warp-logs/warp-cli"));
    assert_eq!(tui.max_rotation, MAX_FILES_IN_CLI_ROTATION);
    assert_eq!(cli.log_directory, PathBuf::from("/tmp/warp-logs/oz"));
    assert_eq!(cli.max_rotation, MAX_FILES_IN_CLI_ROTATION);
}

#[test]
fn tui_bundle_uses_resolved_state_and_ignores_legacy_oz_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let state = log_state(tmp.path(), LogFrontend::Tui, "warp_preview.log");
    fs::create_dir(&state.log_directory).unwrap();
    write_bytes(
        &state.log_directory.join("warp_preview.log"),
        b"active tui session",
    );
    write_bytes(
        &state.log_directory.join("warp_preview.log.old.0"),
        b"previous tui session",
    );
    write_bytes(
        &state.log_directory.join("warp_preview.log.in_session.0"),
        b"mid-session tui chunk",
    );
    let legacy = tmp.path().join("oz");
    fs::create_dir(&legacy).unwrap();
    touch(&legacy, "warp_preview.log");
    let zip_path = state.create_log_bundle_zip().unwrap();

    assert_eq!(zip_path.parent(), Some(state.log_directory.as_path()));
    assert_eq!(
        zip_entry_names(&zip_path),
        vec![
            "warp_preview.log",
            "warp_preview.log.in_session.0",
            "warp_preview.log.old.0",
        ]
    );
}

#[test]
fn collects_active_in_session_and_old_logs_in_expected_order() {
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), "warp.log");
    let in_session_0 = touch(tmp.path(), "warp.log.in_session.0");
    let in_session_1 = touch(tmp.path(), "warp.log.in_session.1");
    let in_session_2 = touch(tmp.path(), "warp.log.in_session.2");
    let old_0 = touch(tmp.path(), "warp.log.old.0");
    let old_1 = touch(tmp.path(), "warp.log.old.1");

    let paths = collect_log_paths_in(tmp.path(), "warp.log").unwrap();

    assert_eq!(
        paths,
        vec![
            active,
            in_session_0,
            in_session_1,
            in_session_2,
            old_0,
            old_1
        ]
    );
}

#[test]
fn includes_in_session_logs_even_when_no_active_or_old_logs_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let in_session_0 = touch(tmp.path(), "warp.log.in_session.0");

    let paths = collect_log_paths_in(tmp.path(), "warp.log").unwrap();

    assert_eq!(paths, vec![in_session_0]);
}

#[test]
fn ignores_unrelated_files_and_malformed_suffixes() {
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), "warp.log");
    touch(tmp.path(), "warp.log.in_session.abc"); // not a number
    touch(tmp.path(), "warp.log.in_session."); // empty suffix
    touch(tmp.path(), "warp.log.old.xyz"); // not a number
    touch(tmp.path(), "other.log"); // unrelated
    touch(tmp.path(), "warp.log.old.temp"); // matches old. prefix but non-numeric

    let paths = collect_log_paths_in(tmp.path(), "warp.log").unwrap();

    assert_eq!(paths, vec![active]);
}

#[test]
fn errors_when_directory_is_empty() {
    let tmp = tempfile::tempdir().unwrap();
    let err = collect_log_paths_in(tmp.path(), "warp.log").unwrap_err();
    assert!(err.to_string().contains("No warp logs were found"));
}

#[test]
fn respects_channel_specific_logfile_name() {
    // Beta/preview channels use a different base name; make sure scanning
    // is gated on that name and doesn't pick up the wrong channel's files.
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), "warp_preview.log");
    let in_session_0 = touch(tmp.path(), "warp_preview.log.in_session.0");
    touch(tmp.path(), "warp.log"); // different channel — must be ignored
    touch(tmp.path(), "warp.log.in_session.0");

    let paths = collect_log_paths_in(tmp.path(), "warp_preview.log").unwrap();

    assert_eq!(paths, vec![active, in_session_0]);
}

#[test]
fn interleaves_old_logs_with_their_nested_in_session_chunks() {
    // Previous sessions' mid-rotation chunks are nested under their
    // parent .old.K slot; collection should output each .old.K
    // immediately followed by its .old.K.in_session.M chunks before
    // moving on to .old.{K+1}.
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), "warp.log");
    let cur_in_session_0 = touch(tmp.path(), "warp.log.in_session.0");
    let old_0 = touch(tmp.path(), "warp.log.old.0");
    let old_0_chunk_0 = touch(tmp.path(), "warp.log.old.0.in_session.0");
    let old_0_chunk_1 = touch(tmp.path(), "warp.log.old.0.in_session.1");
    let old_1 = touch(tmp.path(), "warp.log.old.1");
    let old_1_chunk_0 = touch(tmp.path(), "warp.log.old.1.in_session.0");

    let paths = collect_log_paths_in(tmp.path(), "warp.log").unwrap();

    assert_eq!(
        paths,
        vec![
            active,
            cur_in_session_0,
            old_0,
            old_0_chunk_0,
            old_0_chunk_1,
            old_1,
            old_1_chunk_0,
        ]
    );
}

#[test]
fn nested_chunks_surface_even_when_parent_old_slot_is_missing() {
    // If a .old.K slot is missing from disk but its nested chunks
    // remain (e.g. truncated by manual cleanup), the chunks should
    // still be bundled rather than silently dropped.
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), "warp.log");
    let orphan_chunk = touch(tmp.path(), "warp.log.old.3.in_session.0");

    let paths = collect_log_paths_in(tmp.path(), "warp.log").unwrap();

    assert_eq!(paths, vec![active, orphan_chunk]);
}

#[test]
fn migrate_previous_session_renames_in_session_to_old_0_in_session() {
    // Mid-session chunks from the previous session belong with the
    // .old.0 slot that holds that session's final-state log.
    let tmp = tempfile::tempdir().unwrap();
    touch(tmp.path(), "warp.log.in_session.0");
    touch(tmp.path(), "warp.log.in_session.1");
    touch(tmp.path(), "warp.log.in_session.2");

    migrate_previous_session_in_session_chunks(tmp.path(), "warp.log");

    assert!(tmp.path().join("warp.log.old.0.in_session.0").is_file());
    assert!(tmp.path().join("warp.log.old.0.in_session.1").is_file());
    assert!(tmp.path().join("warp.log.old.0.in_session.2").is_file());
    // Bare .in_session.* slots are free for the new session.
    assert!(!tmp.path().join("warp.log.in_session.0").exists());
    assert!(!tmp.path().join("warp.log.in_session.1").exists());
    assert!(!tmp.path().join("warp.log.in_session.2").exists());
}

#[test]
fn migrate_previous_session_is_a_noop_when_no_in_session_chunks_exist() {
    let tmp = tempfile::tempdir().unwrap();
    let unrelated = touch(tmp.path(), "warp.log");

    migrate_previous_session_in_session_chunks(tmp.path(), "warp.log");

    // Active log untouched; no spurious .old.0.in_session.* files.
    assert!(unrelated.is_file());
    let any_nested = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("warp.log.old.0.in_session.")
        });
    assert!(!any_nested);
}

#[test]
fn migrate_previous_session_ignores_malformed_in_session_filenames() {
    let tmp = tempfile::tempdir().unwrap();
    let real = touch(tmp.path(), "warp.log.in_session.0");
    let bogus_a = touch(tmp.path(), "warp.log.in_session.abc");
    let bogus_b = touch(tmp.path(), "warp.log.in_session.");
    let unrelated = touch(tmp.path(), "warp.log.in_session.0.weird"); // not a usize

    migrate_previous_session_in_session_chunks(tmp.path(), "warp.log");

    assert!(!real.exists()); // moved
    assert!(tmp.path().join("warp.log.old.0.in_session.0").is_file());
    // Malformed entries are left where they are.
    assert!(bogus_a.is_file());
    assert!(bogus_b.is_file());
    assert!(unrelated.is_file());
}

#[test]
fn shift_nested_chunks_renames_old_n_in_session_to_old_n_plus_1() {
    let tmp = tempfile::tempdir().unwrap();
    touch(tmp.path(), "warp.log.old.0.in_session.0");
    touch(tmp.path(), "warp.log.old.0.in_session.1");

    shift_old_session_in_session_chunks(tmp.path(), "warp.log", 0);

    assert!(tmp.path().join("warp.log.old.1.in_session.0").is_file());
    assert!(tmp.path().join("warp.log.old.1.in_session.1").is_file());
    assert!(!tmp.path().join("warp.log.old.0.in_session.0").exists());
    assert!(!tmp.path().join("warp.log.old.0.in_session.1").exists());
}

#[test]
fn shift_nested_chunks_only_touches_the_requested_slot() {
    // Shifting slot 0 must not disturb slot 1's nested chunks.
    let tmp = tempfile::tempdir().unwrap();
    touch(tmp.path(), "warp.log.old.0.in_session.0");
    let slot1_chunk = touch(tmp.path(), "warp.log.old.1.in_session.0");

    shift_old_session_in_session_chunks(tmp.path(), "warp.log", 0);

    assert!(tmp.path().join("warp.log.old.1.in_session.0").is_file());
    assert_eq!(tmp.path().join("warp.log.old.1.in_session.0"), slot1_chunk);
}

#[test]
fn remove_nested_chunks_deletes_every_chunk_of_the_target_slot() {
    let tmp = tempfile::tempdir().unwrap();
    touch(tmp.path(), "warp.log.old.4.in_session.0");
    touch(tmp.path(), "warp.log.old.4.in_session.1");
    let survivor = touch(tmp.path(), "warp.log.old.3.in_session.0");

    remove_old_session_in_session_chunks(tmp.path(), "warp.log", 4);

    assert!(!tmp.path().join("warp.log.old.4.in_session.0").exists());
    assert!(!tmp.path().join("warp.log.old.4.in_session.1").exists());
    // Other slots' chunks are untouched.
    assert!(survivor.is_file());
}

#[test]
fn resolved_active_paths_use_frontend_directory_and_channel_name() {
    let base = PathBuf::from("/tmp/warp-logs");
    let gui = log_state(&base, LogFrontend::Gui, "warp_dev.log");
    let tui = log_state(&base, LogFrontend::Tui, "warp_local.log");
    let cli = log_state(&base, LogFrontend::Cli, "warp_preview.log");

    assert_eq!(gui.log_file_path(), base.join("warp_dev.log"));
    assert_eq!(
        tui.log_file_path(),
        base.join("warp-cli").join("warp_local.log")
    );
    assert_eq!(
        cli.log_file_path(),
        base.join("oz").join("warp_preview.log")
    );
}

#[test]
fn crash_recovery_paths_use_channel_name_in_tui_directory() {
    let tui_dir = PathBuf::from("/tmp/warp-logs/warp-cli");
    assert_eq!(
        temp_log_file_path(&tui_dir, "warp_dev.log"),
        tui_dir.join("warp_dev.log.old.temp")
    );
    assert_eq!(
        crash_recovery_process_log_file_path(&tui_dir, "warp_dev.log"),
        tui_dir.join("warp_dev.log.recovery")
    );
}

// --- #1071: a second process must never rename, truncate or rotate the live
// owner's log. Each `setup_log_files_for_current_execution` call below opens
// its own handle on `<name>.lock`, so the OS lock contends exactly as it does
// between two processes (flock on Unix, LockFileEx on Windows).

const LOG: &str = "phosphor.log";

fn read(dir: &Path, name: &str) -> String {
    fs::read_to_string(dir.join(name)).unwrap()
}

fn owner_setup(dir: &Path) -> LogFileSetup {
    let setup = setup_log_files_for_current_execution(dir, LOG, false).unwrap();
    assert!(setup.ownership.is_owner(), "first process must own the log");
    assert_eq!(setup.path, dir.join(LOG));
    setup
}

#[test]
fn second_process_does_not_rename_or_truncate_the_live_owner_log() {
    let tmp = tempfile::tempdir().unwrap();
    let mut owner = owner_setup(tmp.path());
    owner.file.write_all(b"owner line 1\n").unwrap();

    let mut second = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();

    assert!(!second.ownership.is_owner());
    assert_eq!(
        second.path,
        tmp.path()
            .join(format!("{LOG}.secondary.{}", std::process::id()))
    );
    second.file.write_all(b"second line\n").unwrap();
    // The owner keeps writing after the second process started.
    owner.file.write_all(b"owner line 2\n").unwrap();

    assert_eq!(read(tmp.path(), LOG), "owner line 1\nowner line 2\n");
    assert!(!tmp.path().join(format!("{LOG}.old.temp")).exists());
    assert_eq!(fs::read_to_string(&second.path).unwrap(), "second line\n");
}

#[test]
fn second_process_rotation_does_not_move_the_owner_files() {
    let tmp = tempfile::tempdir().unwrap();
    write_bytes(&tmp.path().join(format!("{LOG}.old.0")), b"older session");
    let mut owner = owner_setup(tmp.path());
    owner.file.write_all(b"owner live").unwrap();

    // The second process's background rotation, with the ownership its own
    // startup determined.
    let second = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();
    let rotated = rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &second.ownership,
    );

    assert!(!rotated);
    assert_eq!(read(tmp.path(), LOG), "owner live");
    assert_eq!(read(tmp.path(), &format!("{LOG}.old.0")), "older session");
    assert!(!tmp.path().join(format!("{LOG}.old.1")).exists());
}

#[test]
fn owner_session_log_survives_repeated_second_launches_and_rotates_after_exit() {
    // The #1071 sequence: A runs; B and C launch (and forward) while A is
    // alive; A exits; D starts. A's whole log must end up in `.old.0`.
    let tmp = tempfile::tempdir().unwrap();
    let mut a = owner_setup(tmp.path());
    a.file.write_all(b"A: whole session\n").unwrap();

    let b = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();
    drop(b);
    let c = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();
    drop(c);
    a.file.write_all(b"A: graceful quit\n").unwrap();
    drop(a);

    let d = owner_setup(tmp.path());
    assert!(rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &d.ownership
    ));

    assert_eq!(
        read(tmp.path(), &format!("{LOG}.old.0")),
        "A: whole session\nA: graceful quit\n"
    );
    assert!(!tmp.path().join(format!("{LOG}.old.1")).exists());
    assert_eq!(read(tmp.path(), LOG), "");
    drop(d);
}

#[test]
fn normal_start_after_clean_exit_still_rotates() {
    let tmp = tempfile::tempdir().unwrap();
    write_bytes(&tmp.path().join(LOG), b"previous");
    write_bytes(&tmp.path().join(format!("{LOG}.old.0")), b"older");

    let setup = owner_setup(tmp.path());
    assert!(rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &setup.ownership
    ));

    assert_eq!(read(tmp.path(), LOG), "");
    assert_eq!(read(tmp.path(), &format!("{LOG}.old.0")), "previous");
    assert_eq!(read(tmp.path(), &format!("{LOG}.old.1")), "older");
    assert!(!tmp.path().join(format!("{LOG}.old.temp")).exists());
    drop(setup);
}

#[test]
fn crash_recovery_child_takes_over_the_crashed_parent_log() {
    let tmp = tempfile::tempdir().unwrap();
    let mut parent = owner_setup(tmp.path());
    parent.file.write_all(b"parent before crash").unwrap();

    let mut child = setup_log_files_for_current_execution(tmp.path(), LOG, true).unwrap();
    assert!(!child.ownership.is_owner());
    assert_eq!(child.path, tmp.path().join(format!("{LOG}.recovery")));
    assert_eq!(read(tmp.path(), LOG), "parent before crash");
    child.file.write_all(b"child").unwrap();

    // The parent crashes: the OS releases its lock.
    drop(parent);
    let active = take_over_log_after_parent_crash(tmp.path(), LOG);

    assert!(active.ownership.is_owner());
    assert_eq!(active.path, tmp.path().join(LOG));
    assert_eq!(read(tmp.path(), LOG), "child");
    assert!(!tmp.path().join(format!("{LOG}.recovery")).exists());
    assert!(rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &active.ownership
    ));
    assert_eq!(
        read(tmp.path(), &format!("{LOG}.old.0")),
        "parent before crash"
    );
}

#[test]
fn crash_recovery_child_leaves_a_live_owner_alone() {
    // The crashed parent was not the owner: another live process still holds
    // the lock and its log must not move.
    let tmp = tempfile::tempdir().unwrap();
    let mut owner = owner_setup(tmp.path());
    owner.file.write_all(b"owner").unwrap();
    let mut child = setup_log_files_for_current_execution(tmp.path(), LOG, true).unwrap();
    child.file.write_all(b"child").unwrap();

    let active = take_over_log_after_parent_crash(tmp.path(), LOG);

    assert!(!active.ownership.is_owner());
    assert_eq!(read(tmp.path(), LOG), "owner");
    assert!(!tmp.path().join(format!("{LOG}.old.temp")).exists());
    assert_eq!(fs::read_to_string(&active.path).unwrap(), "child");
    assert_ne!(active.path, tmp.path().join(LOG));
}

#[test]
fn secondary_logs_are_pruned_to_a_bound() {
    let tmp = tempfile::tempdir().unwrap();
    let base = std::time::SystemTime::now() - Duration::from_secs(3600);
    for pid in 1..=8u32 {
        let path = tmp.path().join(format!("{LOG}.secondary.{pid}"));
        let file = File::create(&path).unwrap();
        file.set_modified(base + Duration::from_secs(u64::from(pid)))
            .unwrap();
    }
    let unrelated = touch(tmp.path(), &format!("{LOG}.secondary.notapid"));
    let own = touch(tmp.path(), &format!("{LOG}.secondary.999"));
    File::options()
        .write(true)
        .open(&own)
        .unwrap()
        .set_modified(base)
        .unwrap();

    prune_secondary_logs(tmp.path(), LOG, MAX_SECONDARY_LOGS, Some(&own));

    let mut remaining: Vec<u32> = fs::read_dir(tmp.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|e| {
            e.file_name()
                .to_str()?
                .strip_prefix(&format!("{LOG}.secondary."))?
                .parse()
                .ok()
        })
        .collect();
    remaining.sort_unstable();
    // The excluded (own, oldest) file survives and counts towards the bound;
    // the newest others fill the rest.
    assert_eq!(remaining, vec![5, 6, 7, 8, 999]);
    assert!(unrelated.is_file());
}

#[test]
fn bundle_includes_secondary_logs_after_the_owner_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let active = touch(tmp.path(), LOG);
    let old_0 = touch(tmp.path(), &format!("{LOG}.old.0"));
    let secondary = touch(tmp.path(), &format!("{LOG}.secondary.42"));
    touch(tmp.path(), &format!("{LOG}.lock"));

    let paths = collect_log_paths_in(tmp.path(), LOG).unwrap();

    assert_eq!(paths, vec![active, old_0, secondary]);
}

/// The numeric `<LOG>.secondary.<pid>` files in `dir`, sorted.
fn secondary_pids(dir: &Path) -> Vec<u32> {
    let prefix = format!("{LOG}.secondary.");
    let mut pids: Vec<u32> = fs::read_dir(dir)
        .unwrap()
        .filter_map(Result::ok)
        .filter_map(|e| e.file_name().to_str()?.strip_prefix(&prefix)?.parse().ok())
        .collect();
    pids.sort_unstable();
    pids
}

/// Leaves `count` stale secondaries (pids 1..=count, oldest first) in `dir`,
/// as earlier forwarded launches would have.
fn stale_secondaries(dir: &Path, count: u32) {
    let base = std::time::SystemTime::now() - Duration::from_secs(3600);
    for pid in 1..=count {
        File::create(dir.join(format!("{LOG}.secondary.{pid}")))
            .unwrap()
            .set_modified(base + Duration::from_secs(u64::from(pid)))
            .unwrap();
    }
}

// #1088: the bound is enforced on the real path a forwarded launch takes, not
// only by `prune_secondary_logs` called directly. Fewer than
// `MAX_SECONDARY_LOGS` files (the 3 the report saw) are kept by design.
#[test]
fn forwarded_launch_keeps_at_most_the_bound_of_secondary_logs() {
    let tmp = tempfile::tempdir().unwrap();
    let owner = owner_setup(tmp.path());
    stale_secondaries(tmp.path(), 8);

    let second = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();

    assert!(!second.ownership.is_owner());
    let pids = secondary_pids(tmp.path());
    assert_eq!(pids.len(), MAX_SECONDARY_LOGS, "got {pids:?}");
    assert!(pids.contains(&std::process::id()), "own log kept: {pids:?}");
    // The newest of the stale ones fill the rest.
    assert!(pids.contains(&8) && pids.contains(&5) && !pids.contains(&4));
    drop(owner);
}

#[test]
fn forwarded_launches_below_the_bound_keep_every_secondary_log() {
    let tmp = tempfile::tempdir().unwrap();
    let owner = owner_setup(tmp.path());
    stale_secondaries(tmp.path(), 2);

    let _second = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();

    let mut expected = vec![1, 2, std::process::id()];
    expected.sort_unstable();
    assert_eq!(secondary_pids(tmp.path()), expected);
    drop(owner);
}

#[test]
fn owner_rotation_prunes_secondary_logs_and_a_non_owner_does_not() {
    let tmp = tempfile::tempdir().unwrap();
    stale_secondaries(tmp.path(), 9);
    let owner = owner_setup(tmp.path());
    let second = setup_log_files_for_current_execution(tmp.path(), LOG, false).unwrap();
    // The forwarded launch already pruned to the bound; refill past it.
    stale_secondaries(tmp.path(), 9);

    assert!(!rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &second.ownership
    ));
    assert_eq!(secondary_pids(tmp.path()).len(), 10);

    assert!(rotate_files_in(
        tmp.path(),
        LOG,
        MAX_FILES_IN_GUI_ROTATION,
        &owner.ownership
    ));
    assert_eq!(secondary_pids(tmp.path()).len(), MAX_SECONDARY_LOGS);
    drop(owner);
}
