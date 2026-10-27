use crate::editor::tests::sample_text;

use super::*;
use crate::editor::EditOrigin;
use anyhow::Error;
use warpui::App;

#[test]
fn test_chars_at() -> Result<()> {
    App::test((), |mut app| async move {
        let text = sample_text(6, 6);
        let buffer = app.add_model(|_| Buffer::new(text));
        let map = app.add_model(|ctx| DisplayMap::new(buffer.clone(), 4, ctx));
        buffer.update(&mut app, |buffer, ctx| {
            buffer.edit_for_test(
                vec![
                    Point::new(1, 0)..Point::new(1, 0),
                    Point::new(1, 1)..Point::new(1, 1),
                    Point::new(2, 1)..Point::new(2, 1),
                ],
                "\t",
                EditOrigin::UserInitiated,
                ctx,
            )
        })?;

        map.read(&app, |map, ctx| {
            assert_eq!(
                map.chars_at(DisplayPoint::new(1, 0), ctx)?
                    .take(10)
                    .collect::<String>(),
                "    b   bb"
            );
            assert_eq!(
                map.chars_at(DisplayPoint::new(1, 2), ctx)?
                    .take(10)
                    .collect::<String>(),
                "  b   bbbb"
            );
            assert_eq!(
                map.chars_at(DisplayPoint::new(1, 6), ctx)?
                    .take(13)
                    .collect::<String>(),
                "  bbbbb\nc   c"
            );

            Ok::<(), Error>(())
        })?;

        Ok(())
    })
}

#[test]
fn test_expand_tabs() {
    assert_eq!(expand_tabs("\t".chars(), 0, 4), 0);
    assert_eq!(expand_tabs("\t".chars(), 1, 4), 4);
    assert_eq!(expand_tabs("\ta".chars(), 2, 4), 5);
}

#[test]
fn test_collapse_tabs() {
    assert_eq!(collapse_tabs("\t".chars(), 0, Bias::Left, 4), (0, 0));
    assert_eq!(collapse_tabs("\t".chars(), 0, Bias::Right, 4), (0, 0));
    assert_eq!(collapse_tabs("\t".chars(), 1, Bias::Left, 4), (0, 3));
    assert_eq!(collapse_tabs("\t".chars(), 1, Bias::Right, 4), (1, 0));
    assert_eq!(collapse_tabs("\t".chars(), 2, Bias::Left, 4), (0, 2));
    assert_eq!(collapse_tabs("\t".chars(), 2, Bias::Right, 4), (1, 0));
    assert_eq!(collapse_tabs("\t".chars(), 3, Bias::Left, 4), (0, 1));
    assert_eq!(collapse_tabs("\t".chars(), 3, Bias::Right, 4), (1, 0));
    assert_eq!(collapse_tabs("\t".chars(), 4, Bias::Left, 4), (1, 0));
    assert_eq!(collapse_tabs("\t".chars(), 4, Bias::Right, 4), (1, 0));
    assert_eq!(collapse_tabs("\ta".chars(), 5, Bias::Left, 4), (2, 0));
    assert_eq!(collapse_tabs("\ta".chars(), 5, Bias::Right, 4), (2, 0));
}

#[test]
fn test_max_point() -> Result<()> {
    App::test((), |mut app| async move {
        let buffer = app.add_model(|_| Buffer::new("aaa\n\t\tbbb"));
        let map = app.add_model(|ctx| DisplayMap::new(buffer.clone(), 4, ctx));
        map.read(&app, |map, app| {
            assert_eq!(map.max_point(app), DisplayPoint::new(1, 11))
        });
        Ok(())
    })
}

/// #955: tab expansion must not walk a multi-megabyte row char by char. The fast path answers
/// from fragment slices, and must agree with the exact scan wherever it does answer.
#[test]
fn test_tab_free_prefix_fast_path_on_huge_row() {
    App::test((), |mut app| async move {
        let huge = 3 * 1024 * 1024;
        let buffer = app.add_model(|_| Buffer::new("x".repeat(huge)));
        let map = app.add_model(|ctx| DisplayMap::new(buffer.clone(), 4, ctx));

        map.read(&app, |map, ctx| {
            // Provably tab-free through the whole 3 MB row: answered without a char walk.
            assert!(map.fold_map.is_tab_free_prefix(0, huge, ctx));
            // More chars than the row has: not proven, the exact path decides.
            assert!(!map.fold_map.is_tab_free_prefix(0, huge + 1, ctx));
            // Short prefixes keep the exact path.
            assert!(!map.fold_map.is_tab_free_prefix(0, 10, ctx));

            let end = Point::new(0, huge as u32);
            let display = end.to_display_point(map, ctx).unwrap();
            assert_eq!(display, DisplayPoint::new(0, huge as u32));
            assert_eq!(display.to_buffer_point(map, Bias::Left, ctx).unwrap(), end);
            assert_eq!(map.line_len(0, ctx).unwrap(), huge as u32);
        });
    })
}

#[test]
fn test_tab_free_prefix_fast_path_still_expands_tabs() {
    App::test((), |mut app| async move {
        let mut text = "x".repeat(5000);
        text.insert(3000, '\t');
        let buffer = app.add_model(|_| Buffer::new(text));
        let map = app.add_model(|ctx| DisplayMap::new(buffer.clone(), 4, ctx));

        map.read(&app, |map, ctx| {
            assert!(map.fold_map.is_tab_free_prefix(0, 2000, ctx));
            assert!(!map.fold_map.is_tab_free_prefix(0, 3500, ctx));

            // Before the tab: identity. After it: the tab at column 3000 (a multiple of 4)
            // occupies 4 display columns, i.e. 3 more than one char.
            let before = Point::new(0, 2000).to_display_point(map, ctx).unwrap();
            assert_eq!(before, DisplayPoint::new(0, 2000));
            let after = Point::new(0, 3500).to_display_point(map, ctx).unwrap();
            assert_eq!(after, DisplayPoint::new(0, 3503));
            assert_eq!(
                after.to_buffer_point(map, Bias::Left, ctx).unwrap(),
                Point::new(0, 3500)
            );
            assert_eq!(map.line_len(0, ctx).unwrap(), 5003);
        });
    })
}
