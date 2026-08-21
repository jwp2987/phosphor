//! Integration-test helpers for tab groups.
//!
//! Tab groups had 151 unit tests and zero integration tests while three
//! user-visible bugs lived in `Workspace::on_tab_drag`: a tab could not be
//! dragged out of a group, dragging one *through* a group split it, and the
//! split rendered as two headers sharing one id (only one of which would
//! close). None of that is reachable from a unit harness, because
//! `on_tab_drag` resolves the hovered group from laid-out element rects via
//! `element_position_by_id`. These helpers exist so the GUI-driving
//! integration suite can assert the model state those real drags produce.
//!
//! The two assertions that make a claim about *rendering* —
//! [`assert_group_header_count`] and [`assert_groups_contiguous`] — also read
//! the rects the tab bar actually painted, via [`rendered_tab_bar_faults`]. A
//! header-count check derived only from the model would pass even if the tab
//! bar drew the group in two pieces, and the geometric checks there are what
//! catch that.
//!
//! **The rendered half is a keyed probe, not a survey of the screen.**
//! `PositionCache` offers `get_position(id)` and no enumeration, so every rect
//! those assertions read is fetched under an id the *model* predicts, and the
//! cache never evicts. So: chrome painted for a group the model no longer
//! holds is invisible (nothing asks for its key), and chrome that stopped being
//! painted after one good frame is invisible (its last rect is still cached).
//! The second half of the original report — a duplicate header that outlived
//! its group and would not close — sits in exactly that blind spot. Closing it
//! needs enumeration *and* eviction in `warpui_core`; until then these
//! assertions catch a group drawn in the wrong *place*, not one drawn out of
//! nothing. See [`rendered_tab_bar_faults`] for the itemised list.

mod assertion;
mod step;

pub use assertion::*;
pub use step::*;
