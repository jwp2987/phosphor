use std::path::PathBuf;
use std::time::SystemTime;

use super::*;

fn stamp(len: u64) -> BranchStamp {
    BranchStamp::from_entries(vec![(
        PathBuf::from("/r/.git/HEAD"),
        Some((SystemTime::UNIX_EPOCH, len)),
    )])
}

fn temp_repo(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("phosphor-{name}-{}", std::process::id()));
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(dir.join(".git").join("refs").join("heads")).unwrap();
    std::fs::write(dir.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
    dir
}

/// #1342: the second ask under an unchanged stamp is answered from the cache (no git); on old
/// code there was no cache, so every Enter spawned `git branch`.
#[test]
fn unchanged_stamp_is_a_hit() {
    let id = SessionId::from(9001);
    store(id, "/r", stamp(3), vec!["* main".into()]);
    assert_eq!(
        lookup(id, "/r", &stamp(3)),
        Some(vec!["* main".to_string()])
    );
}

/// #1342: a moved HEAD / changed refs is a miss, so a new or switched branch is never stale.
#[test]
fn changed_stamp_is_a_miss() {
    let id = SessionId::from(9002);
    store(id, "/r", stamp(3), vec!["* main".into()]);
    assert_eq!(lookup(id, "/r", &stamp(4)), None);
}

#[test]
fn other_session_or_dir_is_a_miss() {
    store(SessionId::from(9003), "/r", stamp(3), vec!["* main".into()]);
    assert_eq!(lookup(SessionId::from(9004), "/r", &stamp(3)), None);
    assert_eq!(lookup(SessionId::from(9003), "/r/sub", &stamp(3)), None);
}

/// #1342: creating a branch (even nested, `feature/x`) or moving HEAD changes the real stamp.
#[test]
fn stamp_changes_on_branch_create_and_head_move() {
    let repo = temp_repo("branch-stamp");
    let pwd = repo.to_string_lossy().into_owned();
    let heads = repo.join(".git").join("refs").join("heads");
    let base = branch_stamp(&pwd).expect("a plain repo can be stamped");
    assert_eq!(
        branch_stamp(&pwd),
        Some(base.clone()),
        "stable when nothing changes"
    );

    std::fs::create_dir_all(heads.join("feature")).unwrap();
    std::fs::write(heads.join("feature").join("x"), "abc\n").unwrap();
    let with_branch = branch_stamp(&pwd).unwrap();
    assert_ne!(with_branch, base, "a new nested branch changes the stamp");

    std::fs::write(
        repo.join(".git").join("HEAD"),
        "ref: refs/heads/feature/x\n",
    )
    .unwrap();
    assert_ne!(
        branch_stamp(&pwd).unwrap(),
        with_branch,
        "HEAD moving changes it"
    );
    std::fs::remove_dir_all(&repo).ok();
}

/// No repository above the directory: no stamp, so the caller runs git as before.
#[test]
fn no_repo_means_no_stamp() {
    let dir = std::env::temp_dir().join(format!("phosphor-norepo-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let got = branch_stamp(&dir.to_string_lossy());
    std::fs::remove_dir_all(&dir).ok();
    // A stray .git in a temp ancestor would make this Some; only assert the common case.
    if !std::env::temp_dir()
        .ancestors()
        .any(|d| d.join(".git").exists())
    {
        assert_eq!(got, None);
    }
}

/// #1371: a watcher refs event (epoch bump) invalidates a cached list even when every file stamp
/// (mtime, len) is identical. On the old file-only stamp this was a hit and served a stale list.
#[test]
fn refs_epoch_change_is_a_miss_with_identical_file_stamps() {
    let id = SessionId::from(9005);
    store(id, "/r", stamp(3).with_epoch(7), vec!["* main".into()]);
    assert_eq!(
        lookup(id, "/r", &stamp(3).with_epoch(7)),
        Some(vec!["* main".to_string()])
    );
    assert_eq!(lookup(id, "/r", &stamp(3).with_epoch(8)), None);
}

/// #1382: a full cache evicts only the oldest entry; it used to clear every session's list.
#[test]
fn full_cache_evicts_only_the_oldest_entry() {
    let mut cache: CacheMap = HashMap::new();
    let id = SessionId::from(9100);
    for i in 0..MAX_CACHED_ENTRIES {
        store_in(&mut cache, (id, format!("/d{i}")), stamp(1), vec![]);
    }
    assert_eq!(cache.len(), MAX_CACHED_ENTRIES);
    store_in(&mut cache, (id, "/new".to_string()), stamp(1), vec![]);
    assert_eq!(cache.len(), MAX_CACHED_ENTRIES);
    assert!(cache.contains_key(&(id, "/new".to_string())));
    assert!(
        !cache.contains_key(&(id, "/d0".to_string())),
        "oldest evicted"
    );
    assert!(
        cache.contains_key(&(id, "/d1".to_string())),
        "the rest are kept"
    );
}
