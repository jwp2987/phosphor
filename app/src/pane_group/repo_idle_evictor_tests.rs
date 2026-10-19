use std::path::{Path, PathBuf};
use instant::Instant;
use std::time::Duration;

use super::{IdleTracker, is_referenced};

const GRACE: Duration = Duration::from_secs(300);

fn p(s: &str) -> PathBuf {
    PathBuf::from(s)
}

#[test]
fn referenced_when_equal_inside_or_containing() {
    let cwds = [p("/work/repo/src")];
    assert!(is_referenced(Path::new("/work/repo"), cwds.iter()));
    let cwds = [p("/work/repo")];
    assert!(is_referenced(Path::new("/work/repo"), cwds.iter()));
    let cwds = [p("/work")];
    assert!(is_referenced(Path::new("/work/repo"), cwds.iter()));
    // `starts_with` is component-wise: a sibling sharing a name prefix is not a match.
    let cwds = [p("/work/repo-two")];
    assert!(!is_referenced(Path::new("/work/repo"), cwds.iter()));
}

#[test]
fn repo_is_evicted_only_after_the_grace_period() {
    let mut tracker = IdleTracker::default();
    let repo = vec![p("/work/repo")];
    let t0 = Instant::now();

    assert!(tracker.observe(t0, GRACE, &repo, |_| false).is_empty());
    assert!(
        tracker
            .observe(t0 + GRACE - Duration::from_secs(1), GRACE, &repo, |_| false)
            .is_empty()
    );
    assert_eq!(tracker.observe(t0 + GRACE, GRACE, &repo, |_| false), repo);
}

#[test]
fn being_referenced_again_resets_the_idle_clock() {
    let mut tracker = IdleTracker::default();
    let repo = vec![p("/work/repo")];
    let t0 = Instant::now();

    tracker.observe(t0, GRACE, &repo, |_| false);
    // A session comes back just before the deadline...
    let t1 = t0 + GRACE - Duration::from_secs(1);
    assert!(tracker.observe(t1, GRACE, &repo, |_| true).is_empty());
    // ...so the original deadline no longer evicts it; a fresh grace period starts.
    let t2 = t0 + GRACE + Duration::from_secs(1);
    assert!(tracker.observe(t2, GRACE, &repo, |_| false).is_empty());
    assert_eq!(tracker.observe(t2 + GRACE, GRACE, &repo, |_| false), repo);
}

#[test]
fn referenced_repos_are_never_evicted_and_dropped_candidates_are_forgotten() {
    let mut tracker = IdleTracker::default();
    let a = p("/work/a");
    let b = p("/work/b");
    let t0 = Instant::now();

    tracker.observe(t0, GRACE, &[a.clone(), b.clone()], |_| false);
    // `a` stops being a candidate (e.g. removed elsewhere); `b` stays referenced.
    let due = tracker.observe(t0 + GRACE * 2, GRACE, std::slice::from_ref(&b), |path| path == b);
    assert!(due.is_empty());
    // `a` returning later starts from scratch rather than inheriting its old idle time.
    let due = tracker.observe(t0 + GRACE * 3, GRACE, std::slice::from_ref(&a), |_| false);
    assert!(due.is_empty());
}
