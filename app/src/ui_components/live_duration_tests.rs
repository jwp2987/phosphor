use chrono::TimeZone;

use super::*;

fn at(secs: i64, millis: i64) -> (DateTime<Local>, DateTime<Local>) {
    let start = Local.timestamp_opt(1_700_000_000, 0).unwrap();
    (
        start,
        start + chrono::Duration::seconds(secs) + chrono::Duration::milliseconds(millis),
    )
}

#[test]
fn text_matches_the_block_label_format() {
    let (start, now) = at(0, 900);
    assert_eq!(live_duration_text(start, now), None, "nothing before 1 s");
    let (start, now) = at(5, 400);
    assert_eq!(
        live_duration_text(start, now).as_deref(),
        Some(Block::format_duration(chrono::Duration::seconds(5)).as_str())
    );
    let (start, now) = at(65, 0);
    assert_eq!(
        live_duration_text(start, now).as_deref(),
        Some(Block::format_duration(chrono::Duration::seconds(65)).as_str())
    );
    let (start, now) = at(3 * 3600 + 4 * 60 + 12, 0);
    assert_eq!(
        live_duration_text(start, now).as_deref(),
        Some(Block::format_duration(chrono::Duration::seconds(3 * 3600 + 4 * 60 + 12)).as_str())
    );
}

#[test]
fn every_counter_character_has_a_glyph() {
    for secs in [1, 9, 10, 59, 60, 61, 599, 3599, 3600, 3661, 36_000, 360_000] {
        let (start, now) = at(secs, 0);
        let text = live_duration_text(start, now).unwrap();
        for c in text.chars() {
            assert!(GLYPHS.contains(c), "{c:?} in {text:?} has no glyph");
        }
    }
}

#[test]
fn the_next_tick_lands_just_after_the_next_whole_second() {
    let (start, now) = at(3, 0);
    assert_eq!(until_next_tick(start, now), Duration::from_millis(1005));
    let (start, now) = at(3, 999);
    assert_eq!(until_next_tick(start, now), Duration::from_millis(6));
    let (start, now) = at(3, 250);
    assert_eq!(until_next_tick(start, now), Duration::from_millis(755));
}
