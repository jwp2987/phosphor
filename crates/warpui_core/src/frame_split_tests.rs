use std::time::Duration;

use instant::Instant;

use super::{BuildSplit, KeyFrameBuild, ViewClock, top_views};
use crate::EntityId;

fn at(start: Instant, micros: u64) -> Instant {
    start + Duration::from_micros(micros)
}

fn us(micros: u64) -> Duration {
    Duration::from_micros(micros)
}

/// A view's self time is its own elements only: the child views inside it are taken out, at
/// every depth, so the self times of a walk add up to the walk.
#[test]
fn self_time_leaves_out_the_child_views() {
    let (root, pane, editor, footer) = (
        EntityId::from_usize(1),
        EntityId::from_usize(2),
        EntityId::from_usize(3),
        EntityId::from_usize(4),
    );
    let start = Instant::now();
    let mut clock = ViewClock::default();
    clock.enter(at(start, 0)); // root
    clock.enter(at(start, 100)); // pane
    clock.enter(at(start, 150)); // editor
    clock.exit(editor, at(start, 450));
    clock.exit(pane, at(start, 500));
    clock.enter(at(start, 600)); // footer, a sibling of the pane
    clock.exit(footer, at(start, 650));
    clock.exit(root, at(start, 1000));

    let times = clock.finish();
    assert_eq!(
        times,
        vec![
            (editor, us(300)),
            (pane, us(100)),
            (footer, us(50)),
            (root, us(550)),
        ]
    );
    let sum: Duration = times.iter().map(|(_, time)| *time).sum();
    assert_eq!(sum, us(1000), "self times add up to the whole walk");
}

/// A view laid out twice in one walk (a parent that measures a child, then lays it out) is two
/// entries; an unmatched exit records nothing instead of panicking inside a frame.
#[test]
fn a_view_visited_twice_is_two_entries_and_an_unmatched_exit_is_ignored() {
    let view = EntityId::from_usize(7);
    let start = Instant::now();
    let mut clock = ViewClock::default();
    clock.exit(view, at(start, 5));
    clock.enter(at(start, 10));
    clock.exit(view, at(start, 30));
    clock.enter(at(start, 40));
    clock.exit(view, at(start, 100));
    assert_eq!(clock.finish(), vec![(view, us(20)), (view, us(60))]);
}

#[test]
fn top_views_adds_up_a_type_and_names_the_three_largest() {
    let views = vec![
        ("Terminal".to_string(), us(300)),
        ("ActionButton".to_string(), us(40)),
        ("ActionButton".to_string(), us(70)),
        ("Workspace".to_string(), us(100)),
        ("EditorView".to_string(), us(90)),
    ];
    assert_eq!(
        top_views(&views),
        "Terminal:0.30,ActionButton:0.11,Workspace:0.10"
    );
    assert_eq!(top_views(&[]), "-");
    // equal times: by name, so the line is stable
    let tie = vec![("B".to_string(), us(10)), ("A".to_string(), us(10))];
    assert_eq!(top_views(&tie), "A:0.01,B:0.01");
    // a name stays one token of the log line
    let odd = vec![("Odd Name,x:y".to_string(), us(1000))];
    assert_eq!(top_views(&odd), "Odd_Name_x_y:1.00");
}

/// Passes add up; a pass that skipped layout adds paint only and keeps the mounted count of the
/// last layout.
#[test]
fn passes_add_up_and_a_paint_only_pass_adds_no_layout() {
    let laid_out = BuildSplit {
        layout: us(600),
        after_layout: us(20),
        paint: us(400),
        mounted: 57,
        ..Default::default()
    };
    let paint_only = BuildSplit {
        layout: us(999),
        after_layout: us(999),
        paint: us(300),
        mounted: 0,
        ..Default::default()
    };
    let mut build = KeyFrameBuild::default();
    build.add_pass(us(200), true, &laid_out);
    build.add_pass(us(0), false, &paint_only);
    assert_eq!((build.passes, build.layout_passes), (2, 1));
    assert_eq!(build.render, us(200));
    assert_eq!(build.layout, us(600));
    assert_eq!(build.after_layout, us(20));
    assert_eq!(build.paint, us(700));
    assert_eq!(build.mounted, 57);
}

/// The line script/realapp/appci/render_hops.py parses (`_KEY_FRAME_BUILD`, and the self-test
/// `key_frame_build_hop_parses` carries the same text).
#[test]
fn the_detail_is_the_line_the_harness_parses() {
    let build = KeyFrameBuild {
        passes: 1,
        layout_passes: 1,
        updated: 4,
        mounted: 57,
        render: us(210),
        layout: us(630),
        after_layout: us(20),
        paint: us(480),
        hover: us(310),
        total: us(1800),
        layout_views: vec![
            ("Terminal".to_string(), us(310)),
            ("Workspace".to_string(), us(100)),
            ("EditorView".to_string(), us(80)),
            ("Input".to_string(), us(30)),
        ],
        paint_views: vec![("Terminal".to_string(), us(220))],
    };
    assert_eq!(
        build.detail(),
        "passes=1 layout_passes=1 views=4 mounted=57 render_ms=0.21 layout_ms=0.63 \
         after_layout_ms=0.02 paint_ms=0.48 hover_ms=0.31 total_ms=1.80 \
         layout_top=Terminal:0.31,Workspace:0.10,EditorView:0.08 paint_top=Terminal:0.22"
    );
    // nothing timed per view (view timing was off): the fields are still there
    assert!(
        KeyFrameBuild::default()
            .detail()
            .ends_with("total_ms=0.00 layout_top=- paint_top=-")
    );
}
