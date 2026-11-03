use super::*;
use std::os::unix::fs::MetadataExt as _;

fn fake_proc(entries: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    for (pid, comm) in entries {
        let proc_dir = dir.path().join(pid);
        fs::create_dir(&proc_dir).unwrap();
        fs::write(proc_dir.join("comm"), format!("{comm}\n")).unwrap();
    }
    dir
}

fn my_uid(dir: &tempfile::TempDir) -> u32 {
    fs::metadata(dir.path()).unwrap().uid()
}

#[test]
fn finds_sway_and_hyprland_by_comm() {
    let dir = fake_proc(&[("100", "bash"), ("200", "sway"), ("300", "Hyprland")]);
    // Lowest pid wins, like the `ps | grep -m 1` pipeline this replaced.
    assert_eq!(
        find_compositor_in_proc(dir.path(), my_uid(&dir)).as_deref(),
        Some("sway")
    );
    let dir = fake_proc(&[("100", "bash"), ("300", "Hyprland")]);
    // The match keeps the case `ps` printed.
    assert_eq!(
        find_compositor_in_proc(dir.path(), my_uid(&dir)).as_deref(),
        Some("Hyprland")
    );
}

#[test]
fn pids_are_ordered_numerically_not_lexically() {
    let dir = fake_proc(&[("9", "swayidle"), ("10", "Hyprland")]);
    assert_eq!(
        find_compositor_in_proc(dir.path(), my_uid(&dir)).as_deref(),
        Some("sway")
    );
}

#[test]
fn ignores_other_users_non_pid_entries_and_unrelated_processes() {
    let dir = fake_proc(&[("100", "bash"), ("200", "kwin_x11")]);
    fs::create_dir(dir.path().join("self")).unwrap();
    fs::write(dir.path().join("self").join("comm"), "sway\n").unwrap();
    assert_eq!(find_compositor_in_proc(dir.path(), my_uid(&dir)), None);

    let dir = fake_proc(&[("200", "sway")]);
    assert_eq!(find_compositor_in_proc(dir.path(), my_uid(&dir) + 1), None);
}

#[test]
fn missing_proc_root_is_none() {
    assert_eq!(
        find_compositor_in_proc(path::Path::new("/nonexistent/proc"), 0),
        None
    );
}

#[test]
fn uid_comes_from_status_not_directory_owner() {
    // A non-dumpable process has a root-owned /proc dir; `status` still names the real user.
    let dir = fake_proc(&[("200", "sway")]);
    let uid = my_uid(&dir);
    fs::write(
        dir.path().join("200").join("status"),
        format!("Name:\tsway\nUid:\t{uid}\t{uid}\t{uid}\t{uid}\n"),
    )
    .unwrap();
    assert_eq!(
        find_compositor_in_proc(dir.path(), uid).as_deref(),
        Some("sway")
    );
    assert_eq!(find_compositor_in_proc(dir.path(), uid + 1), None);

    // The effective uid (second field) decides, as with `ps -u`.
    fs::write(
        dir.path().join("200").join("status"),
        format!("Uid:\t0\t{uid}\t0\t{uid}\n"),
    )
    .unwrap();
    assert_eq!(
        find_compositor_in_proc(dir.path(), uid).as_deref(),
        Some("sway")
    );
}
