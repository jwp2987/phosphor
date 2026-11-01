use remote_server::proto::{RipgrepSearchMatch, RipgrepSearchSubmatch, RipgrepSearchSuccess};
use warp_util::host_id::HostId;
use warp_util::local_or_remote_path::LocalOrRemotePath;

use futures::FutureExt as _;
use futures::channel::{mpsc, oneshot};
use std::cell::RefCell;
use std::rc::Rc;
use warp_ripgrep::search::Match as RipgrepMatch;

use super::{Emit, GlobalSearch, START_BATCH_AFTER_COUNT, pump_matches};

fn host() -> HostId {
    HostId::new("test-host".to_string())
}

fn proto_match(
    path: &str,
    line_number: u32,
    line_text: &str,
    submatches: Vec<(u64, u64)>,
) -> RipgrepSearchMatch {
    RipgrepSearchMatch {
        file_path: path.to_string(),
        line_number,
        line_text: line_text.to_string(),
        submatches: submatches
            .into_iter()
            .map(|(byte_start, byte_end)| RipgrepSearchSubmatch {
                byte_start,
                byte_end,
            })
            .collect(),
    }
}

#[test]
fn remote_matches_become_remote_locations_on_the_host() {
    let success = RipgrepSearchSuccess {
        matches: vec![proto_match(
            "/repo/src/main.rs",
            7,
            "fn main() {}",
            vec![(3, 7)],
        )],
        capped: false,
    };

    let results = GlobalSearch::remote_matches_to_global(&host(), success);

    assert_eq!(results.len(), 1);
    match &results[0].location {
        LocalOrRemotePath::Remote(remote) => {
            assert_eq!(remote.host_id, host());
            assert_eq!(remote.path.as_str(), "/repo/src/main.rs");
        }
        LocalOrRemotePath::Local(_) => panic!("expected a remote location"),
    }
    assert_eq!(results[0].line_number, 7);
    assert_eq!(results[0].column_num, Some(4));
    assert_eq!(results[0].line_text, "fn main() {}");
}

#[test]
fn remote_matches_expand_one_row_per_submatch() {
    let success = RipgrepSearchSuccess {
        matches: vec![proto_match(
            "/repo/a.rs",
            1,
            "foo foo",
            vec![(0, 3), (4, 7)],
        )],
        capped: false,
    };

    let results = GlobalSearch::remote_matches_to_global(&host(), success);

    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|m| m.submatches.len() == 1));
}

#[test]
fn remote_matches_with_invalid_paths_are_dropped() {
    let success = RipgrepSearchSuccess {
        matches: vec![
            proto_match("relative/path.rs", 1, "x", vec![(0, 1)]),
            proto_match("/repo/ok.rs", 2, "x", vec![(0, 1)]),
        ],
        capped: false,
    };

    let results = GlobalSearch::remote_matches_to_global(&host(), success);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].location.display_path(), "/repo/ok.rs");
}

#[test]
fn remote_match_leading_whitespace_is_trimmed_per_submatch() {
    // Leading whitespace before the submatch is trimmed and offsets adjusted,
    // matching local search behavior.
    let success = RipgrepSearchSuccess {
        matches: vec![proto_match("/repo/a.rs", 1, "    foo", vec![(4, 7)])],
        capped: false,
    };

    let results = GlobalSearch::remote_matches_to_global(&host(), success);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].line_text, "foo");
    assert_eq!(results[0].column_num, Some(5));
    assert_eq!(results[0].submatches[0].byte_start.as_usize(), 0);
    assert_eq!(results[0].submatches[0].byte_end.as_usize(), 3);
}

#[test]
fn remote_match_column_counts_characters_not_bytes() {
    let success = RipgrepSearchSuccess {
        matches: vec![proto_match("/repo/a.rs", 1, "€foo", vec![(3, 6)])],
        capped: false,
    };

    let results = GlobalSearch::remote_matches_to_global(&host(), success);

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].column_num, Some(2));
}

fn raw_match(n: u32) -> RipgrepMatch {
    RipgrepMatch {
        file_path: std::path::PathBuf::from("/repo/a.rs"),
        line_number: n,
        line_text: "x".to_string(),
        submatches: vec![warp_ripgrep::search::Submatch {
            byte_start: string_offset::ByteOffset::from(0usize),
            byte_end: string_offset::ByteOffset::from(1usize),
        }],
    }
}

type Log = Rc<RefCell<Vec<String>>>;

fn logging_emit(log: &Log) -> impl FnMut(Emit) -> std::future::Ready<()> {
    let log = log.clone();
    move |e| {
        log.borrow_mut().push(match e {
            Emit::Single(_) => "single".to_string(),
            Emit::Batch(items) => format!("batch{}", items.len()),
        });
        std::future::ready(())
    }
}

/// Drive `fut` once without blocking; returns whether it finished.
fn poll_once<F: std::future::Future + Unpin>(fut: &mut F) -> bool {
    (&mut *fut).now_or_never().is_some()
}

#[test]
fn final_partial_batch_is_shown_on_completion() {
    let (tx, rx) = mpsc::unbounded();
    let total = START_BATCH_AFTER_COUNT + 3;
    for i in 0..total {
        tx.unbounded_send(raw_match(i as u32)).unwrap();
    }
    drop(tx);
    let log: Log = Default::default();
    let n = futures::executor::block_on(pump_matches(
        rx,
        || futures::future::pending::<()>(),
        logging_emit(&log),
    ));
    assert_eq!(n, total);
    let log = log.borrow();
    assert_eq!(log.last().unwrap(), "batch3");
}

#[test]
fn slow_tail_batch_flushes_on_its_deadline_without_another_match() {
    let (tx, rx) = mpsc::unbounded();
    for i in 0..START_BATCH_AFTER_COUNT + 2 {
        tx.unbounded_send(raw_match(i as u32)).unwrap();
    }
    let (fire, deadline) = oneshot::channel::<()>();
    let deadline = Some(deadline);
    let deadline = RefCell::new(deadline);
    let log: Log = Default::default();
    let mut fut = Box::pin(pump_matches(
        rx,
        || deadline.borrow_mut().take().unwrap().map(|_| ()),
        logging_emit(&log),
    ));
    // Stream is open and idle: the two batched matches are pending.
    assert!(!poll_once(&mut fut));
    assert!(!log.borrow().iter().any(|e| e.starts_with("batch")));
    // Deadline fires with no further match: batch is shown.
    fire.send(()).unwrap();
    assert!(!poll_once(&mut fut));
    assert_eq!(log.borrow().last().unwrap(), "batch2");
    drop(tx);
    assert!(poll_once(&mut fut));
    // Nothing is flushed twice.
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| e.starts_with("batch"))
            .count(),
        1
    );
}

#[test]
fn size_flush_cancels_the_pending_deadline() {
    let (tx, rx) = mpsc::unbounded();
    for i in 0..START_BATCH_AFTER_COUNT + super::MAX_BATCH_SIZE {
        tx.unbounded_send(raw_match(i as u32)).unwrap();
    }
    let armed = std::cell::Cell::new(0);
    let log: Log = Default::default();
    let mut fut = Box::pin(pump_matches(
        rx,
        || {
            armed.set(armed.get() + 1);
            futures::future::pending::<()>()
        },
        logging_emit(&log),
    ));
    assert!(!poll_once(&mut fut));
    assert_eq!(
        log.borrow().last().unwrap(),
        &format!("batch{}", super::MAX_BATCH_SIZE)
    );
    assert_eq!(armed.get(), 1);
    drop(tx);
    assert!(poll_once(&mut fut));
    // No empty batch is emitted at the end.
    assert_eq!(
        log.borrow()
            .iter()
            .filter(|e| e.starts_with("batch"))
            .count(),
        1
    );
}

#[test]
fn superseded_search_is_not_current() {
    use super::{ActiveSearch, batch_is_current};
    let mut model = GlobalSearch::new();
    assert!(!batch_is_current(&model, 1));
    model.active_search = Some(ActiveSearch {
        search_id: 2,
        remaining_sources: 1,
        completed_sources: 0,
        local_source_failed: false,
        remote_source_failures: 0,
        total_match_count: 0,
        capped: false,
    });
    assert!(!batch_is_current(&model, 1));
    assert!(batch_is_current(&model, 2));
}
