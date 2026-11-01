//! Runtime kill switches for the input queue migration (design section 4.0).
//!
//! `PHOSPHOR_INPUT_QUEUE_<AREA>=0` turns one area back to its legacy path; the legacy code stays
//! in the tree until the last phase. `PHOSPHOR_INPUT_QUEUE=legacy` (or `0`/`off`) turns every area
//! off, and a comma list (`PHOSPHOR_INPUT_QUEUE=paste,ssh`) turns on only the listed areas. The
//! per-area variable wins over the global one. The default is ON: an area ships on the new path
//! once its tests pass. `Area::Paste` (phase 1), `Area::Subshell` (phase 2) and `Area::Enter` (phase 3) are read so far.

use std::sync::OnceLock;

/// One migrated area; the phase that moves it is in the design's table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Area {
    /// Phase 1: paste (Q4, Q4b).
    Paste,
    /// Phase 2: subshell setup and bootstrap (Q6a, Q6b).
    Subshell,
    /// Phase 3: Enter verdict, Tab, after-enter replay (Q1, Q2).
    Enter,
    /// Phase 4: ssh (Q7, Q5).
    Ssh,
    /// Phase 5: hand-off holds (Q3a/b/c).
    Handoff,
}

impl Area {
    pub const ALL: [Area; 5] = [
        Area::Paste,
        Area::Subshell,
        Area::Enter,
        Area::Ssh,
        Area::Handoff,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Area::Paste => "paste",
            Area::Subshell => "subshell",
            Area::Enter => "enter",
            Area::Ssh => "ssh",
            Area::Handoff => "handoff",
        }
    }

    pub fn env_var(self) -> &'static str {
        match self {
            Area::Paste => "PHOSPHOR_INPUT_QUEUE_PASTE",
            Area::Subshell => "PHOSPHOR_INPUT_QUEUE_SUBSHELL",
            Area::Enter => "PHOSPHOR_INPUT_QUEUE_ENTER",
            Area::Ssh => "PHOSPHOR_INPUT_QUEUE_SSH",
            Area::Handoff => "PHOSPHOR_INPUT_QUEUE_HANDOFF",
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

fn is_falsy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "0" | "false" | "off" | "no" | "legacy"
    )
}

fn is_truthy(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "on" | "yes" | "all"
    )
}

/// Pure decision from the two variables' values; `is_enabled` caches it per area.
pub fn parse_enabled(global: Option<&str>, per_area: Option<&str>, area: Area) -> bool {
    if let Some(value) = per_area {
        if is_falsy(value) {
            return false;
        }
        if is_truthy(value) {
            return true;
        }
    }
    match global {
        None => true,
        Some(value) if value.trim().is_empty() || is_truthy(value) => true,
        Some(value) if is_falsy(value) => false,
        Some(list) => list
            .split(',')
            .any(|item| item.trim().eq_ignore_ascii_case(area.name())),
    }
}

/// Whether `area` runs on the new queue. Read once per process.
pub fn is_enabled(area: Area) -> bool {
    static CACHE: [OnceLock<bool>; 5] = [const { OnceLock::new() }; 5];
    *CACHE[area.index()].get_or_init(|| {
        parse_enabled(
            std::env::var("PHOSPHOR_INPUT_QUEUE").ok().as_deref(),
            std::env::var(area.env_var()).ok().as_deref(),
            area,
        )
    })
}
