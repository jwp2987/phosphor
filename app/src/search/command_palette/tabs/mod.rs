//! Ctrl+Tab tab switcher data source.
//!
//! Ported from Warp's `app/src/search/command_palette/tabs/` at the pinned
//! oracle (`02b53fcd8`, Warp `2026.07.29.09.05` stable — see `ORACLE.md`).
//!
//! Backs `CtrlTabBehavior::CycleMostRecentTab`: it serves the *tabs* of the
//! current window ordered most-recently-used first, where the sessions source
//! (`super::navigation`) serves sessions across windows.

pub mod data_source;
pub mod search_item;

pub use data_source::DataSource;
pub use search_item::SearchItem;
