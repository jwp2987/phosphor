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
//! the rects the tab bar actually painted, via
//! [`rendered_tab_bar_faults`]. A header-count check derived only from the
//! model would pass even if the tab bar drew nothing, which is precisely the
//! coverage the duplicate-header bug needed and did not have.

mod assertion;
mod step;

pub use assertion::*;
pub use step::*;
