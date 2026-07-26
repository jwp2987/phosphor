//! Zap local identity facade.
//!
//! This module keeps the type surface + pub method signatures of `AuthState` /
//! `AuthStateProvider` / `AuthManager` / `User` / `UserUid` / `Credentials`, but
//! **all method bodies are localized**:
//! - `is_logged_in()` / the various `is_*` predicates: always return the constant
//!   corresponding to the local user.
//! - `user_id()`: returns a constant [`UserUid`] based on `TEST_USER_UID`.
//! - `username_for_display` / `display_name`: based on placeholder metadata from
//!   [`User::test`].
//! - External-account callback entry points have been retired; nothing depends on
//!   a remote account client anymore.
//!
//! The 167 call sites of `crate::auth::AuthStateProvider::as_ref(ctx).get()` keep
//! compiling with zero changes, and at runtime they always get a local placeholder
//! state of "logged in, unlimited Free Tier".
//!
//! See the README for the physical-deletion list: 21 UI / RPC / token-persistence /
//! web-handoff files — login_slide / paste_auth_token_modal / web_handoff, etc. —
//! were removed together with the external account system.

use std::sync::Arc;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

use crate::server_time::ServerTimestamp;

pub const TEST_USER_EMAIL: &str = "test_user@warp.dev";
pub const TEST_USER_UID: &str = "test_user_uid";

pub mod user_uid;

pub use user_uid::UserUid;

#[derive(Clone, Copy, Debug)]
pub enum OwnerType {
    Team,
    User,
}

/// Zap's local API key prefix.
///
/// Historically used to identify "strings starting with wk- as a managed API
/// key"; there is no managed-account API key concept left on the BYOP path. The
/// constant is still consumed internally by `AuthState::initialize` plus a
/// handful of legacy call sites that match on the prefix, so it's kept.
pub const API_KEY_PREFIX: &str = "wk-";

// ---------- Credentials / AuthToken / LoginToken ----------
//
// Originally the runtime branch for a few authentication methods: managed
// token / API key / session cookie. After Zap's localization, only the
// `ApiKey` / `Test` variants that are actually used remain. The managed-token
// and cookie variants have been physically deleted; all former
// external-account branches under Zap always take the `None` path / return early.

/// Represents how a user authenticates with Zap.
///
/// Zap's localized branches:
/// - `ApiKey`: on the BYOP path the user brings their own LLM provider API key,
///   which is actually managed by settings/keychain respectively; this only
///   keeps the enum facade for `AuthState::credentials()` and other read
///   methods.
/// - `Test`: used under test / `skip_login` builds.
#[derive(Clone, Debug)]
pub enum Credentials {
    /// BYOP / Zap Inc API key; keeps `owner_type` around for old code to read
    /// (always `None`).
    ApiKey {
        key: String,
        owner_type: Option<OwnerType>,
    },
    /// Test / `skip_login` build placeholder.
    Test,
}

impl Credentials {
    /// Returns the API key string (only when the variant is [`Credentials::ApiKey`]).
    pub fn as_api_key(&self) -> Option<&str> {
        match self {
            Credentials::ApiKey { key, .. } => Some(key),
            Credentials::Test => None,
        }
    }

    /// Returns the API key owner type (always `None` on the Zap path).
    pub fn api_key_owner_type(&self) -> Option<OwnerType> {
        match self {
            Credentials::ApiKey { owner_type, .. } => *owner_type,
            Credentials::Test => None,
        }
    }

    /// Returns the bearer token to write into the Authorization header.
    ///
    /// After localization only `ApiKey` produces a real value; `Test` returns
    /// [`AuthToken::NoAuth`].
    pub fn bearer_token(&self) -> AuthToken {
        match self {
            Credentials::ApiKey { key, .. } => AuthToken::ApiKey(key.clone()),
            Credentials::Test => AuthToken::NoAuth,
        }
    }
}

/// Short-lived token used in HTTP request headers.
#[derive(Debug, Clone)]
pub enum AuthToken {
    /// BYOP / platform-layer API key.
    ApiKey(String),
    /// No token at all (session cookie / test / Zap local mode).
    NoAuth,
}

impl AuthToken {
    /// Returns the bearer token string, if any.
    pub fn bearer_token(&self) -> Option<String> {
        match self {
            AuthToken::ApiKey(key) => Some(key.clone()),
            AuthToken::NoAuth => None,
        }
    }

    /// Returns a reference to the token used in the Authorization header.
    pub fn as_bearer_token(&self) -> Option<&str> {
        match self {
            AuthToken::ApiKey(key) => Some(key),
            AuthToken::NoAuth => None,
        }
    }
}

// ---------- User metadata ----------

/// Anonymous user type facade. After Zap's localization there is no anonymous
/// user concept; the enum is kept so match arms scattered across
/// telemetry / settings still compile. No Zap code path ever constructs
/// `Some(AnonymousUserType::...)`.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AnonymousUserType {
    NativeClientAnonymousUser,
    NativeClientAnonymousUserFeatureGated,
    WebClientAnonymousUser,
}

/// Auth principal type facade. Always equal to `User` under Zap.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum PrincipalType {
    #[default]
    User,
    ServiceAccount,
}

/// Personal object limits facade (originally the anonymous user's Free Tier
/// limits). Zap never constructs this value, but keeps the struct so
/// consumers keep compiling.
#[derive(Debug, Copy, Clone, Serialize, Deserialize)]
pub struct PersonalObjectLimits {
    pub env_var_limit: usize,
    pub notebook_limit: usize,
    pub workflow_limit: usize,
}

/// User metadata facade; only keeps the handful of fields used by
/// telemetry / display.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct UserMetadata {
    pub email: String,
    pub display_name: Option<String>,
    pub photo_url: Option<String>,
}

/// The current logged-in user (local placeholder).
#[derive(Debug, Clone)]
pub struct User {
    pub local_id: UserUid,
    pub metadata: UserMetadata,
    pub is_onboarded: bool,
    pub needs_sso_link: bool,
    pub anonymous_user_type: Option<AnonymousUserType>,
    pub is_on_work_domain: bool,
    pub linked_at: Option<ServerTimestamp>,
    pub personal_object_limits: Option<PersonalObjectLimits>,
    pub principal_type: PrincipalType,
}

impl User {
    /// Username for display — display_name takes priority, otherwise email.
    pub fn username_for_display(&self) -> &str {
        self.metadata
            .display_name
            .as_deref()
            .unwrap_or(self.metadata.email.as_str())
    }

    /// The user's display name, without falling back to email.
    pub fn display_name(&self) -> Option<String> {
        self.metadata.display_name.clone()
    }

    /// Test/default user placeholder. Zap uses this user on every code path.
    pub fn test() -> Self {
        Self {
            local_id: UserUid::new(TEST_USER_UID),
            metadata: UserMetadata {
                email: TEST_USER_EMAIL.to_string(),
                display_name: None,
                photo_url: None,
            },
            is_onboarded: true,
            needs_sso_link: false,
            anonymous_user_type: None,
            is_on_work_domain: false,
            linked_at: None,
            personal_object_limits: None,
            principal_type: PrincipalType::User,
        }
    }

    /// Whether the user is anonymous. Zap always returns `false`.
    pub fn is_user_anonymous(&self) -> bool {
        false
    }

    pub fn anonymous_user_type(&self) -> Option<AnonymousUserType> {
        self.anonymous_user_type
    }

    pub fn personal_object_limits(&self) -> Option<PersonalObjectLimits> {
        self.personal_object_limits
    }

    pub fn linked_at(&self) -> Option<ServerTimestamp> {
        self.linked_at
    }
}

// ---------- AuthState ----------

/// Current auth state (localized stub).
///
/// All queries about "logged in, anonymous, needs reauth" return fixed values;
/// `user_id()` always returns `Some(UserUid::new(TEST_USER_UID))`.
/// All 167+ consumers compile with zero changes.
pub struct AuthState {
    user: RwLock<Option<User>>,
    credentials: RwLock<Option<Credentials>>,
}

impl Default for AuthState {
    fn default() -> Self {
        Self::new_for_test()
    }
}

impl AuthState {
    /// Creates the local default AuthState (always treated as the logged-in test user).
    pub fn new() -> Self {
        Self {
            user: RwLock::new(Some(User::test())),
            credentials: RwLock::new(Some(Credentials::Test)),
        }
    }

    /// Constructs an AuthState for test scenarios (equivalent to [`AuthState::new`]).
    pub fn new_for_test() -> Self {
        Self::new()
    }

    /// Initializes AuthState. The `api_key` parameter is faithfully kept (the
    /// BYOP entry point may still pass one in), but every other external-account
    /// check path is a no-op.
    #[cfg_attr(target_family = "wasm", allow(unused_variables))]
    pub fn initialize(_ctx: &AppContext, api_key: Option<String>) -> Self {
        let state = Self::new();
        if let Some(api_key_value) = api_key {
            let formatted = if api_key_value.starts_with(API_KEY_PREFIX) {
                api_key_value
            } else {
                format!("{API_KEY_PREFIX}{api_key_value}")
            };
            *state.credentials.write() = Some(Credentials::ApiKey {
                key: formatted,
                owner_type: None,
            });
        }
        state
    }

    /// Whether the user is logged in. Always `true` under Zap.
    pub fn is_logged_in(&self) -> bool {
        true
    }

    /// Whether anonymous or logged out. Always `false` under Zap.
    pub fn is_anonymous_or_logged_out(&self) -> bool {
        false
    }

    /// Returns the cached access token (ignoring validity). Under Zap this only
    /// has a value if the user has `Credentials::ApiKey` attached.
    pub fn get_access_token_ignoring_validity(&self) -> Option<String> {
        self.credentials
            .read()
            .as_ref()?
            .bearer_token()
            .bearer_token()
    }

    pub fn username_for_display(&self) -> Option<String> {
        Some(self.user.read().as_ref()?.username_for_display().to_owned())
    }

    pub fn display_name(&self) -> Option<String> {
        self.user
            .read()
            .as_ref()
            .and_then(|user| user.display_name())
    }

    pub fn user_email(&self) -> Option<String> {
        self.user
            .read()
            .as_ref()
            .map(|user| user.metadata.email.clone())
    }

    pub fn is_onboarded(&self) -> Option<bool> {
        self.user.read().as_ref().map(|user| user.is_onboarded)
    }

    pub fn user_email_domain(&self) -> Option<String> {
        self.user.read().as_ref().map(|user| {
            user.metadata
                .email
                .split('@')
                .nth(1)
                .unwrap_or("")
                .to_string()
        })
    }

    pub fn is_user_anonymous(&self) -> Option<bool> {
        Some(false)
    }

    pub fn is_user_web_anonymous_user(&self) -> Option<bool> {
        Some(false)
    }

    pub fn is_anonymous_user_feature_gated(&self) -> Option<bool> {
        Some(false)
    }

    /// Zap's local user can never hit the Free Tier limit.
    pub fn is_anonymous_user_past_object_limit(
        &self,
        _object_type: crate::cloud_object::ObjectType,
        _num_objects: usize,
    ) -> Option<bool> {
        Some(false)
    }

    pub fn user_photo_url(&self) -> Option<String> {
        self.user
            .read()
            .as_ref()
            .and_then(|user| user.metadata.photo_url.clone())
    }

    pub fn needs_sso_link(&self) -> Option<bool> {
        Some(false)
    }

    pub fn anonymous_user_type(&self) -> Option<AnonymousUserType> {
        None
    }

    pub fn personal_object_limits(&self) -> Option<PersonalObjectLimits> {
        None
    }

    /// Marks the user as onboarded.
    pub fn set_is_onboarded(&self, is_onboarded: bool) {
        if let Some(user) = self.user.write().as_mut() {
            user.is_onboarded = is_onboarded;
        }
    }

    pub fn user_id(&self) -> Option<UserUid> {
        self.user.read().as_ref().map(|user| user.local_id)
    }

    /// Returns the nil UUID string. After Zap's localization this ID no longer
    /// appears in any outgoing HTTP header; it only serves as a formal
    /// placeholder for telemetry context / session headers.
    pub fn anonymous_id(&self) -> String {
        Uuid::nil().to_string()
    }

    /// Returns whether reauthentication is needed. Always `false` under Zap.
    pub fn needs_reauth(&self) -> bool {
        false
    }

    /// Returns whether the current user's anonymous renotification block has
    /// expired. Zap users aren't treated as anonymous users, so this function
    /// returns `false` (the sign-up prompt is never shown).
    pub fn anonymous_user_renotification_block_expired(
        &self,
        _last_time_opt: Option<String>,
    ) -> bool {
        false
    }

    pub fn is_on_work_domain(&self) -> Option<bool> {
        Some(false)
    }

    pub fn is_api_key_authenticated(&self) -> bool {
        matches!(
            self.credentials.read().as_ref(),
            Some(Credentials::ApiKey { .. })
        )
    }

    pub fn api_key(&self) -> Option<String> {
        self.credentials
            .read()
            .as_ref()
            .and_then(|c| c.as_api_key().map(|s| s.to_owned()))
    }

    pub fn principal_type(&self) -> Option<PrincipalType> {
        Some(PrincipalType::User)
    }

    pub fn is_service_account(&self) -> bool {
        false
    }

    pub fn api_key_owner_type(&self) -> Option<OwnerType> {
        self.credentials.read().as_ref()?.api_key_owner_type()
    }

    /// Returns a clone of the current credentials.
    pub fn credentials(&self) -> Option<Credentials> {
        self.credentials.read().clone()
    }

    /// Resets the local auth state back to the local placeholder user's default
    /// snapshot; used by `log_out` and other local-reset paths.
    pub fn reset_local_defaults(&self) {
        *self.user.write() = Some(User::test());
        *self.credentials.write() = Some(Credentials::Test);
    }
}

impl warp_managed_secrets::ActorProvider for AuthState {
    fn actor_uid(&self) -> Option<String> {
        self.user_id().map(|uid| uid.as_string())
    }
}

/// Singleton wrapper for AuthState.
pub struct AuthStateProvider {
    auth_state: Arc<AuthState>,
}

impl AuthStateProvider {
    pub fn new(auth_state: Arc<AuthState>) -> Self {
        Self { auth_state }
    }

    pub fn new_for_test() -> Self {
        Self {
            auth_state: Arc::new(AuthState::new_for_test()),
        }
    }

    /// Constructs a "logged out" AuthState provider.
    ///
    /// Zap no longer has a real logged-out state; this function returns a
    /// provider equivalent to `new_for_test` ("logged-in test user") so old
    /// test code keeps compiling.
    pub fn new_logged_out_for_test() -> Self {
        Self::new_for_test()
    }

    pub fn get(&self) -> &Arc<AuthState> {
        &self.auth_state
    }
}

impl Entity for AuthStateProvider {
    type Event = ();
}

impl SingletonEntity for AuthStateProvider {}

// ---------- AuthManager facade ----------

/// A string-constant identifier for "login gated" left over from the old UI
/// (originally an `&'static str`).
pub type LoginGatedFeature = &'static str;

/// URL-constructing callback for
/// `AuthManager::open_url_maybe_with_anonymous_token`.
///
/// In the original UI, this callback would receive an anonymous user token and
/// then assemble a URL that "opens a browser with identity attached". Under
/// Zap, anonymous identity no longer exists, so the callback is discarded.
pub type AnonymousTokenUrlBuilder = Box<dyn FnOnce(Option<&str>) -> String>;

/// AuthView variant facade. Zap has physically deleted the AuthView UI; every
/// dispatch site in the stub only produces a log, but the enum surface is kept
/// so old `match` arms keep compiling.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AuthViewVariant {
    Initial,
    RequireLoginCloseable,
    ShareRequirementCloseable,
}

// ---------- UI view facade (placeholder for the physically-removed UI) ----------
//
// `root_view.rs` / `workspace/view.rs` originally held 6 `ViewHandle<T>` fields,
// plus events originating from those views. After Wave 3-1 physically removed
// the view bodies, these view + event enum facades are kept so the
// `ViewHandle<AuthView>` type, event match arms, and
// `ctx.add_typed_action_view(AuthView::new)` call still compile.
//
// At runtime these view code paths are still created but not rendered
// (`View::render` returns `Empty`), and events are never triggered (the
// original UI interaction points no longer exist).

use warpui::elements::Empty;
use warpui::{Element, View, ViewContext};

/// AuthView facade. The original UI contained the "sign in / sign up" form;
/// physically removed after localization.
pub struct AuthView {
    variant: AuthViewVariant,
}

impl AuthView {
    pub fn new(variant: AuthViewVariant, _ctx: &mut ViewContext<Self>) -> Self {
        Self { variant }
    }

    pub fn set_variant(&mut self, _ctx: &mut ViewContext<Self>, variant: AuthViewVariant) {
        self.variant = variant;
    }

    /// Returns the current variant. Not used on the Zap path.
    pub fn variant(&self) -> AuthViewVariant {
        self.variant
    }

    /// The original native login UI skipped the "enter passcode" step straight
    /// into the following "open in browser" step. Zap: no-op.
    pub fn skip_to_browser_open_step(&mut self, _ctx: &mut ViewContext<Self>) {}
}

impl Entity for AuthView {
    type Event = AuthViewEvent;
}

impl View for AuthView {
    fn ui_name() -> &'static str {
        "AuthView (stub)"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(Empty::new())
    }
}

impl warpui::TypedActionView for AuthView {
    type Action = ();
    fn handle_action(&mut self, _action: &(), _ctx: &mut ViewContext<Self>) {}
}

#[derive(Debug)]
pub enum AuthViewEvent {
    Close,
}

/// AuthOverrideWarningModal facade.
pub struct AuthOverrideWarningModal;

impl AuthOverrideWarningModal {
    pub fn new(_ctx: &mut ViewContext<Self>, _variant: AuthOverrideWarningModalVariant) -> Self {
        Self
    }
}

impl Entity for AuthOverrideWarningModal {
    type Event = AuthOverrideWarningModalEvent;
}

impl View for AuthOverrideWarningModal {
    fn ui_name() -> &'static str {
        "AuthOverrideWarningModal (stub)"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(Empty::new())
    }
}

impl warpui::TypedActionView for AuthOverrideWarningModal {
    type Action = ();
    fn handle_action(&mut self, _action: &(), _ctx: &mut ViewContext<Self>) {}
}

#[derive(Debug)]
pub enum AuthOverrideWarningModalEvent {
    Close,
    BulkExport,
}

#[derive(Clone, Copy, Debug)]
pub enum AuthOverrideWarningModalVariant {
    OnboardingView,
    WorkspaceModal,
}

/// NeedsSsoLinkView facade.
pub struct NeedsSsoLinkView;

impl NeedsSsoLinkView {
    pub fn new() -> Self {
        Self
    }

    pub fn set_email(&mut self, _email: String) {}
}

impl Default for NeedsSsoLinkView {
    fn default() -> Self {
        Self::new()
    }
}

impl Entity for NeedsSsoLinkView {
    type Event = ();
}

impl View for NeedsSsoLinkView {
    fn ui_name() -> &'static str {
        "NeedsSsoLinkView (stub)"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(Empty::new())
    }
}

impl warpui::TypedActionView for NeedsSsoLinkView {
    type Action = ();
    fn handle_action(&mut self, _action: &(), _ctx: &mut ViewContext<Self>) {}
}

/// WebHandoffView facade (wasm-only re-login entry point).
pub struct WebHandoffView;

impl WebHandoffView {
    pub fn new(_ctx: &mut ViewContext<Self>) -> Self {
        Self
    }
}

impl Entity for WebHandoffView {
    type Event = WebHandoffEvent;
}

impl View for WebHandoffView {
    fn ui_name() -> &'static str {
        "WebHandoffView (stub)"
    }

    fn render(&self, _app: &AppContext) -> Box<dyn Element> {
        Box::new(Empty::new())
    }
}

#[derive(Debug)]
pub enum WebHandoffEvent {
    Unsupported,
}

/// AuthManager event facade. `AuthManagerEvent::AuthComplete` can still be
/// triggered internally by `AuthManager::new` to remain compatible with some
/// subscribers that depend on an "authenticated" signal.
#[derive(Debug)]
pub enum AuthManagerEvent {
    AuthComplete,
    AuthFailed(UserAuthenticationError),
    SkippedLogin,
    NeedsReauth,
    AttemptedLoginGatedFeature {
        auth_view_variant: AuthViewVariant,
    },
    /// Low-frequency failure: same as above.
    CreateAnonymousUserFailed,
}

/// User authentication error facade. A handful of subscribers still match on
/// each variant, so the enum is kept; Zap no longer triggers construction of
/// any variant.
#[derive(Debug, thiserror::Error)]
pub enum UserAuthenticationError {
    #[error("Access token denied")]
    DeniedAccessToken,
    #[error("User account disabled")]
    UserAccountDisabled,
    #[error("Invalid state parameter")]
    InvalidStateParameter,
    #[error("Missing state parameter")]
    MissingStateParameter,
    #[error("Unexpected error: {0}")]
    Unexpected(anyhow::Error),
}

/// Server-persisted user privacy settings facade, still consumed by
/// `settings/privacy.rs`.
#[derive(Copy, Clone, Debug, Default)]
pub struct SyncedUserSettings {
    pub is_crash_reporting_enabled: bool,
    pub is_telemetry_enabled: bool,
}

/// Current user info persisted in the SQLite `current_user_information` table.
/// `persistence/sqlite.rs` and `persistence/mod.rs` still consume this struct,
/// so it's kept.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedCurrentUserInformation {
    pub email: String,
}

/// AuthManager facade. After Zap's localization every external-account/RPC
/// entry point becomes a no-op. `AuthManager` is still mounted as a singleton
/// model on the App, to keep `subscribe_to_model` / `handle(ctx).update(...)`
/// calls at zero changes, while retaining the local identity / onboarded flag /
/// logout reset semantics.
pub struct AuthManager {
    auth_state: Arc<AuthState>,
}

impl AuthManager {
    /// Creates the AuthManager. After localization it no longer accepts an
    /// external-account client parameter.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let auth_state = AuthStateProvider::as_ref(ctx).get().clone();
        Self { auth_state }
    }

    /// Constructs for test scenarios, equivalent to [`Self::new`].
    pub fn new_for_test(ctx: &mut ModelContext<Self>) -> Self {
        Self::new(ctx)
    }

    /// Refreshes the current user state.
    ///
    /// Historically this would refresh a cloud token; after Zap's localization,
    /// the auth state has already completed local initialization at startup, so
    /// no external-account request is sent anymore.
    pub fn refresh_user(&self, _ctx: &mut ModelContext<Self>) {}

    /// Actively logs out.
    ///
    /// Zap no longer enters a "logged out in the cloud" state; this simply
    /// resets the local identity snapshot back to the default placeholder user,
    /// for reuse by settings-reset / session-cleanup call sites, etc.
    pub(crate) fn log_out(&mut self, _ctx: &mut ModelContext<Self>) {
        self.auth_state.reset_local_defaults();
        log::debug!("AuthManager::log_out performed a local reset: switched to the local placeholder user state");
    }

    /// Marks that reauthentication is needed. Localized: no-op.
    pub fn set_needs_reauth(&mut self, _new_value: bool, _ctx: &mut ModelContext<Self>) {}

    /// Creates an anonymous user. Localized: no-op — directly emits
    /// `AuthComplete` so the onboarding flow can proceed.
    pub fn create_anonymous_user(
        &mut self,
        _referral_code: Option<String>,
        ctx: &mut ModelContext<Self>,
    ) {
        ctx.emit(AuthManagerEvent::AuthComplete);
    }

    /// Dispatches "an anonymous user attempted to touch a login-gated feature".
    /// Localized: no-op.
    pub fn attempt_login_gated_feature(
        &mut self,
        _feature: LoginGatedFeature,
        _auth_view_variant: AuthViewVariant,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// An anonymous user hit the Drive object limit reminder. Localized: no-op.
    pub fn anonymous_user_hit_drive_object_limit(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Starts the anonymous-user-to-full-user browser login flow. Localized: no-op.
    pub fn initiate_anonymous_user_linking(
        &mut self,
        _entrypoint: crate::server::telemetry::AnonymousUserSignupEntrypoint,
        _ctx: &mut ModelContext<Self>,
    ) {
    }

    /// Sets the local onboarded flag once the user finishes onboarding.
    pub fn set_user_onboarded(&mut self, ctx: &mut ModelContext<Self>) {
        self.auth_state.set_is_onboarded(true);
        ctx.emit(AuthManagerEvent::AuthComplete);
    }

    // ---------- URL-construction facade ----------
    //
    // Before their physical removal, the old UI (login_slide /
    // paste_auth_token_modal / auth_view_modal) called these methods to fill in
    // legacy sign-in prompt links; Zap no longer opens the Zap cloud sign-in page.
    // There are no callers left after the physical UI removal, but the
    // enum/trait may still be consumed reflectively, so the stub is kept.

    pub fn sign_up_url(&self) -> String {
        String::new()
    }

    pub fn sign_in_url(&self) -> String {
        String::new()
    }

    pub fn upgrade_url(&self) -> String {
        String::new()
    }

    pub fn login_options_url(&self) -> String {
        String::new()
    }

    pub fn link_sso_url(&self) -> String {
        String::new()
    }

    /// Opens a URL in the browser, optionally attaching an anonymous token.
    /// Localized: no-op.
    pub fn open_url_maybe_with_anonymous_token(
        &mut self,
        _ctx: &mut ModelContext<Self>,
        _url_constructor: AnonymousTokenUrlBuilder,
    ) {
    }

    /// Copies the anonymous user's login link to the clipboard. Localized: no-op.
    pub fn copy_anonymous_user_linking_url_to_clipboard(&mut self, _ctx: &mut ModelContext<Self>) {}
}

impl Entity for AuthManager {
    type Event = AuthManagerEvent;
}

impl SingletonEntity for AuthManager {}

// ---------- Whole-module init ----------

/// Init for Zap's local identity facade (no-op).
///
/// The sub-modules originally mounted in `init` — `init` /
/// `auth_view_body::init` / `auth_override_warning_body::init` /
/// `login_slide::init` / `paste_auth_token_modal::init` — have all been
/// physically removed.
pub fn init(_app: &mut AppContext) {}
