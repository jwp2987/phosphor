use settings_page::{FilteredPageType, MatchData, PageType, SettingsWidget};
use warpui::elements::Empty;
use warpui::{App, AppContext, Element, Entity, View};

use super::*;
use crate::appearance::Appearance;

// ── SettingsSection classification ──────────────────────────────────────────
//
// NOTE: Warp's `is_code_subpage()` / `is_cloud_platform_subpage()` and the
// `CodeIndexing` / `CloudEnvironments` / `OzCloudAPIKeys` / `Account` /
// `Teams` / `BillingAndUsage` `SettingsSection` variants do not
// exist in this fork -- the decentralized branch collapsed the sidebar to a
// single "Agents" umbrella (see `SettingsView::new`) and dropped the
// cloud-only Account/Billing/Teams/Cloud-platform pages entirely. Tests that
// depended on those are adapted below to use sections that do exist in this
// fork (`About`, `AI`, `Code`), or dropped where the underlying feature is
// gone (cloud subpages, multi-umbrella nav).

#[test]
fn ai_subpages_are_identified() {
    assert!(SettingsSection::WarpAgent.is_ai_subpage());
    assert!(SettingsSection::AgentProfiles.is_ai_subpage());
    assert!(SettingsSection::AgentMCPServers.is_ai_subpage());
    assert!(SettingsSection::AgentProviders.is_ai_subpage());
    assert!(SettingsSection::Knowledge.is_ai_subpage());
    assert!(SettingsSection::ThirdPartyCLIAgents.is_ai_subpage());

    assert!(!SettingsSection::AI.is_ai_subpage());
    assert!(!SettingsSection::About.is_ai_subpage());
    assert!(!SettingsSection::Code.is_ai_subpage());
}

#[test]
fn is_subpage_covers_all_umbrella_types() {
    // All subpages under any umbrella should return true.
    for section in SettingsSection::ai_subpages() {
        assert!(section.is_subpage(), "{section:?} should be a subpage");
    }

    // Top-level pages should not be subpages.
    assert!(!SettingsSection::About.is_subpage());
    assert!(!SettingsSection::AI.is_subpage());
    assert!(!SettingsSection::Code.is_subpage());
    assert!(!SettingsSection::Appearance.is_subpage());
    assert!(!SettingsSection::Privacy.is_subpage());
}

// ── parent_page_section mapping ─────────────────────────────────────────────

#[test]
fn ai_subpages_map_to_ai_backing_page() {
    assert_eq!(
        SettingsSection::WarpAgent.parent_page_section(),
        SettingsSection::AI
    );
    assert_eq!(
        SettingsSection::AgentProfiles.parent_page_section(),
        SettingsSection::AI
    );
    assert_eq!(
        SettingsSection::AgentProviders.parent_page_section(),
        SettingsSection::AI
    );
    assert_eq!(
        SettingsSection::Knowledge.parent_page_section(),
        SettingsSection::AI
    );
    assert_eq!(
        SettingsSection::ThirdPartyCLIAgents.parent_page_section(),
        SettingsSection::AI
    );
}

#[test]
fn agent_mcp_servers_maps_to_mcp_servers_page() {
    // AgentMCPServers renders the standalone MCPServers page, not the AI page.
    assert_eq!(
        SettingsSection::AgentMCPServers.parent_page_section(),
        SettingsSection::MCPServers
    );
}

#[test]
fn editor_and_code_review_maps_to_code_backing_page() {
    assert_eq!(
        SettingsSection::EditorAndCodeReview.parent_page_section(),
        SettingsSection::Code
    );
}

#[test]
fn non_subpage_sections_map_to_themselves() {
    assert_eq!(
        SettingsSection::About.parent_page_section(),
        SettingsSection::About
    );
    assert_eq!(
        SettingsSection::AI.parent_page_section(),
        SettingsSection::AI
    );
    assert_eq!(
        SettingsSection::Appearance.parent_page_section(),
        SettingsSection::Appearance
    );
    assert_eq!(
        SettingsSection::Privacy.parent_page_section(),
        SettingsSection::Privacy
    );
}

// ── ai_subpages list ────────────────────────────────────────────────────────

#[test]
fn ai_subpages_list_contains_all_ai_subpage_variants() {
    let subpages = SettingsSection::ai_subpages();
    assert!(subpages.contains(&SettingsSection::WarpAgent));
    assert!(subpages.contains(&SettingsSection::AgentProfiles));
    assert!(subpages.contains(&SettingsSection::AgentMCPServers));
    assert!(subpages.contains(&SettingsSection::AgentProviders));
    assert!(subpages.contains(&SettingsSection::Knowledge));
    assert!(subpages.contains(&SettingsSection::ThirdPartyCLIAgents));
}

#[test]
fn ai_subpages_list_does_not_contain_non_subpages() {
    let subpages = SettingsSection::ai_subpages();
    assert!(!subpages.contains(&SettingsSection::AI));
    assert!(!subpages.contains(&SettingsSection::About));
    assert!(!subpages.contains(&SettingsSection::Code));
}

// ── MatchData behavior ──────────────────────────────────────────────────────

#[test]
fn match_data_uncounted_true_is_truthy() {
    assert!(MatchData::Uncounted(true).is_truthy());
}

#[test]
fn match_data_uncounted_false_is_not_truthy() {
    assert!(!MatchData::Uncounted(false).is_truthy());
}

#[test]
fn match_data_countable_nonzero_is_truthy() {
    assert!(MatchData::Countable(3).is_truthy());
    assert!(MatchData::Countable(1).is_truthy());
}

#[test]
fn match_data_countable_zero_is_not_truthy() {
    assert!(!MatchData::Countable(0).is_truthy());
}

// ── Display / FromStr round-trip ────────────────────────────────────────────

#[test]
fn subpage_display_names_are_correct() {
    crate::i18n::init(Some("en"));

    assert_eq!(SettingsSection::WarpAgent.to_string(), "Phosphor Agent");
    assert_eq!(SettingsSection::AgentProfiles.to_string(), "Profiles");
    assert_eq!(SettingsSection::AgentMCPServers.to_string(), "MCP servers");
    assert_eq!(SettingsSection::AgentProviders.to_string(), "Providers");
    assert_eq!(SettingsSection::Knowledge.to_string(), "Knowledge");
    assert_eq!(
        SettingsSection::ThirdPartyCLIAgents.to_string(),
        "Third party CLI agents"
    );
    assert_eq!(
        SettingsSection::EditorAndCodeReview.to_string(),
        "Editor and Code Review"
    );
}

#[test]
fn subpage_from_str_parses_display_names() {
    // Both the legacy "Oz" name and the "Zap Agent" name must resolve to
    // SettingsSection::WarpAgent so existing deep links, persisted telemetry
    // strings, and external callers continue to work after renames.
    assert_eq!(
        SettingsSection::from_str("Oz"),
        Ok(SettingsSection::WarpAgent)
    );
    assert_eq!(
        SettingsSection::from_str("Zap Agent"),
        Ok(SettingsSection::WarpAgent)
    );
    assert_eq!(
        SettingsSection::from_str("Profiles"),
        Ok(SettingsSection::AgentProfiles)
    );
    assert_eq!(
        SettingsSection::from_str("Knowledge"),
        Ok(SettingsSection::Knowledge)
    );
    assert_eq!(
        SettingsSection::from_str("Editor and Code Review"),
        Ok(SettingsSection::EditorAndCodeReview)
    );
}

// ── Stable persistence keys ─────────────────────────────────────────────────
//
// Regression guard for issue #578. `Display` is localized in this fork, so the
// settings pane used to persist a translated string that `FromStr` (English
// literals) could not read back: the user silently landed on the default
// section instead of the pane they left open. Persistence now stores
// `persistence_key()` and reads it with `from_persistence_key()`, which also
// upgrades every legacy value.

#[test]
fn all_lists_every_settings_section() {
    // Exhaustive match, no wildcard arm: adding a `SettingsSection` variant
    // stops this test compiling until the variant is also added to `all()`,
    // so a new section can never quietly miss persistence coverage. (The
    // matching guard for the key itself is `persistence_key`, which is an
    // exhaustive match too.)
    fn is_a_known_section(section: SettingsSection) {
        match section {
            SettingsSection::About
            | SettingsSection::MCPServers
            | SettingsSection::Appearance
            | SettingsSection::Features
            | SettingsSection::Keybindings
            | SettingsSection::ZapDrive
            | SettingsSection::Warpify
            | SettingsSection::AI
            | SettingsSection::WarpAgent
            | SettingsSection::AgentProfiles
            | SettingsSection::AgentMCPServers
            | SettingsSection::AgentProviders
            | SettingsSection::Knowledge
            | SettingsSection::ThirdPartyCLIAgents
            | SettingsSection::Network
            | SettingsSection::Privacy
            | SettingsSection::Code
            | SettingsSection::EditorAndCodeReview
            | SettingsSection::Scripting => {}
        }
    }

    for section in SettingsSection::all() {
        is_a_known_section(*section);
    }

    // The default section must be persistable like any other.
    assert!(SettingsSection::all().contains(&SettingsSection::default()));
}

#[test]
fn persistence_keys_are_unique_and_not_localized() {
    let mut seen: Vec<&'static str> = Vec::new();
    for section in SettingsSection::all() {
        let key = section.persistence_key();
        assert!(
            key.is_ascii() && !key.is_empty(),
            "{section:?} has a non-ASCII or empty persistence key {key:?}; \
             stored identifiers must never be translated"
        );
        assert!(
            !seen.contains(&key),
            "persistence key {key:?} is used by more than one section, so one \
             of them would restore as the other"
        );
        seen.push(key);
    }
}

#[test]
fn persistence_key_round_trips_for_every_section() {
    for section in SettingsSection::all() {
        assert_eq!(
            SettingsSection::from_persistence_key(section.persistence_key()),
            Some(*section),
            "{section:?} does not survive persist -> read"
        );
        // `FromStr` is the locale-independent parser behind
        // `surface.settings.open`, so it must accept the stable key too.
        assert_eq!(
            SettingsSection::from_str(section.persistence_key()),
            Ok(*section),
            "surface.settings.open cannot resolve the stable key for {section:?}"
        );
    }
}

#[test]
fn from_persistence_key_upgrades_legacy_english_labels() {
    // Every row written before stable keys existed holds the section's English
    // label, because that is what `Display` produced on an English UI. The
    // table is spelled out rather than read from `Display` so this test does
    // not depend on the process-wide locale; the exhaustive match means a new
    // variant has to declare its legacy label here.
    fn legacy_english_label(section: SettingsSection) -> &'static str {
        match section {
            SettingsSection::Scripting => "Scripting",
            SettingsSection::About => "About",
            SettingsSection::MCPServers => "MCP Servers",
            SettingsSection::Appearance => "Appearance",
            SettingsSection::Features => "Features",
            SettingsSection::Keybindings => "Keyboard shortcuts",
            SettingsSection::ZapDrive => "Phosphor Drive",
            SettingsSection::Warpify => "Warpify",
            SettingsSection::AI => "AI",
            SettingsSection::WarpAgent => "Phosphor Agent",
            SettingsSection::AgentProfiles => "Profiles",
            SettingsSection::AgentMCPServers => "MCP servers",
            SettingsSection::AgentProviders => "Providers",
            SettingsSection::Knowledge => "Knowledge",
            SettingsSection::ThirdPartyCLIAgents => "Third party CLI agents",
            SettingsSection::Network => "Network",
            SettingsSection::Privacy => "Privacy",
            SettingsSection::Code => "Code",
            SettingsSection::EditorAndCodeReview => "Editor and Code Review",
        }
    }

    for section in SettingsSection::all() {
        let legacy_value = legacy_english_label(*section);
        assert_eq!(
            SettingsSection::from_persistence_key(legacy_value),
            Some(*section),
            "a settings pane persisted as {legacy_value:?} would be lost"
        );
    }

    // Renames the pages went through before the Phosphor rebrand, which may
    // still be sitting in a long-lived database.
    assert_eq!(
        SettingsSection::from_persistence_key("Oz"),
        Some(SettingsSection::WarpAgent)
    );
    assert_eq!(
        SettingsSection::from_persistence_key("Zap Agent"),
        Some(SettingsSection::WarpAgent)
    );
    assert_eq!(
        SettingsSection::from_persistence_key("Zap Drive"),
        Some(SettingsSection::ZapDrive)
    );
}

#[test]
fn every_display_label_is_readable_back() {
    // Guards the other half of the same bug: the English label persisted by an
    // older build has to remain parseable as the label is reworded. This is
    // how `settings-section-warp-agent = Phosphor Agent` came to be
    // unparseable -- `FromStr` still only knew the older "Oz" / "Zap Agent".
    crate::i18n::init(Some("en"));

    for section in SettingsSection::all() {
        let label = section.to_string();
        // Compared by label rather than by identity: in some locales two
        // sections share one label (zh-CN renders both MCP server pages the
        // same), and no parser can tell those apart. Under English -- where the
        // labels are distinct -- this is exactly an identity check.
        assert_eq!(
            SettingsSection::from_persistence_key(&label).map(|parsed| parsed.to_string()),
            Some(label.clone()),
            "the displayed label {label:?} cannot be read back as {section:?}"
        );
    }
}

#[test]
fn from_persistence_key_upgrades_the_legacy_network_labels() {
    // Both spellings the Network page was ever stored as: the English label,
    // and the zh-CN one that an earlier point patch had to teach `FromStr`
    // because a real user already had it in their database.
    assert_eq!(
        SettingsSection::from_persistence_key("Network"),
        Some(SettingsSection::Network)
    );
    assert_eq!(
        SettingsSection::from_persistence_key("网络"),
        Some(SettingsSection::Network)
    );
}

#[test]
fn from_str_does_not_accept_localized_labels() {
    // `surface.settings.open --page <name>` must resolve the same page
    // whatever language the UI is in, so the localized alternatives live in
    // the persistence-only upgrade path, not in `FromStr`.
    assert_eq!(SettingsSection::from_str("网络"), Err(()));
}

#[test]
fn from_persistence_key_rejects_unknown_values() {
    assert_eq!(SettingsSection::from_persistence_key("NotASection"), None);
    assert_eq!(SettingsSection::from_persistence_key(""), None);
}

// ── Privacy page registration ───────────────────────────────────────────────
// Regression guard for issue #2: this fork defined and read `SafeModeEnabled`,
// `SecretDisplayModeSetting`, `IsCrashReportingEnabled` and `IsTelemetryEnabled`
// at runtime but had never ported Warp's privacy page, so none of them had a
// reachable GUI control. The section must exist, must be a top-level page, and
// must be the section `PrivacyPageView` registers itself under -- otherwise the
// sidebar entry and the page look-up disagree and the nav item renders nothing.

#[test]
fn privacy_section_display_name_is_correct() {
    // `App::test` never initializes i18n globally, so do it explicitly here
    // rather than relying on another test having run first.
    crate::i18n::init(Some("en"));

    assert_eq!(SettingsSection::Privacy.to_string(), "Privacy");
}

#[test]
fn privacy_section_from_str_parses_display_name() {
    assert_eq!(
        SettingsSection::from_str("Privacy"),
        Ok(SettingsSection::Privacy)
    );
}

#[test]
fn privacy_page_view_is_registered_under_the_privacy_section() {
    // `SettingsPage::new` keys the page by `V::section()` and `settings_page()`
    // looks it up by the section the sidebar nav item carries, so these must match.
    assert_eq!(PrivacyPageView::section(), SettingsSection::Privacy);
}

// ── Subpage search filter simulation ────────────────────────────────────────
// These tests simulate the per-subpage search filtering logic used in
// handle_search_editor_event: each subpage should only be visible if its
// own widgets' search terms match, not if a sibling subpage's terms match.

/// Helper: given a map of subpage→MatchData, returns which subpages are visible.
fn visible_subpages(
    subpage_filter: &HashMap<SettingsSection, MatchData>,
    subpages: &[SettingsSection],
) -> Vec<SettingsSection> {
    subpages
        .iter()
        .filter(|s| {
            subpage_filter
                .get(s)
                .map(|md| md.is_truthy())
                .unwrap_or(false)
        })
        .copied()
        .collect()
}

#[test]
fn search_knowledge_shows_only_knowledge_subpage() {
    // Simulate: searching "knowledge" matched the Knowledge subpage but not others.
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(1));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    let visible = visible_subpages(&filter, SettingsSection::ai_subpages());

    assert_eq!(visible, vec![SettingsSection::Knowledge]);
}

#[test]
fn search_agent_shows_profiles_and_cli_agents() {
    // "agent" appears in both AgentProfiles and ThirdPartyCLIAgents search terms.
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(2));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(0));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(1),
    );

    let visible = visible_subpages(&filter, SettingsSection::ai_subpages());

    assert!(visible.contains(&SettingsSection::AgentProfiles));
    assert!(visible.contains(&SettingsSection::ThirdPartyCLIAgents));
    assert!(!visible.contains(&SettingsSection::WarpAgent));
    assert!(!visible.contains(&SettingsSection::Knowledge));
}

#[test]
fn empty_search_shows_no_subpages_in_filter() {
    // When search is cleared, subpage_filter is empty — all subpages fall back
    // to their backing page visibility (Uncounted(true) by default).
    let filter: HashMap<SettingsSection, MatchData> = HashMap::new();

    let visible = visible_subpages(&filter, SettingsSection::ai_subpages());

    // No entries in filter means no subpage-specific filtering; all return false
    // from the filter map. The actual rendering code falls back to the backing
    // page's pages_filter which defaults to Uncounted(true).
    assert!(visible.is_empty());
}

#[test]
fn search_with_no_matches_hides_all_subpages() {
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(0));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    let visible = visible_subpages(&filter, SettingsSection::ai_subpages());

    assert!(visible.is_empty());
}

/// Helper: check if an umbrella should be visible given a subpage filter.
fn umbrella_visible(
    subpage_filter: &HashMap<SettingsSection, MatchData>,
    umbrella_subpages: &[SettingsSection],
) -> bool {
    umbrella_subpages.iter().any(|s| {
        subpage_filter
            .get(s)
            .map(|md| md.is_truthy())
            .unwrap_or(false)
    })
}

#[test]
fn umbrella_hidden_when_no_subpages_match() {
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(0));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    assert!(!umbrella_visible(&filter, SettingsSection::ai_subpages()));
}

// ── cycle_pages search filter ────────────────────────────────────────────────
// These tests validate the logic added to cycle_pages() to ensure arrow key
// navigation respects the active search filter.

/// Mirrors the filter predicate used in cycle_pages() when search is active.
fn section_passes_nav_filter(
    section: SettingsSection,
    subpage_filter: &HashMap<SettingsSection, MatchData>,
    pages_filter: &[(SettingsSection, MatchData)],
) -> bool {
    if let Some(md) = subpage_filter.get(&section) {
        md.is_truthy()
    } else {
        let backing = section.parent_page_section();
        pages_filter
            .iter()
            .any(|(s, md)| *s == backing && md.is_truthy())
    }
}

#[test]
fn nav_filter_includes_matching_subpage_and_excludes_others() {
    let mut subpage_filter = HashMap::new();
    subpage_filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    subpage_filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    subpage_filter.insert(SettingsSection::Knowledge, MatchData::Countable(1));
    subpage_filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    // No page-level filter entries needed since all AI subpages have subpage_filter entries.
    let pages_filter: Vec<(SettingsSection, MatchData)> = vec![];

    assert!(!section_passes_nav_filter(
        SettingsSection::WarpAgent,
        &subpage_filter,
        &pages_filter
    ));
    assert!(!section_passes_nav_filter(
        SettingsSection::AgentProfiles,
        &subpage_filter,
        &pages_filter
    ));
    assert!(section_passes_nav_filter(
        SettingsSection::Knowledge,
        &subpage_filter,
        &pages_filter
    ));
    assert!(!section_passes_nav_filter(
        SettingsSection::ThirdPartyCLIAgents,
        &subpage_filter,
        &pages_filter
    ));
}

#[test]
fn nav_filter_falls_back_to_pages_filter_for_top_level_pages() {
    // Top-level pages (About, Appearance, etc.) have no subpage_filter entry.
    // They fall back to pages_filter using parent_page_section() == themselves.
    let subpage_filter: HashMap<SettingsSection, MatchData> = HashMap::new();
    let pages_filter = vec![
        (SettingsSection::About, MatchData::Uncounted(true)),
        (SettingsSection::Appearance, MatchData::Countable(0)),
        (SettingsSection::Features, MatchData::Uncounted(true)),
    ];

    assert!(section_passes_nav_filter(
        SettingsSection::About,
        &subpage_filter,
        &pages_filter
    ));
    assert!(!section_passes_nav_filter(
        SettingsSection::Appearance,
        &subpage_filter,
        &pages_filter
    ));
    assert!(section_passes_nav_filter(
        SettingsSection::Features,
        &subpage_filter,
        &pages_filter
    ));
}

#[test]
fn umbrella_visible_when_any_subpage_matches() {
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(1));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    assert!(umbrella_visible(&filter, SettingsSection::ai_subpages()));
}

// ── Search auto-select simulation ───────────────────────────────────────────
// These tests simulate the auto-select logic in handle_search_editor_event:
// when the current subpage is filtered out by search, the view should jump
// to the first visible subpage or page.

/// Simulates the "is current still visible" check from the search handler.
/// Returns true if `current` is still visible given the subpage_filter and
/// a list of (backing_section, is_truthy) pairs for pages_filter.
fn is_current_visible(
    current: SettingsSection,
    subpage_filter: &HashMap<SettingsSection, MatchData>,
    pages_visible: &[(SettingsSection, bool)],
) -> bool {
    if let Some(md) = subpage_filter.get(&current) {
        return md.is_truthy();
    }
    let backing = current.parent_page_section();
    pages_visible
        .iter()
        .any(|(section, visible)| *section == backing && *visible)
}

/// Simulates finding the first visible section from the nav_items order.
fn first_visible_section(
    nav_order: &[SettingsSection],
    subpage_filter: &HashMap<SettingsSection, MatchData>,
    pages_visible: &[(SettingsSection, bool)],
) -> Option<SettingsSection> {
    nav_order.iter().copied().find(|section| {
        if let Some(md) = subpage_filter.get(section) {
            md.is_truthy()
        } else {
            let backing = section.parent_page_section();
            pages_visible
                .iter()
                .any(|(s, visible)| *s == backing && *visible)
        }
    })
}

#[test]
fn auto_select_jumps_away_from_filtered_out_subpage() {
    // User is on Knowledge, searches "agent" which matches Profiles but not Knowledge.
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(2));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(0));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(1),
    );

    let current = SettingsSection::Knowledge;
    assert!(
        !is_current_visible(current, &filter, &[]),
        "Knowledge should not be visible when it has 0 matches"
    );

    // The nav order: WarpAgent, Profiles, ..., Knowledge, ThirdPartyCLI
    let nav_order = SettingsSection::ai_subpages();
    let first = first_visible_section(nav_order, &filter, &[]);
    assert_eq!(
        first,
        Some(SettingsSection::AgentProfiles),
        "Should auto-select Profiles as the first visible subpage"
    );
}

#[test]
fn auto_select_stays_on_current_when_it_matches() {
    // User is on Knowledge, searches "knowledge" which matches Knowledge.
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(1));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    let current = SettingsSection::Knowledge;
    assert!(
        is_current_visible(current, &filter, &[]),
        "Knowledge should remain visible when it has matches"
    );
}

#[test]
fn auto_select_falls_back_to_top_level_page_when_no_subpages_match() {
    // All AI subpages filtered out, but About (top-level) is still visible.
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));
    filter.insert(SettingsSection::Knowledge, MatchData::Countable(0));
    filter.insert(
        SettingsSection::ThirdPartyCLIAgents,
        MatchData::Countable(0),
    );

    let pages_visible = vec![
        (SettingsSection::About, true),
        (SettingsSection::AI, false),
    ];

    // Nav order includes top-level About before the AI subpages.
    let nav_order = vec![
        SettingsSection::About,
        SettingsSection::WarpAgent,
        SettingsSection::AgentProfiles,
        SettingsSection::Knowledge,
        SettingsSection::ThirdPartyCLIAgents,
    ];

    let first = first_visible_section(&nav_order, &filter, &pages_visible);
    assert_eq!(
        first,
        Some(SettingsSection::About),
        "Should fall back to About when no subpages match"
    );
}

#[test]
fn auto_select_handles_standalone_subpage_via_backing_page() {
    // AgentMCPServers has its own backing page (MCPServers), not in subpage_filter.
    // It should be visible if its backing page is visible.
    let filter = HashMap::new(); // no per-subpage entries for AgentMCPServers

    let pages_visible = vec![
        (SettingsSection::MCPServers, true),
        (SettingsSection::AI, false),
    ];

    let current = SettingsSection::AgentMCPServers;
    assert!(
        is_current_visible(current, &filter, &pages_visible),
        "AgentMCPServers should be visible via its MCPServers backing page"
    );
}

#[test]
fn auto_select_with_no_matches_anywhere() {
    let mut filter = HashMap::new();
    filter.insert(SettingsSection::WarpAgent, MatchData::Countable(0));
    filter.insert(SettingsSection::AgentProfiles, MatchData::Countable(0));

    let pages_visible = vec![
        (SettingsSection::About, false),
        (SettingsSection::AI, false),
    ];

    let nav_order = vec![
        SettingsSection::About,
        SettingsSection::WarpAgent,
        SettingsSection::AgentProfiles,
    ];

    let first = first_visible_section(&nav_order, &filter, &pages_visible);
    assert_eq!(
        first, None,
        "No section should be selected when nothing matches"
    );
}

// ── Backward compatibility ──────────────────────────────────────────────────

#[test]
fn legacy_ai_section_maps_to_oz_default() {
    // SettingsSection::AI should be treated as backward-compat and map to
    // WarpAgent via the code in set_and_refresh_current_page_internal.
    // Here we just verify the parent_page_section is still AI (for page lookup).
    assert_eq!(
        SettingsSection::AI.parent_page_section(),
        SettingsSection::AI
    );
    // And that AI is NOT itself a subpage.
    assert!(!SettingsSection::AI.is_subpage());
}

// ── Collapsed umbrella nav-stop behavior ────────────────────────────────────
// Verify that arrow-key navigation lands on a collapsed umbrella as a single
// stop (and activates it by jumping to the first subpage, which auto-expands
// the umbrella) instead of silently skipping over it.
//
// NOTE: unlike Warp, this fork's `SettingsView::new` builds only a single
// "Agents" umbrella -- the Code / Cloud-platform umbrellas Warp's original
// `realistic_nav_items()` modeled don't exist here (the decentralized branch
// dropped the cloud/billing/teams pages that lived under them, and `Code` is
// a plain top-level page with no subpages of its own). `realistic_nav_items`
// below mirrors this fork's actual sidebar layout from `SettingsView::new`.

use nav::{SettingsNavItem, SettingsUmbrella};

/// Builds the nav-items layout used by `SettingsView::new`, matching the real
/// sidebar ordering (minus the feature-flagged `Network` page) so tests
/// exercise a realistic nav order.
fn realistic_nav_items() -> Vec<SettingsNavItem> {
    vec![
        SettingsNavItem::Umbrella(SettingsUmbrella::new(
            "Agents",
            SettingsSection::ai_subpages().to_vec(),
        )),
        SettingsNavItem::Page(SettingsSection::Code),
        SettingsNavItem::Page(SettingsSection::Appearance),
        SettingsNavItem::Page(SettingsSection::Features),
        SettingsNavItem::Page(SettingsSection::Keybindings),
        SettingsNavItem::Page(SettingsSection::Warpify),
        SettingsNavItem::Page(SettingsSection::About),
    ]
}

/// Mutably flips an umbrella's `expanded` flag at `nav_index`.
fn set_expanded(nav_items: &mut [SettingsNavItem], nav_index: usize, expanded: bool) {
    if let Some(SettingsNavItem::Umbrella(u)) = nav_items.get_mut(nav_index) {
        u.expanded = expanded;
    } else {
        panic!("nav_items[{nav_index}] is not an Umbrella");
    }
}

#[test]
fn collapsed_umbrella_is_a_single_nav_stop() {
    let nav_items = realistic_nav_items();
    // The umbrella defaults to collapsed.
    let stops = build_nav_stops(&nav_items, |_| true);

    // Expect: <Agents umbrella>, Code, Appearance, Features, Keybindings,
    // Warpify, About.
    assert_eq!(stops.len(), 7);
    assert!(matches!(
        stops[0],
        NavStop::CollapsedUmbrella {
            nav_index: 0,
            first_subpage: SettingsSection::WarpAgent,
            last_subpage: SettingsSection::ThirdPartyCLIAgents,
        }
    ));
    assert!(matches!(stops[1], NavStop::Section(SettingsSection::Code)));
    assert!(matches!(
        stops[2],
        NavStop::Section(SettingsSection::Appearance)
    ));
    assert!(matches!(
        stops[6],
        NavStop::Section(SettingsSection::About)
    ));
}

#[test]
fn expanded_umbrella_produces_section_stop_per_subpage() {
    let mut nav_items = realistic_nav_items();
    // Expand the Agents umbrella so each of its subpages becomes a nav stop.
    set_expanded(&mut nav_items, 0, true);

    let stops = build_nav_stops(&nav_items, |_| true);

    // Expect: WarpAgent, AgentProfiles, AgentProviders, AgentMCPServers,
    // Knowledge, ThirdPartyCLIAgents, Code, Appearance, Features,
    // Keybindings, Warpify, About.
    let sections: Vec<_> = stops
        .iter()
        .map(|s| match s {
            NavStop::Section(section) => format!("{section:?}"),
            NavStop::CollapsedUmbrella { nav_index, .. } => format!("Umbrella@{nav_index}"),
        })
        .collect();
    assert_eq!(
        sections,
        vec![
            "WarpAgent",
            "AgentProfiles",
            "AgentProviders",
            "AgentMCPServers",
            "Knowledge",
            "ThirdPartyCLIAgents",
            "Code",
            "Appearance",
            "Features",
            "Keybindings",
            "Warpify",
            "About",
        ]
    );
}

#[test]
fn collapsed_umbrella_with_filtered_subpages_uses_first_visible_subpage() {
    // When a search filter hides the first subpage, activating the collapsed
    // umbrella should land on the *next* visible subpage (still auto-expanding).
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| {
        // Hide WarpAgent (first AI subpage); keep the rest.
        section != SettingsSection::WarpAgent
    });

    let agents_stop = stops
        .iter()
        .find(|s| matches!(s, NavStop::CollapsedUmbrella { nav_index: 0, .. }))
        .expect("Agents umbrella should still be a collapsed stop");

    match agents_stop {
        NavStop::CollapsedUmbrella {
            first_subpage,
            last_subpage,
            ..
        } => {
            assert_eq!(
                *first_subpage,
                SettingsSection::AgentProfiles,
                "WarpAgent is hidden by the filter, so the first visible subpage is AgentProfiles"
            );
            assert_eq!(
                *last_subpage,
                SettingsSection::ThirdPartyCLIAgents,
                "last_subpage is unaffected by hiding WarpAgent and should remain the last visible subpage"
            );
        }
        _ => unreachable!(),
    }
}

#[test]
fn umbrella_with_no_visible_subpages_is_skipped_entirely() {
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| !section.is_ai_subpage());

    // The Agents umbrella's subpages are all AI subpages, so the entire
    // umbrella should be absent from the nav order.
    assert!(
        stops
            .iter()
            .all(|s| !matches!(s, NavStop::CollapsedUmbrella { .. })),
        "Agents umbrella should not appear when none of its subpages are visible"
    );
    // The still-visible top-level pages remain as stops.
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::Code)))
    );
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::About)))
    );
}

#[test]
fn filtered_out_top_level_page_is_skipped() {
    let nav_items = realistic_nav_items();

    let stops = build_nav_stops(&nav_items, |section| section != SettingsSection::Warpify);

    assert!(
        !stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::Warpify))),
        "Warpify should be filtered out entirely"
    );
    // But other pages remain.
    assert!(
        stops
            .iter()
            .any(|s| matches!(s, NavStop::Section(SettingsSection::About)))
    );
}

// ── current_stop_index ──────────────────────────────────────────────────────

#[test]
fn current_stop_index_matches_section_stop() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    let idx = current_stop_index(&stops, &nav_items, SettingsSection::Appearance);
    assert_eq!(idx, Some(2));
}

#[test]
fn current_stop_index_maps_subpage_to_collapsed_umbrella() {
    // Edge case: the user manually collapsed the Agents umbrella while still
    // on one of its subpages. The collapsed umbrella should match as the
    // current stop so arrow-key cycling continues from the umbrella's position.
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    let idx = current_stop_index(&stops, &nav_items, SettingsSection::Knowledge);
    assert_eq!(
        idx,
        Some(0),
        "Knowledge is under the collapsed Agents umbrella at nav_index 0"
    );
}

#[test]
fn current_stop_index_returns_none_when_section_is_not_present() {
    let nav_items = realistic_nav_items();
    // Filter out all AI subpages (and therefore the Agents umbrella) entirely.
    let stops = build_nav_stops(&nav_items, |section| !section.is_ai_subpage());

    // Knowledge isn't directly in stops, and no remaining collapsed umbrella
    // contains it, so current_stop_index should return None.
    assert_eq!(
        current_stop_index(&stops, &nav_items, SettingsSection::Knowledge),
        None
    );
}

// ── next_stop_index wrapping ────────────────────────────────────────────────

#[test]
fn next_stop_index_wraps_at_ends() {
    assert_eq!(next_stop_index(0, 3, CycleDirection::Up), 2);
    assert_eq!(next_stop_index(2, 3, CycleDirection::Down), 0);
    assert_eq!(next_stop_index(1, 3, CycleDirection::Up), 0);
    assert_eq!(next_stop_index(1, 3, CycleDirection::Down), 2);
}

#[test]
fn next_stop_index_handles_single_stop() {
    assert_eq!(next_stop_index(0, 1, CycleDirection::Up), 0);
    assert_eq!(next_stop_index(0, 1, CycleDirection::Down), 0);
}

// ── End-to-end cycling (no search) ──────────────────────────────────────────
// These tests simulate the sequence of nav-stop activations that would result
// from repeatedly pressing Down/Up, ensuring a collapsed umbrella is never
// skipped over.

/// Computes the section that would become active after applying the direction
/// once, starting from `current`. Mirrors the final target-resolution step in
/// `cycle_pages`.
fn simulate_cycle(
    nav_items: &[SettingsNavItem],
    stops: &[NavStop],
    current: SettingsSection,
    direction: CycleDirection,
) -> SettingsSection {
    let active = current_stop_index(stops, nav_items, current)
        .expect("current should exist in stops in these tests");
    let next = next_stop_index(active, stops.len(), direction);
    match stops[next] {
        NavStop::Section(section) => section,
        NavStop::CollapsedUmbrella {
            first_subpage,
            last_subpage,
            ..
        } => match direction {
            CycleDirection::Up => last_subpage,
            CycleDirection::Down => first_subpage,
        },
    }
}

#[test]
fn arrow_down_wrapping_from_last_page_lands_on_first_agents_subpage() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    // About is the last stop; pressing Down from there wraps around to the
    // collapsed Agents umbrella and should auto-expand it, selecting
    // WarpAgent (the first subpage), not skip over it.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::About,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::WarpAgent);
}

#[test]
fn arrow_up_from_code_with_collapsed_agents_lands_on_last_subpage() {
    let nav_items = realistic_nav_items();
    let stops = build_nav_stops(&nav_items, |_| true);

    // Pressing Up from Code (the stop right after the collapsed Agents
    // umbrella) should land on the collapsed Agents umbrella, which resolves
    // to ThirdPartyCLIAgents (last visible subpage) so the user continues
    // moving in natural reading order rather than being jumped back to the
    // top of the umbrella.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::Code,
        CycleDirection::Up,
    );
    assert_eq!(next, SettingsSection::ThirdPartyCLIAgents);
}

#[test]
fn arrow_up_into_collapsed_umbrella_respects_search_filter_for_last_subpage() {
    let nav_items = realistic_nav_items();
    // Hide the last two AI subpages; the last *visible* subpage of the
    // still-collapsed Agents umbrella should be AgentMCPServers.
    let is_visible = |section: SettingsSection| {
        !matches!(
            section,
            SettingsSection::Knowledge | SettingsSection::ThirdPartyCLIAgents
        )
    };
    let stops = build_nav_stops(&nav_items, is_visible);

    // From Code, Up should land on the last *visible* AI subpage
    // (AgentMCPServers), not on the filtered-out Knowledge/ThirdPartyCLIAgents
    // or on the first subpage WarpAgent.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::Code,
        CycleDirection::Up,
    );
    assert_eq!(next, SettingsSection::AgentMCPServers);
}

#[test]
fn arrow_down_from_expanded_last_subpage_leaves_umbrella() {
    let mut nav_items = realistic_nav_items();
    set_expanded(&mut nav_items, 0, true); // expand Agents
    let stops = build_nav_stops(&nav_items, |_| true);

    // ThirdPartyCLIAgents is the last Agents subpage; Down should move to
    // Code (the next top-level page in the nav order).
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::ThirdPartyCLIAgents,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::Code);
}

#[test]
fn arrow_down_wrapping_into_collapsed_umbrella_respects_search_filter() {
    let nav_items = realistic_nav_items();
    // Search filter hides WarpAgent and AgentProfiles so the first visible AI
    // subpage is AgentProviders.
    let is_visible = |section: SettingsSection| {
        !matches!(
            section,
            SettingsSection::WarpAgent | SettingsSection::AgentProfiles
        )
    };
    let stops = build_nav_stops(&nav_items, is_visible);

    // From About (last stop), Down wraps around to the collapsed Agents
    // umbrella and should land on AgentProviders (first visible subpage),
    // not on WarpAgent / AgentProfiles.
    let next = simulate_cycle(
        &nav_items,
        &stops,
        SettingsSection::About,
        CycleDirection::Down,
    );
    assert_eq!(next, SettingsSection::AgentProviders);
}

// ── Active subpage filter reapply after rebuild (APP-4922) ───────────────────
// Searching on an AI/Code subpage rebuilds the subpage's PageType (via
// set_active_subpage), which resets its widget filter to every widget; the
// active query must be reapplied so only matching widgets render. These tests
// exercise the real PageType::Uncategorized filter lifecycle via the real
// `update_filter` method. The production reapply call sites in mod.rs
// (handle_search_editor_event/cycle_pages/SelectAndRefresh) need a full
// ViewContext<SettingsView>, so they are verified via computer-use screenshots.
//
// NOTE: Warp additionally has a `search_terms_match_direct_unit_checks` test
// exercising a module-level `pub(super) fn search_terms_match` in
// settings_page.rs. In this fork that helper is a private fn nested inside
// `PageType::update_filter` (not exposed at module scope), so it can't be
// called directly from here -- see final report (NEEDS-ADAPTATION). Its
// behavior is still covered indirectly by the `update_filter`-driven tests
// below.

/// Minimal View so PageType<V> can be instantiated in a unit test without the
/// full SettingsView/ViewContext the production reapply call sites require.
struct TestSettingsView;

impl Entity for TestSettingsView {
    type Event = ();
}

impl View for TestSettingsView {
    fn ui_name() -> &'static str {
        "TestSettingsView"
    }

    fn render(&self, _: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

/// A SettingsWidget whose only test-relevant state is its search terms; render
/// is never invoked by the filter lifecycle under test.
struct StubWidget {
    terms: &'static str,
}

impl SettingsWidget for StubWidget {
    type View = TestSettingsView;

    fn search_terms(&self) -> &str {
        self.terms
    }

    fn render(&self, _: &Self::View, _: &Appearance, _: &AppContext) -> Box<dyn Element> {
        Empty::new().finish()
    }
}

/// A fresh Uncategorized page mirroring set_active_subpage -> build_page ->
/// new_uncategorized: every widget index visible by default.
fn stub_widgets_page() -> PageType<TestSettingsView> {
    let widgets: Vec<Box<dyn SettingsWidget<View = TestSettingsView>>> = vec![
        Box::new(StubWidget {
            terms: "warp agent global ai toggle",
        }),
        Box::new(StubWidget {
            terms: "active ai autosuggestions prompt",
        }),
        Box::new(StubWidget {
            terms: "ai input model api key",
        }),
        Box::new(StubWidget {
            terms: "file search fuzzy opener",
        }),
        Box::new(StubWidget {
            terms: "voice input",
        }),
    ];
    PageType::new_uncategorized(widgets, None)
}

/// Number of widgets the page would render under its current filter.
fn visible_widget_count<V: View>(page: &PageType<V>) -> usize {
    let FilteredPageType::Uncategorized { widgets, .. } = page.get_filtered() else {
        panic!("expected Uncategorized page");
    };
    widgets.len()
}

#[test]
fn rebuild_resets_filter_to_all_widgets() {
    // Searching "file search" matches exactly one widget. A freshly built page
    // (mirroring set_active_subpage -> build_page -> new_uncategorized) resets
    // the filter to every widget, so without reapplying update_filter the
    // subpage would show all widgets.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            let md = page.update_filter("file search", ctx);
            assert!(md.is_truthy());
            assert_eq!(visible_widget_count(&page), 1);

            let rebuilt = stub_widgets_page();
            assert_eq!(
                visible_widget_count(&rebuilt),
                5,
                "rebuild resets the filter to all widgets when update_filter isn't reapplied"
            );
        });
    });
}

#[test]
fn rebuild_with_reapply_keeps_only_matching_widgets() {
    // The fix: after a rebuild, reapply update_filter with the active query so
    // only matching widgets render on the restored subpage.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("file search", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("file search", ctx);
            assert_eq!(
                visible_widget_count(&rebuilt),
                1,
                "reapplying the filter after a rebuild keeps only matching widgets visible"
            );
        });
    });
}

#[test]
fn reapply_handles_multi_word_and_case() {
    // A multi-word, case-insensitive query survives the rebuild + reapply cycle.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("AI INPUT", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("AI INPUT", ctx);
            assert_eq!(visible_widget_count(&rebuilt), 1);
        });
    });
}

#[test]
fn empty_query_after_reapply_shows_all_widgets() {
    // When the search is cleared, the subpage shows all widgets again.
    App::test((), |mut app| async move {
        app.update(|ctx| {
            let mut page = stub_widgets_page();
            page.update_filter("agent", ctx);
            assert_eq!(visible_widget_count(&page), 1);

            let mut rebuilt = stub_widgets_page();
            rebuilt.update_filter("", ctx);
            assert_eq!(
                visible_widget_count(&rebuilt),
                5,
                "an empty query restores every widget on the subpage"
            );
        });
    });
}
