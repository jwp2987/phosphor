use super::{SurfaceDestination, surface_unavailable_reason};

/// Adapted from the pinned oracle's `agent_management_surface_reports_feature_flag_unavailable`
/// (02b53fcd8). The pin gates this surface on `FeatureFlag::AgentManagementView`; this fork has
/// no such flag because the Agent Management view was removed along with cloud-runner
/// orchestration (see DECLINED.md), so `SurfaceDestination::AgentManagement` is unconditionally
/// unavailable here instead of conditionally gated.
#[test]
fn agent_management_surface_is_unconditionally_unavailable() {
    warpui::App::test((), |mut app| async move {
        assert_eq!(
            app.update(|ctx| { surface_unavailable_reason(SurfaceDestination::AgentManagement, ctx) }),
            Some("agent management is not available in this build")
        );
    });
}
