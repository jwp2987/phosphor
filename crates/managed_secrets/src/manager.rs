use std::{collections::HashMap, future::Future, sync::Arc};

use warpui::{Entity, SingletonEntity};

use crate::{
    ManagedSecret, ManagedSecretValue,
    client::{IdentityTokenOptions, ManagedSecretsClient, TaskIdentityToken},
};

/// Singleton model for working with Zap-managed secrets.
pub struct ManagedSecretManager {
    client: Arc<dyn ManagedSecretsClient>,
}

impl ManagedSecretManager {
    pub fn new(client: Arc<dyn ManagedSecretsClient>) -> Self {
        Self { client }
    }

    /// List all managed secrets accessible to the current user.
    pub fn list_secrets(&self) -> impl Future<Output = anyhow::Result<Vec<ManagedSecret>>> + use<> {
        let client = self.client.clone();
        async move {
            let secrets = client.list_secrets().await?;
            Ok(secrets)
        }
    }

    /// Get Zap-managed secrets scoped to the currently-executing task.
    ///
    /// This will fail if not in an ambient agent.
    pub fn get_task_secrets(
        &self,
        task_id: String,
    ) -> impl Future<Output = anyhow::Result<HashMap<String, ManagedSecretValue>>> + use<> {
        let client = self.client.clone();
        async move {
            let secrets = client.get_task_secrets(task_id).await?;
            Ok(secrets)
        }
    }

    /// Issue a short-lived OIDC identity token for the current task.
    pub fn issue_task_identity_token(
        &self,
        options: IdentityTokenOptions,
    ) -> impl Future<Output = anyhow::Result<TaskIdentityToken>> + use<> {
        let client = self.client.clone();
        async move { client.issue_task_identity_token(options).await }
    }
}

impl Entity for ManagedSecretManager {
    type Event = ();
}

impl SingletonEntity for ManagedSecretManager {}
