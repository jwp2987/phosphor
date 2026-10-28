use std::fmt;

#[cfg(test)]
#[path = "redacted_secret_tests.rs"]
mod tests;

/// A secret string (an API key, an auth-header value, ...) whose `Debug` output never shows the
/// value.
///
/// warpui logs every dispatched typed action as `{action:?}` at Info, and the log file ends up
/// in user-shareable log bundles, so any action payload that carries a secret must wrap it in
/// this type (#1065). Unlike [`crate::user_input::UserInput`], which still shows its value in
/// debug builds, this redacts unconditionally: a key is never fit for a log, dev build or not.
///
/// There is deliberately no `Display` impl, so the value can only be read through
/// [`Self::expose`] / [`Self::into_inner`], which keeps every read site greppable.
#[derive(Clone, PartialEq, Eq, Default)]
pub struct RedactedSecret(String);

impl RedactedSecret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// The secret value. Never pass this to a log or a `Debug`/`Display` formatter.
    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for RedactedSecret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}
