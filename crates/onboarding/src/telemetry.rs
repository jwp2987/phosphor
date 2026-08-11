// Port audit against the pinned oracle (02b53fcd8, ORACLE.md), #2 sweep,
// docs/sweep/warp-cli.md: the pin's `telemetry_tests.rs` has 6 tests with no
// fork equivalent (fork ships this source file but zero tests against it).
// All 6 assert on `OnboardingEvent::payload()` fields the pin adds for the
// "account-first onboarding" redesign -- `ACCOUNT_FIRST_FLOW_VERSION`,
// `account_class`, `OnboardingAuthCompleted`, `OnboardingUpgradeStarted`/
// `Completed`, `OnboardingAction` -- none of which this crate's
// `OnboardingEvent` has (it does not even implement the pin's
// `TelemetryEvent`/`payload()` trait). DECLINED per DECLINED.md's
// "Account-first onboarding, billing, paid tiers" row (#11); this is
// distinct from the separate "Telemetry channel physically removed" row,
// which is about whether events are ever sent, not this event shape.
use serde::{Deserialize, Serialize};

/// Telemetry events for the onboarding flow.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum OnboardingEvent {
    /// The onboarding flow was started.
    OnboardingStarted,
    /// A specific slide was viewed.
    SlideViewed { slide_name: String },
    /// A setting was changed during onboarding.
    SettingChanged { setting: String, value: String },
    /// The onboarding slides were completed.
    OnboardingSlidesCompleted {
        intention: String,
        model: Option<String>,
        autonomy: Option<String>,
        has_project_path: bool,
    },
    /// The user clicked the "Get Started" button.
    GetStartedClicked,
    /// The user started folder selection.
    FolderSelectionStarted,
    /// The user selected a folder.
    FolderSelected,
    /// A callout was displayed.
    CalloutDisplayed { callout: String },
    /// The user clicked next on a callout.
    CalloutNext,
    /// The user completed the callout flow.
    CalloutCompleted { completion_type: String },
    /// The user navigated to the next slide.
    SlideNavigatedNext,
    /// The user navigated to the previous slide.
    SlideNavigatedBack,
}
