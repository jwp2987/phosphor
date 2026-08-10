//! Zap (Phase 3c subtask A1): localized into a stub that's always "unlimited".
//!
//! Historical role: a "monthly AI request quota" model driven by warp.dev
//! server-side RPCs.
//! Zap goes through BYOP (Bring Your Own Provider), where the user pays the LLM
//! provider directly, and should never be constrained by cloud concepts like
//! "requests remaining / upgrade CTA / buy extra credits".
//!
//! Write-scope constraints:
//! * The 30+ UI subscription points
//!   (`subscribe_to_model(&AIRequestUsageModel::handle(ctx), ...)`) are kept; the
//!   events they subscribe to are simply never triggered by any path anymore → the
//!   subscription callbacks become permanently-silent no-ops.
//! * Files that use `RequestLimitInfo` / `RequestUsageInfo` / `BonusGrant` /
//!   `BonusGrantScope` / `RequestLimitRefreshDuration` /
//!   `BuyCreditsBannerDisplayState` / `AIRequestUsageModelEvent` /
//!   `AMBIENT_AGENT_TRIAL_CREDIT_THRESHOLD` spilling over
//!   (`workspaces/gql_convert.rs`, `ai_assistant/requests.rs`,
//!   `ai_assistant/mod.rs`, `settings/ai.rs`, `settings/ai_tests.rs`,
//!   `workspace/bonus_grant_notification_model.rs`, `settings_view/ai_page.rs`,
//!   `terminal/view/ambient_agent/first_time_setup.rs`,
//!   `agent_view/agent_message_bar.rs`) are outside this task's write scope → the
//!   stub must keep these type definitions and equivalent construction
//!   capabilities, only stripping out the RPC / cache / metering business logic.

use crate::{server_time::ServerTimestamp, workspaces::workspace::WorkspaceUid};
use chrono::{DateTime, Utc};
use instant::Instant;
use serde::{Deserialize, Serialize};
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BonusGrantType {
    AmbientOnly,
    Any,
}

/// Threshold of ambient-only credits at which we surface upgrade/CTA UI。
///
/// Zap: never reached in the localized scenario (since
/// `ambient_only_credits_remaining` is always `None`), but the constant definition
/// is kept for external-import compatibility.
pub const AMBIENT_AGENT_TRIAL_CREDIT_THRESHOLD: i32 = 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BonusGrantScope {
    User,
    Workspace(WorkspaceUid),
}

#[derive(Clone, Debug, PartialEq, Default)]
pub enum BuyCreditsBannerDisplayState {
    #[default]
    Hidden,
    OutOfCredits,
    MonthlyLimitReached,
}

#[derive(Clone, Debug)]
pub struct BonusGrant {
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub cost_cents: i32,
    pub expiration: Option<chrono::DateTime<chrono::Utc>>,
    pub grant_type: BonusGrantType,
    pub reason: String,
    pub user_facing_message: Option<String>,
    pub request_credits_granted: i32,
    pub request_credits_remaining: i32,
    pub scope: BonusGrantScope,
}

#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub enum RequestLimitRefreshDuration {
    Weekly,
    Monthly,
    EveryTwoWeeks,
}

/// History: a "monthly request quota" snapshot pushed by the server.
/// Zap: kept only as a type shell (files outside this task's write scope, like
/// `AISettings::update_quota_info` / `ai_assistant/requests.rs`, still construct
/// this struct). `AIRequestUsageModel` no longer holds / caches / updates it.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct RequestLimitInfo {
    pub limit: usize,
    pub num_requests_used_since_refresh: usize,
    pub next_refresh_time: ServerTimestamp,
    pub is_unlimited: bool,
    pub request_limit_refresh_duration: RequestLimitRefreshDuration,
    pub is_unlimited_voice: bool,
    #[serde(default)]
    pub voice_request_limit: usize,
    #[serde(default)]
    pub voice_requests_used_since_last_refresh: usize,
    #[serde(default)]
    pub max_files_per_repo: usize,
    #[serde(default)]
    pub embedding_generation_batch_size: usize,
}

fn default_voice_requests_limit() -> usize {
    10000
}

impl Default for RequestLimitInfo {
    /// Zap: no cloud quota, so the default value is treated as "unlimited".
    fn default() -> Self {
        Self {
            limit: usize::MAX,
            num_requests_used_since_refresh: 0,
            next_refresh_time: ServerTimestamp::new(Utc::now() + chrono::Duration::days(365)),
            is_unlimited: true,
            request_limit_refresh_duration: RequestLimitRefreshDuration::Monthly,
            is_unlimited_voice: true,
            voice_request_limit: default_voice_requests_limit(),
            voice_requests_used_since_last_refresh: 0,
            max_files_per_repo: usize::MAX,
            embedding_generation_batch_size: 100,
        }
    }
}

#[cfg(test)]
impl RequestLimitInfo {
    pub fn new_for_test(limit: usize, num_requests_used_since_refresh: usize) -> Self {
        Self {
            limit,
            num_requests_used_since_refresh,
            ..Self::default()
        }
    }
}

/// The limits `CodebaseIndexManager` sizes itself against.
///
/// Restored verbatim from the pin
/// (`02b53fcd8:app/src/ai/request_usage_model.rs:124`) with the codebase index.
/// `max_indices_allowed: None` means unlimited, which is the pin's own encoding.
pub struct CodebaseContextUsageLimit {
    pub max_files_per_repo: usize,
    pub max_indices_allowed: Option<usize>,
    pub embedding_generation_batch_size: usize,
}

/// History: the aggregate struct returned by the server's `getRequestLimitInfo`.
/// Zap: kept only as a type shell (`ai_assistant/requests.rs` still constructs
/// this type). `AIRequestUsageModel` no longer consumes it.
pub struct RequestUsageInfo {
    pub request_limit_info: RequestLimitInfo,
    pub bonus_grants: Vec<BonusGrant>,
}

/// Zap: the model no longer holds any state.
pub struct AIRequestUsageModel;

impl Entity for AIRequestUsageModel {
    type Event = AIRequestUsageModelEvent;
}

/// Zap: the enum definition is kept for compatibility with subscription callbacks'
/// `match` patterns; after localization, `AIRequestUsageModel` no longer emits any
/// variant → all subscription callbacks become silent no-ops.
pub enum AIRequestUsageModelEvent {
    RequestUsageUpdated,
    RequestBonusRefunded {
        requests_refunded: i32,
        server_conversation_id: String,
        request_id: String,
    },
}

impl AIRequestUsageModel {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self
    }

    #[cfg(test)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self
    }

    pub fn last_update_time(&self) -> Option<Instant> {
        None
    }

    /// Zap: no cloud backend, no-op.
    pub fn refresh_request_usage_async(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Zap (localization): always returns true; running BYOP locally isn't bound by any cloud quota.
    pub fn has_requests_remaining(&self) -> bool {
        true
    }

    /// Zap (localization): always returns true.
    /// AI availability only depends on whether the user has configured an API key
    /// (controlled independently by `ApiKeyManager`), and shouldn't be decided by
    /// cloud metering components like `request_limit_info`.
    pub fn has_any_ai_remaining(&self, _ctx: &AppContext) -> bool {
        true
    }

    /// Zap (localization): no cloud metering, fixed at returning 0.
    pub fn requests_used(&self) -> usize {
        0
    }

    /// Zap (localization): no cloud metering, fixed at returning 0.0.
    pub fn request_percentage_used(&self) -> f32 {
        0.0
    }

    /// Zap (localization): no cloud limit, fixed at returning `usize::MAX`.
    pub fn request_limit(&self) -> usize {
        usize::MAX
    }

    /// The limits the codebase index sizes itself against.
    ///
    /// Restored with the index from the pin
    /// (`02b53fcd8:app/src/ai/request_usage_model.rs:497`), where all three came
    /// from the signed-in user's subscription tier. There is no tier here, so
    /// the two quota-shaped values become "unlimited": `max_indices_allowed` is
    /// `None` (the pin's own encoding for unlimited) and `max_files_per_repo`
    /// keeps the fork's `RequestLimitInfo::default()` value of `usize::MAX`.
    ///
    /// `embedding_generation_batch_size` is *not* a quota — it is how many
    /// fragments go into one embedding request — so it keeps its real value.
    ///
    /// The pin read all three off `self.request_limit_info`, a cached copy of
    /// the server's `getRequestLimitInfo` response. This fork's
    /// `AIRequestUsageModel` is a unit struct that holds no state at all, so the
    /// values come from `RequestLimitInfo::default()`, which is where the fork
    /// already keeps its "no cloud quota" answers.
    pub fn codebase_context_limits(&self) -> CodebaseContextUsageLimit {
        let defaults = RequestLimitInfo::default();
        CodebaseContextUsageLimit {
            max_files_per_repo: defaults.max_files_per_repo,
            max_indices_allowed: None,
            embedding_generation_batch_size: defaults.embedding_generation_batch_size,
        }
    }

    /// Zap (localization): a far-future placeholder time.
    pub fn next_refresh_time(&self) -> DateTime<Utc> {
        Utc::now() + chrono::Duration::days(365)
    }

    /// Zap (localization): always unlimited.
    pub fn is_unlimited(&self) -> bool {
        true
    }

    pub fn refresh_duration_to_string(&self) -> String {
        "monthly".to_string()
    }

    /// Zap (localization): local users have no bonus grants.
    pub fn bonus_grants(&self) -> &[BonusGrant] {
        &[]
    }

    /// Zap (localization): local users have no concept of ambient-only credits.
    pub fn ambient_only_credits_remaining(&self) -> Option<i32> {
        None
    }

    /// Zap (localization): local users have no concept of workspace bonus credits.
    pub fn total_workspace_bonus_credits_remaining(&self, _uid: WorkspaceUid) -> i32 {
        0
    }

    /// Zap (localization): local users have no concept of workspace bonus credits.
    pub fn total_current_workspace_bonus_credits_remaining(&self, _ctx: &AppContext) -> i32 {
        0
    }

    /// Zap (localization): the "buy extra credits" business doesn't apply.
    pub fn compute_buy_addon_credits_banner_display_state(
        &self,
        _ctx: &AppContext,
    ) -> BuyCreditsBannerDisplayState {
        BuyCreditsBannerDisplayState::Hidden
    }

    /// Zap (localization): no-op.
    pub fn dismiss_buy_credits_banner(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Zap (localization): no-op.
    pub fn enable_buy_credits_banner(&mut self, _ctx: &mut ModelContext<Self>) {}

    /// Zap (localization): voice input isn't subject to cloud quota limits, always returns true.
    pub fn can_request_voice(&self) -> bool {
        true
    }
}

impl SingletonEntity for AIRequestUsageModel {}
