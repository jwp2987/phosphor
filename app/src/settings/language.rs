//! User interface language setting (persisted via settings.toml, applied to the i18n loader
//! on startup).
//!
//! Currently supports English, Simplified Chinese, and Japanese. Adding a new language only
//! requires:
//!   1. Adding a variant to `Language`
//!   2. Creating a new translation file at `app/i18n/<locale>/warp.ftl`
//!   3. Adding a case to `Display` + `to_locale_str`
//!
//! The switch takes full effect only after a restart (already-rendered UI text does not
//! automatically re-lay-out; the view needs to be rebuilt).
//! The settings-page dropdown should include a "Takes full effect after restarting Zap" hint.

use enum_iterator::Sequence;
use serde::{Deserialize, Serialize};
use warp_core::settings::{macros::define_settings_group, SupportedPlatforms, SyncToCloud};

#[derive(
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Sequence,
    schemars::JsonSchema,
    settings_value::SettingsValue,
)]
#[schemars(
    description = "The language used in Zap's user interface.",
    rename_all = "snake_case"
)]
pub enum Language {
    /// Follows the system language; falls back to English if the system locale isn't a supported language.
    #[default]
    #[schemars(description = "System default")]
    System,
    #[schemars(description = "English")]
    English,
    #[schemars(description = "Simplified Chinese")]
    SimplifiedChinese,
    #[schemars(description = "Japanese")]
    Japanese,
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Language::System => "System default",
            Language::English => "English",
            Language::SimplifiedChinese => "简体中文",
            Language::Japanese => "日本語",
        };
        write!(f, "{value}")
    }
}

impl Language {
    /// Converts to a BCP-47 locale string; `System` returns `None` to signal system detection.
    pub fn to_locale_str(self) -> Option<&'static str> {
        match self {
            Language::System => None,
            Language::English => Some("en"),
            Language::SimplifiedChinese => Some("zh-CN"),
            Language::Japanese => Some("ja"),
        }
    }

    /// The English language name injected into prompt templates ("English" / "Simplified Chinese" / "Japanese").
    ///
    /// `System` is resolved via the i18n loader's current locale — users with a Chinese/Japanese
    /// system locale who haven't explicitly overridden it should still get CJK output (see the
    /// #276/#277 fix history).
    pub fn prompt_language_name(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::SimplifiedChinese => "Simplified Chinese",
            Language::Japanese => "Japanese",
            Language::System => {
                let locale = crate::i18n::current_languages()
                    .into_iter()
                    .next()
                    .map(|l| l.to_string())
                    .unwrap_or_default();
                if locale.starts_with("zh") {
                    "Simplified Chinese"
                } else if locale.starts_with("ja") {
                    "Japanese"
                } else {
                    "English"
                }
            }
        }
    }
}

define_settings_group!(LanguageSettings, settings: [
    language: LanguageState {
        type: Language,
        default: Language::System,
        supported_platforms: SupportedPlatforms::ALL,
        sync_to_cloud: SyncToCloud::Never,
        private: false,
        storage_key: "Language",
        toml_path: "appearance.language",
        description: "The language used in Zap's user interface. Falls back to English when the chosen language is not fully translated.",
    },
]);
