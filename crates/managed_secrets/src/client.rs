use std::collections::HashMap;
use std::time::Duration;

use async_trait::async_trait;

use anyhow::Result;
use chrono::{DateTime, Utc};
use vec1::Vec1;

use crate::{ManagedSecretType, ManagedSecretValue};

#[derive(Debug, Clone)]
pub enum ManagedSecretOwner {
    User { uid: String },
    Team { uid: String },
}

#[derive(Debug, Clone)]
pub struct ManagedSecret {
    pub name: String,
    pub description: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub owner: ManagedSecretOwner,
    pub type_: ManagedSecretType,
}

/// An OIDC identity token issued for a task workload.
#[derive(Debug, Clone)]
pub struct TaskIdentityToken {
    /// The signed OIDC JWT.
    pub token: String,
    /// When the token expires.
    pub expires_at: DateTime<Utc>,
    /// The OIDC issuer that signed the token.
    pub issuer: String,
}

/// Options for issuing an OIDC identity token.
pub struct IdentityTokenOptions {
    /// The intended audience for the token (e.g. a cloud provider URL).
    pub audience: String,
    /// The requested token lifetime. The server may cap this to a maximum value.
    pub requested_duration: Duration,
    /// Controls how the `sub` claim is formatted. Each element names a claim to
    /// include.
    pub subject_template: Vec1<String>,
}

#[cfg_attr(not(target_family = "wasm"), async_trait)]
#[cfg_attr(target_family = "wasm", async_trait(?Send))]
pub trait ManagedSecretsClient: 'static + Send + Sync {
    async fn list_secrets(&self) -> Result<Vec<ManagedSecret>>;

    async fn get_task_secrets(
        &self,
        task_id: String,
    ) -> Result<HashMap<String, ManagedSecretValue>>;

    /// Issue a short-lived OIDC identity token for the current task.
    ///
    /// The workload token is provided by the local managed-secrets client.
    async fn issue_task_identity_token(
        &self,
        options: IdentityTokenOptions,
    ) -> Result<TaskIdentityToken>;
}
