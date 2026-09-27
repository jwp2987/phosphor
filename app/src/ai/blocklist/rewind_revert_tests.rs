use std::collections::HashMap;
use std::rc::{Rc, Weak};

use super::*;

// ── A simulated rewind ───────────────────────────────────────────────────
//
// `RevertSequence` is driven here exactly as `TerminalView` drives it — start,
// then one `settled` per write outcome — against a simulated disk whose writes
// are guarded the way `FileModel::save_if_unchanged` guards them: a revert
// lands only if the file holds exactly the text its edit's accept wrote.
// Writes complete only when the test says so, so "in flight" is observable.

/// One edit's revert: put `base` back, if the file still holds `accepted`.
#[derive(Debug)]
struct Revert {
    name: &'static str,
    file: &'static str,
    accepted: &'static str,
    base: &'static str,
}

fn revert(
    name: &'static str,
    file: &'static str,
    accepted: &'static str,
    base: &'static str,
) -> (Option<&'static str>, Revert) {
    (
        Some(file),
        Revert {
            name,
            file,
            accepted,
            base,
        },
    )
}

#[derive(Default)]
struct Rewind {
    disk: HashMap<&'static str, String>,
    /// Dispatched, in dispatch order, not yet completed.
    in_flight: Vec<&'static str>,
    /// Every revert ever dispatched.
    dispatched: Vec<&'static str>,
    /// Reverts that landed — what the card, and so the backup, records.
    landed: Vec<&'static str>,
    /// Reverts refused by the guard — each one a toast.
    refused: Vec<&'static str>,
    /// Reverts never attempted because a newer one of their file failed.
    abandoned: Vec<&'static str>,
}

impl Rewind {
    fn dispatch(&mut self, revert: &Revert) -> RevertStart {
        self.in_flight.push(revert.name);
        self.dispatched.push(revert.name);
        RevertStart::InFlight
    }

    /// Completes the in-flight write `name` against the disk and feeds the
    /// outcome back, as the `RevertWriteSettled` subscription does.
    fn complete(
        &mut self,
        sequence: &mut RevertSequence<Revert>,
        name: &'static str,
        reverts: &[Revert],
    ) {
        let position = self
            .in_flight
            .iter()
            .position(|n| *n == name)
            .unwrap_or_else(|| panic!("{name} is not in flight"));
        self.in_flight.remove(position);

        let revert = reverts.iter().find(|r| r.name == name).unwrap();
        let on_disk = self.disk.get(revert.file).cloned().unwrap_or_default();
        let landed = on_disk == revert.accepted;
        if landed {
            self.disk.insert(revert.file, revert.base.to_owned());
            self.landed.push(name);
        } else {
            self.refused.push(name);
        }

        let abandoned = sequence
            .settled(|r| r.name == name, landed, |r| self.dispatch(r))
            .expect("the sequence was waiting on this write");
        self.abandoned.extend(abandoned.into_iter().map(|r| r.name));
    }
}

/// The reverts in `units`, for looking up by name after they move into the
/// sequence.
fn catalogue(units: &[(Option<&'static str>, Revert)]) -> Vec<Revert> {
    units
        .iter()
        .map(|(_, r)| Revert {
            name: r.name,
            file: r.file,
            accepted: r.accepted,
            base: r.base,
        })
        .collect()
}

const ORIGINAL: &str = "v0";
const FIRST: &str = "v1";
const SECOND: &str = "v2";

/// The regression (#686): the agent edited one file twice and both edits were
/// accepted. Rewinding past both must leave the original, with both reverts
/// landed and neither refused. Dispatched together, the older revert probed
/// the file while it still held the newer edit and was refused.
#[test]
fn two_accepted_edits_to_one_file_revert_to_the_original() {
    let units = vec![
        revert("newer", "a.rs", SECOND, FIRST),
        revert("older", "a.rs", FIRST, ORIGINAL),
    ];
    let reverts = catalogue(&units);
    let mut rewind = Rewind::default();
    rewind.disk.insert("a.rs", SECOND.to_owned());
    let mut sequence = RevertSequence::new(units);

    assert!(sequence.start(|r| rewind.dispatch(r)).is_empty());
    assert_eq!(
        rewind.in_flight,
        ["newer"],
        "the older revert must wait for the newer one"
    );

    rewind.complete(&mut sequence, "newer", &reverts);
    assert_eq!(
        rewind.in_flight,
        ["older"],
        "dispatched once the newer landed"
    );
    assert!(!sequence.is_settled());

    rewind.complete(&mut sequence, "older", &reverts);
    assert!(sequence.is_settled());
    assert_eq!(rewind.disk["a.rs"], ORIGINAL);
    assert_eq!(rewind.landed, ["newer", "older"]);
    assert!(rewind.refused.is_empty(), "refused: {:?}", rewind.refused);
}

/// The user edited the file after the newer accept. The newer revert is
/// refused (one toast), the older one is never attempted — it would assert
/// text the file no longer holds — and the user's work is untouched. Nothing
/// landed, so nothing is recorded as reverted.
#[test]
fn a_refused_newer_revert_stops_the_older_ones_for_that_file() {
    let users_work = "v2 + my own work";
    let units = vec![
        revert("newer", "a.rs", SECOND, FIRST),
        revert("older", "a.rs", FIRST, ORIGINAL),
    ];
    let reverts = catalogue(&units);
    let mut rewind = Rewind::default();
    rewind.disk.insert("a.rs", users_work.to_owned());
    let mut sequence = RevertSequence::new(units);

    assert!(sequence.start(|r| rewind.dispatch(r)).is_empty());
    rewind.complete(&mut sequence, "newer", &reverts);

    assert!(sequence.is_settled());
    assert_eq!(
        rewind.refused,
        ["newer"],
        "exactly one refusal, so one toast"
    );
    assert_eq!(rewind.abandoned, ["older"]);
    assert_eq!(
        rewind.dispatched,
        ["newer"],
        "the older revert was not attempted"
    );
    assert_eq!(rewind.disk["a.rs"], users_work);
    assert!(rewind.landed.is_empty());
}

/// Different files do not wait on each other: the newest revert of every file
/// is dispatched at once.
#[test]
fn different_files_revert_concurrently() {
    let units = vec![
        revert("b-newer", "b.rs", SECOND, FIRST),
        revert("a-only", "a.rs", FIRST, ORIGINAL),
        revert("b-older", "b.rs", FIRST, ORIGINAL),
        revert("c-only", "c.rs", FIRST, ORIGINAL),
    ];
    let reverts = catalogue(&units);
    let mut rewind = Rewind::default();
    rewind.disk.insert("a.rs", FIRST.to_owned());
    rewind.disk.insert("b.rs", SECOND.to_owned());
    rewind.disk.insert("c.rs", FIRST.to_owned());
    let mut sequence = RevertSequence::new(units);

    assert!(sequence.start(|r| rewind.dispatch(r)).is_empty());
    assert_eq!(rewind.in_flight, ["b-newer", "a-only", "c-only"]);

    // Outcomes may arrive in any order.
    rewind.complete(&mut sequence, "c-only", &reverts);
    rewind.complete(&mut sequence, "b-newer", &reverts);
    rewind.complete(&mut sequence, "a-only", &reverts);
    rewind.complete(&mut sequence, "b-older", &reverts);

    assert!(sequence.is_settled());
    assert!(rewind.refused.is_empty());
    for file in ["a.rs", "b.rs", "c.rs"] {
        assert_eq!(rewind.disk[file], ORIGINAL, "{file}");
    }
}

/// A refusal on one file stops only that file's older reverts.
#[test]
fn a_refusal_on_one_file_does_not_stop_another() {
    let units = vec![
        revert("a-newer", "a.rs", SECOND, FIRST),
        revert("b-newer", "b.rs", SECOND, FIRST),
        revert("a-older", "a.rs", FIRST, ORIGINAL),
        revert("b-older", "b.rs", FIRST, ORIGINAL),
    ];
    let reverts = catalogue(&units);
    let mut rewind = Rewind::default();
    rewind.disk.insert("a.rs", "edited by the user".to_owned());
    rewind.disk.insert("b.rs", SECOND.to_owned());
    let mut sequence = RevertSequence::new(units);

    assert!(sequence.start(|r| rewind.dispatch(r)).is_empty());
    rewind.complete(&mut sequence, "a-newer", &reverts);
    rewind.complete(&mut sequence, "b-newer", &reverts);
    rewind.complete(&mut sequence, "b-older", &reverts);

    assert!(sequence.is_settled());
    assert_eq!(rewind.refused, ["a-newer"]);
    assert_eq!(rewind.abandoned, ["a-older"]);
    assert_eq!(rewind.landed, ["b-newer", "b-older"]);
    assert_eq!(rewind.disk["b.rs"], ORIGINAL);
}

/// A newer revert that is refused before anything is written (no record of
/// the accept, no backing file) settles during dispatch; the older ones of
/// that file are abandoned right there, and other files proceed.
#[test]
fn a_revert_refused_at_dispatch_abandons_the_older_ones_of_its_file() {
    let mut sequence = RevertSequence::new([
        (Some("a.rs"), "a-newer"),
        (Some("a.rs"), "a-middle"),
        (Some("a.rs"), "a-older"),
        (Some("b.rs"), "b-only"),
    ]);
    let mut dispatched = Vec::new();
    let abandoned = sequence.start(|name| {
        dispatched.push(*name);
        if *name == "a-newer" {
            RevertStart::NotReverted
        } else {
            RevertStart::InFlight
        }
    });

    assert_eq!(abandoned, ["a-middle", "a-older"]);
    assert_eq!(dispatched, ["a-newer", "b-only"]);
    assert!(!sequence.is_settled(), "b-only is still in flight");
    let abandoned = sequence
        .settled(|name| *name == "b-only", true, |_| unreachable!())
        .expect("in flight");
    assert!(abandoned.is_empty());
    assert!(sequence.is_settled());
}

/// A later revert that fails at dispatch, reached because a newer one landed,
/// abandons the rest of the file the same way.
#[test]
fn a_revert_refused_at_dispatch_after_a_landed_one_abandons_the_rest() {
    let mut sequence = RevertSequence::new([
        (Some("a.rs"), "newest"),
        (Some("a.rs"), "middle"),
        (Some("a.rs"), "oldest"),
    ]);
    assert!(sequence.start(|_| RevertStart::InFlight).is_empty());
    let abandoned = sequence
        .settled(
            |name| *name == "newest",
            true,
            |name| {
                assert_eq!(*name, "middle");
                RevertStart::NotReverted
            },
        )
        .expect("in flight");
    assert_eq!(abandoned, ["oldest"]);
    assert!(sequence.is_settled());
}

/// Reverts with no file to write each run alone; they cannot collide.
#[test]
fn reverts_with_no_file_are_not_ordered_against_each_other() {
    let mut sequence = RevertSequence::new([(None::<&str>, "x"), (None, "y")]);
    let mut dispatched = Vec::new();
    assert!(
        sequence
            .start(|name| {
                dispatched.push(*name);
                RevertStart::NotReverted
            })
            .is_empty()
    );
    assert_eq!(dispatched, ["x", "y"]);
    assert!(sequence.is_settled());
}

/// An outcome the batch is not waiting on — a revert still queued behind a
/// newer one, or one already settled — changes nothing.
#[test]
fn an_outcome_not_in_flight_is_ignored() {
    let mut sequence = RevertSequence::new([(Some("a.rs"), "newer"), (Some("a.rs"), "older")]);
    assert!(sequence.start(|_| RevertStart::InFlight).is_empty());

    assert!(
        sequence
            .settled(|name| *name == "older", true, |_| unreachable!())
            .is_none()
    );
    assert!(
        sequence
            .settled(|name| *name == "newer", true, |_| RevertStart::InFlight)
            .is_some()
    );
    assert!(
        sequence
            .settled(|name| *name == "newer", true, |_| unreachable!())
            .is_none()
    );
    assert!(!sequence.is_settled(), "older is in flight now");
}

/// A batch with nothing to revert is settled from the start, so the rewind
/// finishes it (and records nothing) at once.
#[test]
fn an_empty_sequence_is_settled() {
    let mut sequence = RevertSequence::new(std::iter::empty::<(Option<&str>, ())>());
    assert!(sequence.start(|_| unreachable!()).is_empty());
    assert!(sequence.is_settled());
}

/// Defect 3 of #686, at the level this module controls: the rewind drops its
/// own references to the diff views (it removes their blocks) while their
/// writes are in flight. The sequence must keep every revert — and so, in the
/// app, the `ViewHandle<CodeDiffView>` inside it, whose subscriptions deliver
/// the refusal toast and the late mark — alive until it has settled, and let
/// go of each one once it has.
#[test]
fn the_sequence_keeps_every_revert_alive_until_it_settles() {
    let newer = Rc::new("newer");
    let older = Rc::new("older");
    let (weak_newer, weak_older): (Weak<&str>, Weak<&str>) =
        (Rc::downgrade(&newer), Rc::downgrade(&older));
    let mut sequence = RevertSequence::new([(Some("a.rs"), newer), (Some("a.rs"), older)]);
    assert!(sequence.start(|_| RevertStart::InFlight).is_empty());

    // Nothing outside the sequence holds them any more.
    assert!(
        weak_newer.upgrade().is_some(),
        "in flight: must be kept alive"
    );
    assert!(weak_older.upgrade().is_some(), "queued: must be kept alive");

    let abandoned = sequence
        .settled(|r| **r == "newer", false, |_| unreachable!())
        .expect("in flight");
    assert!(weak_newer.upgrade().is_none(), "settled: released");
    assert_eq!(abandoned.len(), 1, "handed back so the card can be told");
    drop(abandoned);
    assert!(weak_older.upgrade().is_none());
    assert!(sequence.is_settled());
}
