//! Common utilities for agent SDK commands.

use std::future::Future;

use futures::TryFutureExt;

use warpui::r#async::FutureExt;
use warpui::{AppContext, SingletonEntity as _};

use crate::ai::agent_sdk::driver::WARP_DRIVE_SYNC_TIMEOUT;

use crate::ai::llms::{LLMId, LLMPreferences};
use crate::cloud_object::model::persistence::ObjectStoreModel;

pub fn validate_agent_mode_base_model_id(
    model_id: &str,
    ctx: &AppContext,
) -> anyhow::Result<LLMId> {
    let llm_prefs = LLMPreferences::as_ref(ctx);

    let valid_ids = llm_prefs
        .get_base_llm_choices_for_agent_mode()
        .map(|info| info.id.clone())
        .collect::<Vec<_>>();

    classify_agent_mode_base_model_id(
        model_id,
        &valid_ids,
        llm_prefs.agent_mode_models_unavailable(),
    )
}

/// Classifies a user-supplied agent-mode model id against the available model
/// list, distinguishing "the model list could not be built, so it is stale or
/// empty" from "the id is genuinely not in a valid list".
///
/// Split out from [`validate_agent_mode_base_model_id`] so the classification is
/// testable without an `AppContext`: the interesting behaviour is the branch, not
/// the lookup.
fn classify_agent_mode_base_model_id(
    model_id: &str,
    valid_ids: &[LLMId],
    list_unavailable: bool,
) -> anyhow::Result<LLMId> {
    let llm_id: LLMId = model_id.into();
    if valid_ids.contains(&llm_id) {
        // An id that is present validates regardless of list health — a locally
        // configured provider's model does not stop existing because some other
        // refresh failed.
        Ok(llm_id)
    } else if list_unavailable {
        Err(anyhow::anyhow!(
            "Could not retrieve the agent-mode model list \
             (the last update failed, so the list is stale or empty). Try again later."
        ))
    } else {
        let suggestions = valid_ids
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Err(anyhow::anyhow!(
            "Unknown model id '{model_id}'. Try one of: {suggestions}"
        ))
    }
}

/// Refresh workspace metadata before executing an operation.
///
/// This keeps workspace state fresh before operations that depend on team membership.
pub fn refresh_workspace_metadata<C>(
    _ctx: &mut C,
) -> impl Future<Output = anyhow::Result<()>> + Send + 'static + use<C> {
    async { Ok(()) }
}

/// Refresh Zap Drive before executing an operation.
pub fn refresh_warp_drive(
    ctx: &AppContext,
) -> impl Future<Output = anyhow::Result<()>> + Send + 'static + use<> {
    ObjectStoreModel::as_ref(ctx)
        .initial_load_complete()
        .with_timeout(WARP_DRIVE_SYNC_TIMEOUT)
        .map_err(|_| anyhow::anyhow!("Timed out waiting for Library to load"))
}

#[cfg(test)]
#[path = "common_tests.rs"]
mod tests;
