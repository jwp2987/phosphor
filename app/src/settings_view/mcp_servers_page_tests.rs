use super::{InstallOrigin, MCPServersSettingsPageView, MCPServersSettingsWidget};
use crate::settings_view::settings_page::{search_terms_match, SettingsWidget};

// These tests cover the origin-first decision tree introduced in
// specs/GH686/product.md. The underlying `start_server_installation` flow
// exercises `MCPServersSettingsPageView::should_show_install_modal`, so
// asserting on the pure predicate captures the security-critical invariants
// without standing up a full view context or MCP manager.

#[test]
fn deeplink_origin_always_shows_modal() {
    // Invariant 2 (specs/GH686/product.md): every deeplink autoinstall shows
    // the installation modal, even when the template has no variables and no
    // markdown instructions.
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::Deeplink,
        /* has_variables */ false,
        /* has_instructions */ false,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::Deeplink,
        true,
        false,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::Deeplink,
        false,
        true,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::Deeplink,
        true,
        true,
    ));
}

#[test]
fn in_app_origin_shows_modal_only_for_variables_or_instructions() {
    // Invariant 9 (specs/GH686/product.md): in-app gallery clicks must keep
    // today's behavior: modal only when has_variables || has_instructions.
    assert!(!MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::InApp,
        false,
        false,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::InApp,
        true,
        false,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::InApp,
        false,
        true,
    ));
    assert!(MCPServersSettingsPageView::should_show_install_modal(
        InstallOrigin::InApp,
        true,
        true,
    ));
}

// This page is a monolithic `SettingsWidget` (`PageType::new_monolith`), so its own
// `search_terms()` is the only thing settings search indexes for anything it renders --
// including the "Auto-spawn servers from third-party agents" toggle
// (`settings-mcp-list-file-based-toggle-label`, `list_page.rs`), which upstream users refer
// to as "file-based" MCP servers (see the sibling `MCPServersWidget::search_terms()` on the
// AI settings page, which already includes the term).
#[test]
fn search_terms_include_file_based_mcp_toggle() {
    let widget = MCPServersSettingsWidget::default();
    assert!(
        search_terms_match(widget.search_terms(), "file-based"),
        "searching \"file-based\" should find the MCP Servers settings page, since it hosts \
         the file-based MCP auto-spawn toggle"
    );
}
