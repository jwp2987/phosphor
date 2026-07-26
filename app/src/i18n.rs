//! Fluent-based localization layer for Zap Desktop.
//!
//! Loading chain:
//!   1. `init()` is called once at startup (idempotent), using `RustEmbed` to
//!      load `app/i18n/{locale}/*.ftl`
//!   2. `LANGUAGE_LOADER` is a global `OnceLock<FluentLanguageLoader>` that
//!      selects the current locale on the fallback chain (defaults to the
//!      system locale, overridable by settings)
//!   3. Call sites use `t!("key")` / `t!("key", name = ..)` to get strings;
//!      missing keys automatically fall back to English
//!
//! When a key is missing:
//!   - Not present in the current locale → fluent internally falls back to
//!     fallback_language (en)
//!   - Not even present in English → returns the key string itself (and
//!     `log::warn`s, to make it easy for CI to catch untranslated entries)

#[cfg(not(target_os = "macos"))]
use i18n_embed::DesktopLanguageRequester;
use i18n_embed::{
    fluent::{fluent_language_loader, FluentLanguageLoader},
    LanguageLoader,
};
use rust_embed::RustEmbed;
use std::sync::OnceLock;
use unic_langid::LanguageIdentifier;

/// Embeds the `app/i18n` directory into the binary. Re-embedded on every build (the debug-embed feature is already enabled workspace-wide).
#[derive(RustEmbed)]
#[folder = "i18n/"]
struct Localizations;

static LANGUAGE_LOADER: OnceLock<FluentLanguageLoader> = OnceLock::new();

/// Called once, early in app startup.
///
/// `override_locale`: the language the user explicitly chose in Settings
/// (e.g. "zh-CN"); when `None`, follows the system locale.
/// Never panics — a load failure falls back to the built-in English bundle.
pub fn init(override_locale: Option<&str>) {
    if LANGUAGE_LOADER.get().is_some() {
        return;
    }

    let loader = fluent_language_loader!();

    // Always load the fallback (en) bundle first — any locale missing a key falls back to it.
    if let Err(e) = loader.load_fallback_language(&Localizations) {
        log::error!("[i18n] failed to load fallback (en) bundle: {e}");
    }

    // Determine the runtime locale list (in priority order).
    let requested: Vec<LanguageIdentifier> = match override_locale {
        Some(s) => match s.parse::<LanguageIdentifier>() {
            Ok(li) => vec![li],
            Err(e) => {
                log::warn!("[i18n] invalid override_locale {s:?}: {e} — falling back to system");
                system_requested_languages()
            }
        },
        None => system_requested_languages(),
    };

    if let Err(e) = i18n_embed::select(&loader, &Localizations, &requested) {
        log::warn!("[i18n] select() failed: {e} — running with fallback only");
    }

    // Don't wrap `{$variable}` interpolations in Unicode bidi isolates (FSI/PDI).
    // Those marks matter for RTL scripts, but Zap ships only en / zh-CN / ja
    // (all LTR), where they just inject invisible U+2068/U+2069 into displayed
    // text (and break exact-match assertions). Applies to the bundles loaded
    // above.
    loader.set_use_isolating(false);

    log::info!(
        "[i18n] initialized; current_languages={:?}, fallback={}",
        loader.current_languages(),
        loader.fallback_language()
    );

    propagate_ui_locale(&loader);

    let _ = LANGUAGE_LOADER.set(loader);
}

/// Forward the resolved UI locale to `warpui::set_ui_locale` so DirectWrite / CoreText
/// glyph fallback biases CJK Han characters toward the user's UI language. Japanese,
/// Simplified Chinese, and Traditional Chinese share Han code points; without a locale
/// hint, DirectWrite tends to pick Microsoft YaHei (Simplified Chinese) on Windows even
/// when the UI is rendered in Japanese.
fn propagate_ui_locale(loader: &FluentLanguageLoader) {
    let langs = loader.current_languages();
    if let Some(li) = langs.first() {
        warpui::set_ui_locale(li.to_string());
    }
}

fn system_requested_languages() -> Vec<LanguageIdentifier> {
    #[cfg(target_os = "macos")]
    {
        macos_requested_languages()
    }

    #[cfg(not(target_os = "macos"))]
    {
        DesktopLanguageRequester::requested_languages()
    }
}

#[cfg(target_os = "macos")]
fn macos_requested_languages() -> Vec<LanguageIdentifier> {
    use objc::{class, msg_send, runtime::Object, sel, sel_impl};
    use warpui::platform::mac::utils::nsstring_as_str;

    unsafe {
        let locale_class = class!(NSLocale);
        let preferred_languages: *const Object = msg_send![locale_class, preferredLanguages];
        let count: usize = msg_send![preferred_languages, count];

        let mut requested = Vec::with_capacity(count);
        for index in 0..count {
            let language: *const Object = msg_send![preferred_languages, objectAtIndex: index];
            match nsstring_as_str(language) {
                Ok(language) => {
                    if let Some(language) = parse_language_identifier(language) {
                        requested.push(language);
                    }
                }
                Err(err) => {
                    log::warn!(
                        "[i18n] failed to read macOS preferred language at index {index}: {err}"
                    );
                }
            }
        }

        languages_or_fallback(requested)
    }
}

fn parse_language_identifier(language: &str) -> Option<LanguageIdentifier> {
    match language.parse::<LanguageIdentifier>() {
        Ok(language) => Some(language),
        Err(err) => {
            log::warn!("[i18n] invalid language identifier {language:?}: {err}");
            None
        }
    }
}

fn languages_or_fallback(languages: Vec<LanguageIdentifier>) -> Vec<LanguageIdentifier> {
    if languages.is_empty() {
        vec![fallback_language()]
    } else {
        languages
    }
}

fn fallback_language() -> LanguageIdentifier {
    "en".parse().expect("en is a valid language identifier")
}

/// Gets the global loader. Returns `None` if `init()` hasn't been called yet (early/test code can fall back to [`t_or`]).
pub fn loader() -> Option<&'static FluentLanguageLoader> {
    LANGUAGE_LOADER.get()
}

/// Switches the runtime locale (callable at any time after `init()`).
///
/// Implementation detail: `FluentLanguageLoader::load_languages` internally
/// protects language data with an RwLock, so `&loader` can be hot-swapped
/// without rebuilding. But **already-rendered UI text does not refresh
/// automatically** — `t!()` returns a `String` copied at call time; seeing the
/// new language requires a view rebuild/redraw. The caller can decide whether
/// to trigger a global redraw, or prompt the user to restart.
///
/// `locale` takes BCP-47 (e.g. `"en"`, `"zh-CN"`). On failure the original locale is kept, a warning is logged, and it never panics.
pub fn set_locale(locale: &str) {
    let Some(loader) = LANGUAGE_LOADER.get() else {
        log::warn!("[i18n] set_locale({locale:?}) called before init() — ignoring");
        return;
    };
    let lang_id: LanguageIdentifier = match locale.parse() {
        Ok(li) => li,
        Err(e) => {
            log::warn!("[i18n] set_locale({locale:?}): invalid BCP-47: {e}");
            return;
        }
    };
    if let Err(e) = loader.load_languages(&Localizations, &[lang_id]) {
        log::warn!("[i18n] set_locale({locale:?}) failed: {e}");
        return;
    }
    log::info!(
        "[i18n] locale switched to {locale:?}; current_languages={:?}",
        loader.current_languages()
    );
    propagate_ui_locale(loader);
}

/// Resets back to the system language (undoes an explicit override).
pub fn reset_to_system_locale() {
    let Some(loader) = LANGUAGE_LOADER.get() else {
        return;
    };
    let requested = system_requested_languages();
    if let Err(e) = i18n_embed::select(loader, &Localizations, &requested) {
        log::warn!("[i18n] reset_to_system_locale failed: {e}");
    }
    propagate_ui_locale(loader);
}

/// Gets the list of currently active languages (primary + fallback). For debug / settings UI display only.
pub fn current_languages() -> Vec<LanguageIdentifier> {
    LANGUAGE_LOADER
        .get()
        .map(|l| l.current_languages())
        .unwrap_or_default()
}

/// The main call-site entry point: `t!("key")` or `t!("key", name = value, count = 3)`.
///
/// - Wraps `i18n_embed_fl::fl!`, but adds a fallback for "loader not yet
///   initialized": returns the key itself, avoiding a panic
/// - Returns a `String` (fed directly to GPUI Text/label_text, no extra conversion needed)
#[macro_export]
macro_rules! t {
    ($message_id:literal $(,)?) => {{
        match $crate::i18n::loader() {
            Some(loader) => ::i18n_embed_fl::fl!(loader, $message_id),
            None => {
                ::log::warn!(
                    "[i18n] t!({:?}) called before init(); returning key as-is",
                    $message_id
                );
                String::from($message_id)
            }
        }
    }};
    ($message_id:literal, $($args:tt)*) => {{
        match $crate::i18n::loader() {
            Some(loader) => ::i18n_embed_fl::fl!(loader, $message_id, $($args)*),
            None => {
                ::log::warn!(
                    "[i18n] t!({:?}, ...) called before init(); returning key as-is",
                    $message_id
                );
                String::from($message_id)
            }
        }
    }};
}

/// Equivalent to `t!`, but returns a `&'static str` (each call permanently
/// occupies a chunk of heap via `Box::leak`).
///
/// Usage constraint: **only call this inside `LazyLock`/one-time
/// initialization** (e.g. a `StaticCommand`-style struct whose field is
/// `&'static str` but must pull its text from fluent). **Do not use in hot
/// paths or loops**, or it will keep leaking memory. Still gets `fl!()`'s
/// compile-time key validation.
#[macro_export]
macro_rules! t_static {
    ($message_id:literal $(,)?) => {{
        let s: String = $crate::t!($message_id);
        &*::std::boxed::Box::leak(s.into_boxed_str())
    }};
}

/// Same as `t!` but with an explicit default value, suited for very-early / loader-not-ready scenarios.
pub fn t_or(message_id: &str, fallback: &str) -> String {
    match LANGUAGE_LOADER.get() {
        Some(loader) if loader.has(message_id) => loader.get(message_id),
        _ => fallback.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_is_idempotent() {
        init(Some("en"));
        init(Some("en"));
        assert!(loader().is_some());
    }

    #[test]
    fn fallback_chain_works() {
        // Build a LOCAL loader instead of `init()`'s global `OnceLock`: this test
        // selects a non-default locale (zh-CN), and going through the global would
        // poison the process-wide loader that every other test shares (and would
        // itself be order-dependent on whoever initialized the global first).
        let loader = fluent_language_loader!();
        loader
            .load_fallback_language(&Localizations)
            .expect("load fallback (en) bundle");
        i18n_embed::select(&loader, &Localizations, &["zh-CN".parse().unwrap()])
            .expect("select zh-CN");
        // common-ok is translated in Chinese.
        assert_eq!(loader.get("common-ok"), "确定");
        // A missing key: fluent returns the key itself or a marked string.
        let missing = loader.get("definitely-does-not-exist");
        assert!(missing.contains("definitely-does-not-exist"));
    }

    #[test]
    fn requested_languages_keep_preferred_order() {
        let languages = ["ja", "zh-CN"]
            .into_iter()
            .filter_map(parse_language_identifier)
            .collect();

        let languages = languages_or_fallback(languages);

        assert_eq!(languages[0].to_string(), "ja");
        assert_eq!(languages[1].to_string(), "zh-CN");
    }

    #[test]
    fn requested_languages_fall_back_to_english_when_empty() {
        let languages = languages_or_fallback(Vec::new());

        assert_eq!(languages.len(), 1);
        assert_eq!(languages[0].to_string(), "en");
    }
}
