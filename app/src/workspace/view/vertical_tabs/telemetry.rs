use crate::workspace::tab_settings::{
    VerticalTabsCompactSubtitle, VerticalTabsDisplayGranularity, VerticalTabsPrimaryInfo,
    VerticalTabsTabItemMode, VerticalTabsViewMode,
};

/// Which display option on the vertical tabs settings popup the user changed,
/// along with the new value they picked.
#[derive(Clone, Copy, Debug)]
pub enum VerticalTabsDisplayOption {
    #[allow(dead_code)] // field: set at construction or persisted
    DisplayGranularity(VerticalTabsDisplayGranularity),
    #[allow(dead_code)] // field: set at construction or persisted
    TabItemMode(VerticalTabsTabItemMode),
    #[allow(dead_code)] // field: set at construction or persisted
    ViewMode(VerticalTabsViewMode),
    #[allow(dead_code)] // field: set at construction or persisted
    PrimaryInfo(VerticalTabsPrimaryInfo),
    #[allow(dead_code)] // field: set at construction or persisted
    CompactSubtitle(VerticalTabsCompactSubtitle),
    #[allow(dead_code)] // field: set at construction or persisted
    ShowPrLink(bool),
    #[allow(dead_code)] // field: set at construction or persisted
    ShowDiffStats(bool),
    #[allow(dead_code)] // field: set at construction or persisted
    ShowDetailsOnHover(bool),
}

impl VerticalTabsDisplayOption {
    #[allow(dead_code)] // used by warp_tui (tui feature)
    fn option_name(&self) -> &'static str {
        match self {
            Self::DisplayGranularity(_) => "display_granularity",
            Self::TabItemMode(_) => "tab_item_mode",
            Self::ViewMode(_) => "view_mode",
            Self::PrimaryInfo(_) => "primary_info",
            Self::CompactSubtitle(_) => "compact_subtitle",
            Self::ShowPrLink(_) => "show_pr_link",
            Self::ShowDiffStats(_) => "show_diff_stats",
            Self::ShowDetailsOnHover(_) => "show_details_on_hover",
        }
    }
}

/// Where in the vertical tabs UI a clickable diff-stats or GitHub PR chip
/// was rendered when the user clicked it.
#[derive(Clone, Copy, Debug)]
pub enum VerticalTabsChipEntrypoint {
    /// The chip was rendered on a row representing a single pane
    /// (display granularity: Panes).
    Pane,
    /// The chip was rendered on a row representing a tab group
    /// (display granularity: Tabs).
    Tab,
    /// The chip was rendered inside the detail sidecar that appears on row hover.
    DetailsSidecar,
}

impl VerticalTabsChipEntrypoint {
    #[allow(dead_code)] // used by sibling unused code or shared name
    fn serialized(&self) -> &'static str {
        match self {
            Self::Pane => "pane",
            Self::Tab => "tab",
            Self::DetailsSidecar => "details_sidecar",
        }
    }
}

#[derive(Debug)]
pub enum VerticalTabsTelemetryEvent {
    /// The user updated a display option in the vertical tabs settings popup.
    #[allow(dead_code)] // field: set at construction or persisted
    DisplayOptionChanged(VerticalTabsDisplayOption),
    /// The user clicked the diff stats chip on a vertical tabs row or the detail sidecar.
    DiffStatsChipClicked {
        #[allow(dead_code)] // field: set at construction or persisted
        entrypoint: VerticalTabsChipEntrypoint,
    },
    /// The user clicked the GitHub PR chip on a vertical tabs row or the detail sidecar.
    PrChipClicked {
        #[allow(dead_code)] // field: set at construction or persisted
        entrypoint: VerticalTabsChipEntrypoint,
    },
}
