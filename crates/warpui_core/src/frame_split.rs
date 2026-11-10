//! Where the time of one frame build goes: measurement only, nothing here changes what a frame
//! shows or when it is drawn (docs/design/key-scene-build.md).
//!
//! The first frame built after a key press is logged as
//! `input_order hop=key_frame_build passes=.. layout_passes=.. views=.. mounted=.. render_ms=..
//! layout_ms=.. after_layout_ms=.. paint_ms=.. hover_ms=.. total_ms=.. layout_top=.. paint_top=..`:
//! the stages of `AppContext::build_scene` and the three view types with the largest self time in
//! layout and in paint. View type names, counts and times only, never text.

use std::time::Duration;

use instant::Instant;

use crate::EntityId;

/// How many view types `layout_top=` / `paint_top=` name.
const TOP_VIEWS: usize = 3;

/// Self time per view of one walk of the view tree (a layout pass or a paint pass): the time
/// spent in a view's own elements, without the child views they contain.
#[derive(Default)]
pub(crate) struct ViewClock {
    /// One entry per view being walked, outermost first: when it was entered and the time its
    /// child views have taken so far.
    open: Vec<(Instant, Duration)>,
    /// `(view, self time)`, in the order the views were left.
    done: Vec<(EntityId, Duration)>,
}

impl ViewClock {
    pub(crate) fn enter(&mut self, now: Instant) {
        self.open.push((now, Duration::ZERO));
    }

    /// Leaves the view entered last. A call without a matching [`Self::enter`] records nothing.
    pub(crate) fn exit(&mut self, view_id: EntityId, now: Instant) {
        let Some((entered, in_children)) = self.open.pop() else {
            return;
        };
        let inclusive = now.duration_since(entered);
        if let Some((_, parent_children)) = self.open.last_mut() {
            *parent_children += inclusive;
        }
        self.done
            .push((view_id, inclusive.saturating_sub(in_children)));
    }

    pub(crate) fn finish(self) -> Vec<(EntityId, Duration)> {
        self.done
    }
}

/// The stages of one `Presenter` scene build. The per-view lists are empty unless view timing
/// was asked for (`Presenter::set_view_timing`).
#[derive(Default, Clone)]
pub(crate) struct BuildSplit {
    pub(crate) layout: Duration,
    pub(crate) after_layout: Duration,
    pub(crate) paint: Duration,
    /// Views the layout pass visited (0 for a frame that skipped layout).
    pub(crate) mounted: usize,
    pub(crate) layout_views: Vec<(EntityId, Duration)>,
    pub(crate) paint_views: Vec<(EntityId, Duration)>,
}

/// One whole `AppContext::build_scene` for the frame after a key press: every pass of its loop
/// added up.
#[derive(Default)]
pub(crate) struct KeyFrameBuild {
    /// Passes of the build loop that built a scene (2 or 3 when the hover pass invalidated).
    pub(crate) passes: u32,
    /// Of those, the ones that ran layout.
    pub(crate) layout_passes: u32,
    /// Views re-rendered (`invalidation.updated`).
    pub(crate) updated: usize,
    /// Views in the element tree as of the last layout pass.
    pub(crate) mounted: usize,
    /// `View::render` of the updated views.
    pub(crate) render: Duration,
    pub(crate) layout: Duration,
    pub(crate) after_layout: Duration,
    pub(crate) paint: Duration,
    /// The synthetic mouse-move dispatched after each pass (hover state for the new layout).
    pub(crate) hover: Duration,
    /// The whole build, including what the stages above do not cover.
    pub(crate) total: Duration,
    /// `(view type, self time)` per view visited; any order, repeated types are added up.
    pub(crate) layout_views: Vec<(String, Duration)>,
    pub(crate) paint_views: Vec<(String, Duration)>,
}

impl KeyFrameBuild {
    /// Adds one pass's presenter stages.
    pub(crate) fn add_pass(&mut self, render: Duration, laid_out: bool, split: &BuildSplit) {
        self.passes += 1;
        self.render += render;
        self.paint += split.paint;
        if laid_out {
            self.layout_passes += 1;
            self.layout += split.layout;
            self.after_layout += split.after_layout;
            self.mounted = split.mounted;
        }
    }

    /// The detail of `input_order hop=key_frame_build`. script/realapp/appci/render_hops.py
    /// (`_KEY_FRAME_BUILD`) parses it.
    pub(crate) fn detail(&self) -> String {
        format!(
            "passes={} layout_passes={} views={} mounted={} render_ms={:.2} layout_ms={:.2} \
             after_layout_ms={:.2} paint_ms={:.2} hover_ms={:.2} total_ms={:.2} layout_top={} \
             paint_top={}",
            self.passes,
            self.layout_passes,
            self.updated,
            self.mounted,
            ms(self.render),
            ms(self.layout),
            ms(self.after_layout),
            ms(self.paint),
            ms(self.hover),
            ms(self.total),
            top_views(&self.layout_views),
            top_views(&self.paint_views),
        )
    }
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// `Terminal:0.41,Workspace:0.12,EditorView:0.08`: the view types with the largest summed self
/// time, largest first (ties by name); `-` when nothing was timed. A name is reduced to
/// `[A-Za-z0-9_]` so the field stays one token.
fn top_views(views: &[(String, Duration)]) -> String {
    let mut by_type: Vec<(String, Duration)> = Vec::new();
    for (name, time) in views {
        let name: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        match by_type.iter_mut().find(|(known, _)| *known == name) {
            Some((_, total)) => *total += *time,
            None => by_type.push((name, *time)),
        }
    }
    if by_type.is_empty() {
        return "-".to_string();
    }
    by_type.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    by_type
        .iter()
        .take(TOP_VIEWS)
        .map(|(name, time)| format!("{name}:{:.2}", ms(*time)))
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
#[path = "frame_split_tests.rs"]
mod tests;
