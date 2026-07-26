//! `ProjectRulesPersister` — persistence bridge for project rules (WARP.md / AGENTS.md).
//!
//! This thin singleton model has only two responsibilities:
//!
//! 1. Subscribes to [`ProjectContextModel`]'s [`KnownRulesChanged`] event, turning
//!    `discovered_rules` / `deleted_rules` into [`ModelEvent::UpsertProjectRules`] /
//!    [`ModelEvent::DeleteProjectRules`] writes to the SQLite `project_rules` table;
//! 2. Subscribes to [`DetectedRepositories`]'s `DetectedGitRepo` event, triggering
//!    [`ProjectContextModel::index_and_store_rules`] to scan for WARP.md / AGENTS.md
//!    when the user enters a new git repository.
//!
//! These two pieces of logic historically lived inside `PersistedWorkspace::new`,
//! tightly coupled with LSP-enabled persistence and "visited git repo history".
//! Now that LSP + workspace history have been retired, this bridge has to live on
//! independently — otherwise project rules would stop being written to disk / stop
//! auto-scanning on `cd`.

use std::sync::mpsc::SyncSender;

use ai::project_context::model::{ProjectContextModel, ProjectContextModelEvent};
use repo_metadata::repositories::{DetectedRepositories, DetectedRepositoriesEvent};
use warpui::{Entity, ModelContext, SingletonEntity};

use crate::persistence::ModelEvent;

/// See the module-level docs for details.
pub struct ProjectRulesPersister {
    /// The channel used to write to SQLite; `None` means persistence isn't enabled in this build.
    persistence_tx: Option<SyncSender<ModelEvent>>,
}

impl Entity for ProjectRulesPersister {
    type Event = ();
}

impl SingletonEntity for ProjectRulesPersister {}

impl ProjectRulesPersister {
    /// Registers two subscriptions:
    /// - `ProjectContextModel` → turns rule deltas into SQLite ModelEvents;
    /// - `DetectedRepositories` → triggers a rule scan when entering a git repository.
    pub fn new(
        persistence_tx: Option<SyncSender<ModelEvent>>,
        ctx: &mut ModelContext<Self>,
    ) -> Self {
        ctx.subscribe_to_model(&ProjectContextModel::handle(ctx), |me, event, _ctx| {
            let ProjectContextModelEvent::KnownRulesChanged(delta) = event else {
                return;
            };

            let mut events = vec![];

            if !delta.discovered_rules.is_empty() {
                events.push(ModelEvent::UpsertProjectRules {
                    project_rule_paths: delta.discovered_rules.clone(),
                });
            }

            if !delta.deleted_rules.is_empty() {
                events.push(ModelEvent::DeleteProjectRules {
                    path: delta.deleted_rules.clone(),
                });
            }

            if events.is_empty() {
                return;
            }

            let Some(tx) = me.persistence_tx.as_ref() else {
                return;
            };

            for event in events {
                if let Err(err) = tx.send(event) {
                    log::warn!("ProjectRulesPersister: failed to write to SQLite: {err}");
                }
            }
        });

        ctx.subscribe_to_model(&DetectedRepositories::handle(ctx), |_me, event, ctx| {
            let DetectedRepositoriesEvent::DetectedGitRepo { repository, .. } = event;
            let repo_path = repository.as_ref(ctx).root_dir().to_local_path_lossy();

            ProjectContextModel::handle(ctx).update(ctx, |model, ctx| {
                let _ = model.index_and_store_rules(repo_path, ctx);
            });
        });

        Self { persistence_tx }
    }

    /// Test-only: doesn't bind a persistence channel, and doesn't subscribe to any model.
    #[cfg(test)]
    pub fn new_for_test(_ctx: &mut ModelContext<Self>) -> Self {
        Self {
            persistence_tx: None,
        }
    }
}
