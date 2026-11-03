//! Zap's local managed-secrets client.
//!
//! Upstream, Zap originally maintained team/user managed secrets here by
//! calling cloud endpoints via server_api. Zap keeps the
//! `warp_managed_secrets` crate for local functionality reuse, but every
//! cloud managed-secret action is unreachable: queries return an empty set,
//! and write actions and OIDC token issuance return a disabled error.

use std::collections::HashMap;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use warp_managed_secrets::client::{ManagedSecretsClient, TaskIdentityToken};
use warp_managed_secrets::{ManagedSecret, ManagedSecretValue};

pub(crate) struct DisabledManagedSecretsClient;

#[async_trait]
impl ManagedSecretsClient for DisabledManagedSecretsClient {
    async fn list_secrets(&self) -> Result<Vec<ManagedSecret>> {
        Ok(Vec::new())
    }

    async fn get_task_secrets(
        &self,
        _task_id: String,
    ) -> Result<HashMap<String, ManagedSecretValue>> {
        Ok(HashMap::new())
    }

    async fn issue_task_identity_token(
        &self,
        _options: warp_managed_secrets::client::IdentityTokenOptions,
    ) -> Result<TaskIdentityToken> {
        Err(anyhow!("Task identity token issuance disabled in Phosphor"))
    }
}
