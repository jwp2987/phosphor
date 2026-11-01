pub use crate::aws_credentials::{AwsCredentials, AwsCredentialsState};
use serde::{Deserialize, Serialize};
use warp_multi_agent_api as api;
use warpui::{Entity, ModelContext, SingletonEntity};
use warpui_extras::secure_storage::{self, AppContextExt};

const SECURE_STORAGE_KEY: &str = "AiApiKeys";

/// Emitted when user-provided API keys are updated in-memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApiKeyManagerEvent {
    KeysUpdated,
}

/// User-provided API keys for AI providers.
///
/// These are used for "Bring Your Own API Key" functionality, allowing
/// users to use their own API keys instead of ones supplied by the
/// application.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ApiKeys {
    pub google: Option<String>,
    pub anthropic: Option<String>,
    pub openai: Option<String>,
    pub open_router: Option<String>,
}

impl ApiKeys {
    pub fn has_any_key(&self) -> bool {
        self.openai.is_some()
            || self.anthropic.is_some()
            || self.google.is_some()
            || self.open_router.is_some()
    }
}

/// Controls how AWS credentials are refreshed by [`ApiKeyManager`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum AwsCredentialsRefreshStrategy {
    /// Load credentials from the local AWS credential chain (~/.aws). This is the default.
    #[default]
    LocalChain,
    /// Credentials are managed externally via OIDC/STS.
    /// The task ID is used to scope the STS AssumeRoleWithWebIdentity session.
    /// The role ARN is the IAM role to assume via STS.
    OidcManaged {
        task_id: Option<String>,
        role_arn: String,
    },
}

/// A structure that manages API keys for AI providers.
pub struct ApiKeyManager {
    keys: ApiKeys,
    /// Edits made while the keychain read is in flight (#1309); re-applied on top of what the
    /// read returns, and only then persisted, so a partial in-memory set never overwrites the
    /// stored keys.
    pending: PendingKeyEdits,
    /// Whether a keychain read is in flight.
    loading: bool,
    /// Identifies the latest read; older results are stale and dropped.
    load_epoch: u64,
    /// Whether the first read has completed.
    loaded: bool,
    pub(crate) aws_credentials_state: AwsCredentialsState,
    aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy,
}

/// Per-field edits (`Some(new_value)`) not yet reconciled with the stored keys.
#[derive(Debug, Default)]
struct PendingKeyEdits {
    google: Option<Option<String>>,
    anthropic: Option<Option<String>>,
    openai: Option<Option<String>>,
    open_router: Option<Option<String>>,
}

impl PendingKeyEdits {
    fn is_empty(&self) -> bool {
        self.google.is_none()
            && self.anthropic.is_none()
            && self.openai.is_none()
            && self.open_router.is_none()
    }

    fn apply_to(self, keys: &mut ApiKeys) {
        if let Some(v) = self.google {
            keys.google = v;
        }
        if let Some(v) = self.anthropic {
            keys.anthropic = v;
        }
        if let Some(v) = self.openai {
            keys.openai = v;
        }
        if let Some(v) = self.open_router {
            keys.open_router = v;
        }
    }
}

impl ApiKeyManager {
    /// Starts reading the stored keys; they arrive shortly after construction, off the UI thread
    /// (#1309), and `KeysUpdated` is emitted when they do.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let mut me = Self {
            keys: ApiKeys::default(),
            pending: PendingKeyEdits::default(),
            loading: false,
            load_epoch: 0,
            loaded: false,
            aws_credentials_state: AwsCredentialsState::Missing,
            aws_credentials_refresh_strategy: AwsCredentialsRefreshStrategy::default(),
        };
        me.start_load(ctx);
        me
    }

    /// Whether the initial read of the stored keys has completed.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn keys(&self) -> &ApiKeys {
        &self.keys
    }

    pub fn set_google_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        if self.loading {
            self.pending.google = Some(key.clone());
        }
        self.keys.google = key;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_anthropic_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        if self.loading {
            self.pending.anthropic = Some(key.clone());
        }
        self.keys.anthropic = key;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_openai_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        if self.loading {
            self.pending.openai = Some(key.clone());
        }
        self.keys.openai = key;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_open_router_key(&mut self, key: Option<String>, ctx: &mut ModelContext<Self>) {
        if self.loading {
            self.pending.open_router = Some(key.clone());
        }
        self.keys.open_router = key;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        self.write_keys_to_secure_storage(ctx);
    }

    pub fn set_aws_credentials_state(
        &mut self,
        state: AwsCredentialsState,
        ctx: &mut ModelContext<Self>,
    ) {
        self.aws_credentials_state = state;
        ctx.emit(ApiKeyManagerEvent::KeysUpdated);
    }

    pub fn aws_credentials_state(&self) -> &AwsCredentialsState {
        &self.aws_credentials_state
    }

    pub fn aws_credentials_refresh_strategy(&self) -> AwsCredentialsRefreshStrategy {
        self.aws_credentials_refresh_strategy.clone()
    }

    pub fn set_aws_credentials_refresh_strategy(
        &mut self,
        strategy: AwsCredentialsRefreshStrategy,
    ) {
        self.aws_credentials_refresh_strategy = strategy;
    }

    pub fn api_keys_for_request(
        &self,
        include_byo_keys: bool,
        include_aws_bedrock_credentials: bool,
    ) -> Option<api::request::settings::ApiKeys> {
        let anthropic = include_byo_keys
            .then(|| self.keys.anthropic.clone())
            .flatten()
            .unwrap_or_default();
        let openai = include_byo_keys
            .then(|| self.keys.openai.clone())
            .flatten()
            .unwrap_or_default();
        let google = include_byo_keys
            .then(|| self.keys.google.clone())
            .flatten()
            .unwrap_or_default();
        let open_router = include_byo_keys
            .then(|| self.keys.open_router.clone())
            .flatten()
            .unwrap_or_default();
        // Also include credentials when running with OIDC-managed Bedrock inference, regardless
        // of the per-user setting flag (which only applies to the local credential chain path).
        let include_aws = include_aws_bedrock_credentials
            || matches!(
                self.aws_credentials_refresh_strategy,
                AwsCredentialsRefreshStrategy::OidcManaged { .. }
            );
        let aws_credentials = include_aws
            .then(|| match self.aws_credentials_state {
                AwsCredentialsState::Loaded {
                    ref credentials, ..
                } => Some(credentials.clone().into()),
                _ => None,
            })
            .flatten();

        if anthropic.is_empty()
            && openai.is_empty()
            && google.is_empty()
            && open_router.is_empty()
            && aws_credentials.is_none()
        {
            None
        } else {
            Some(api::request::settings::ApiKeys {
                anthropic,
                openai,
                google,
                open_router,
                allow_use_of_warp_credits: false,
                aws_credentials,
                // Both fields exist only to authenticate Warp's *server-side* harnesses:
                // `google_cloud_credentials` carries the Gemini Enterprise Agent Platform
                // binding and `grok_oauth_access_token` a connected SuperGrok subscription.
                // This fork has neither flow, so it sends them empty.
                google_cloud_credentials: None,
                grok_oauth_access_token: String::new(),
            })
        }
    }

    /// Re-reads the keys from secure storage, discarding the in-memory copy.
    ///
    /// [`Self::new`] reads secure storage once, at construction. That is
    /// enough while a process is the only writer, but a key can also be
    /// written by a *different* process sharing the same keyring namespace,
    /// and without this an already-running app keeps serving the keys it read
    /// at startup so the newly-saved key appears to have been ignored.
    ///
    /// As of #629 this store has **no writer left in this fork**: its only one
    /// was `zap-tui --set-provider-api-key` / `--clear-provider-api-key`, and
    /// both flags are now refused (`session::reject_provider_api_key_flags`)
    /// because a key here cannot reach the agent. The hook is kept wired
    /// anyway -- it costs one re-read on a revision bump, it is what makes this
    /// store behave correctly if a writer is ever reintroduced, and removing it
    /// would leave `AgentProviderSecrets` as the lone subscriber to a mechanism
    /// documented as serving both. See `app/src/ai/tui_api_keys.rs`.
    ///
    /// Emits [`ApiKeyManagerEvent::KeysUpdated`] unconditionally, matching the
    /// setters above: subscribers re-derive from `keys()` rather than diffing.
    pub fn reload_keys_from_secure_storage(&mut self, ctx: &mut ModelContext<Self>) {
        self.start_load(ctx);
    }

    fn start_load(&mut self, ctx: &mut ModelContext<Self>) {
        self.load_epoch += 1;
        self.loading = true;
        let epoch = self.load_epoch;
        match secure_storage::background::run_or_inline(
            |storage| Self::read_keys(storage),
            ctx.secure_storage(),
        ) {
            Ok(result) => {
                ctx.spawn(result.wait(), move |me, keys, ctx| {
                    if let Some(keys) = keys {
                        me.finish_load(epoch, keys, true, ctx);
                    }
                });
            }
            // Synchronous backend: during construction nothing can be subscribed yet and the
            // model is not registered, so don't emit then.
            Err(keys) => {
                let emit = self.loaded;
                self.finish_load(epoch, keys, emit, ctx);
            }
        }
    }

    /// Adopts a completed read, unless a newer one was started since.
    fn finish_load(
        &mut self,
        epoch: u64,
        mut keys: ApiKeys,
        emit: bool,
        ctx: &mut ModelContext<Self>,
    ) {
        if epoch != self.load_epoch {
            return;
        }
        self.loading = false;
        self.loaded = true;
        let pending = std::mem::take(&mut self.pending);
        let had_pending = !pending.is_empty();
        pending.apply_to(&mut keys);
        self.keys = keys;
        if emit {
            ctx.emit(ApiKeyManagerEvent::KeysUpdated);
        }
        if had_pending {
            self.write_keys_to_secure_storage(ctx);
        }
    }

    fn read_keys(storage: &dyn secure_storage::SecureStorage) -> ApiKeys {
        let key_json = match storage.read_value(SECURE_STORAGE_KEY) {
            Ok(json) => json,
            Err(e) => {
                if !matches!(e, secure_storage::Error::NotFound) {
                    log::error!("Failed to read API keys from secure storage: {e:#}");
                }
                return ApiKeys::default();
            }
        };

        let keys = match serde_json::from_str(&key_json) {
            Ok(keys) => keys,
            Err(e) => {
                log::error!("Failed to deserialize API keys: {e:#}");
                ApiKeys::default()
            }
        };

        keys
    }

    /// The single choke point for every write to this store: all four setters
    /// funnel through it, so the cross-process notification is stamped here
    /// rather than at each call site.
    fn write_keys_to_secure_storage(&mut self, ctx: &mut ModelContext<Self>) {
        // Deferred while a read is in flight; `finish_load` writes the merged result.
        if self.loading {
            return;
        }
        let json = match serde_json::to_string(&self.keys) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to serialize API keys: {e:#}");
                return;
            }
        };
        // Queued behind any earlier write on the secure-storage worker, so writes land in the
        // order they were made; runs synchronously where no worker exists.
        let _ = secure_storage::background::run_or_inline(
            move |storage| {
                if let Err(e) = storage.write_value(SECURE_STORAGE_KEY, &json) {
                    log::error!("Failed to write API keys to secure storage: {e:#}");
                    // Deliberately no revision bump on a failed write: this process is
                    // still serving the new key from memory, and stamping the revision
                    // would make its own watcher reload the *old* value back over it.
                    return;
                }
                // The keyring is shared with every other Zap process -- GUI and TUI use
                // one app id -- so tell them to re-read it. See `crate::secret_revision`.
                crate::secret_revision::bump_or_log();
            },
            ctx.secure_storage(),
        );
    }
}

impl Entity for ApiKeyManager {
    type Event = ApiKeyManagerEvent;
}

impl SingletonEntity for ApiKeyManager {}

#[cfg(test)]
#[path = "api_keys_tests.rs"]
mod tests;
