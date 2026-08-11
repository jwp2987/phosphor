// Port audit against the pinned oracle (02b53fcd8, ORACLE.md), #2 sweep,
// docs/sweep/warp-cli.md: the pin also has `offer_slide.rs` (module
// `OfferSlide`/`OfferVariant`, 3 tests), the "You've got a head start" /
// "Choose how to start" post-signup upsell slide -- its copy advertises
// "expanded cloud agent access", "premium models" and an `account_class`
// telemetry field, and it renders `upgrade_auth_prompt::render_upgrade_auth_
// prompt_bar` (also absent). DECLINED per DECLINED.md's "Account-first
// onboarding, billing, paid tiers" row (#11); not ported here.
mod agent_slide;
mod bottom_nav;
mod customize_slide;
mod intention_slide;
mod intro_slide;
pub mod layout;
mod onboarding_slide;
mod progress_dots;
mod project_slide;
pub mod slide_content;
mod theme_picker_slide;
mod third_party_slide;
mod toggle_card;
mod two_line_button;

pub use agent_slide::{AgentAutonomy, AgentDevelopmentSettings, AgentSlide, OnboardingModelInfo};
pub use bottom_nav::onboarding_bottom_nav;
pub use customize_slide::CustomizeUISlide;
pub use intention_slide::IntentionSlide;
pub use intro_slide::IntroSlide;
pub use onboarding_slide::OnboardingSlide;
pub use project_slide::{ProjectOnboardingSettings, ProjectSlide};
pub use theme_picker_slide::{ThemePickerSlide, ThemePickerSlideEvent};
pub use third_party_slide::ThirdPartySlide;
