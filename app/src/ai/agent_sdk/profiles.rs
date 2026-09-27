use comfy_table::Cell;
use serde::Serialize;
use warp_cli::{agent::AgentProfileCommand, GlobalOptions};
use warpui::{AppContext, ModelContext, SingletonEntity};

use crate::ai::agent_sdk::output::{self, TableFormat};
use crate::ai::execution_profiles::profiles::{AIExecutionProfilesModel, ClientProfileId};
use crate::cloud_object::model::generic_string_model::StringModel;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::server::ids::SyncId;

/// The ID `agent profile list` prints for, and `--profile` accepts as, the default
/// profile when it has no sync ID (the case for every CLI run's default profile).
/// `--profile default` selects the default profile whatever it is listed as.
pub(super) const DEFAULT_PROFILE_CLI_ID: &str = "default";

/// The ID `agent profile list` prints for a profile: its sync ID (`Client-<uuid>` for a
/// locally created profile, a legacy 22-character server ID otherwise), or
/// [`DEFAULT_PROFILE_CLI_ID`] for the unsynced default profile.
///
/// `--profile` resolves its argument by matching it against exactly these strings
/// ([`find_profile_by_cli_id`]), so everything the list prints is selectable (#637). The
/// list used to print `Unsynced` for any profile without a server ID — with no server,
/// every locally created one — while `--profile` accepted only server IDs.
pub(super) fn profile_cli_id(sync_id: Option<SyncId>) -> String {
    match sync_id {
        Some(sync_id) => sync_id.to_string(),
        None => DEFAULT_PROFILE_CLI_ID.to_string(),
    }
}

/// Whether a `--profile` argument names the profile listed as `listed_id`.
fn cli_id_matches(listed_id: &str, raw: &str) -> bool {
    let raw = raw.trim();
    !raw.is_empty() && listed_id == raw
}

/// Resolve a `--profile` argument to the profile `agent profile list` printed it for.
pub(super) fn find_profile_by_cli_id(
    model: &AIExecutionProfilesModel,
    raw: &str,
    ctx: &AppContext,
) -> Option<ClientProfileId> {
    if raw.trim().eq_ignore_ascii_case(DEFAULT_PROFILE_CLI_ID) {
        return Some(model.default_profile_id());
    }
    model.get_all_profile_ids().into_iter().find(|id| {
        model
            .get_profile_by_id(*id, ctx)
            .is_some_and(|profile| cli_id_matches(&profile_cli_id(profile.sync_id()), raw))
    })
}

/// Handle Agent Profile-related CLI commands.
pub fn run(
    ctx: &mut AppContext,
    global_options: GlobalOptions,
    command: AgentProfileCommand,
) -> anyhow::Result<()> {
    let runner = ctx.add_singleton_model(|_ctx| ProfilesCommandRunner);
    match command {
        AgentProfileCommand::List => {
            runner.update(ctx, |runner, ctx| runner.list(global_options, ctx));
            Ok(())
        }
    }
}

/// Singleton model that runs async work for profile CLI commands.
struct ProfilesCommandRunner;

impl ProfilesCommandRunner {
    fn list(&self, global_options: GlobalOptions, ctx: &mut ModelContext<Self>) {
        // Ensure locally persisted profiles have completed their initial load.
        let initial_sync = ObjectStoreModel::as_ref(ctx).initial_load_complete();

        ctx.spawn(initial_sync, move |_, _, ctx| {
            let profiles_model = AIExecutionProfilesModel::as_ref(ctx);

            let profile_ids = profiles_model.get_all_profile_ids();

            let profiles: Vec<_> = profile_ids
                .iter()
                .flat_map(|id| profiles_model.get_profile_by_id(*id, ctx))
                .map(|profile| {
                    let name = profile.data().display_name().to_string();
                    let id = profile_cli_id(profile.sync_id());
                    ProfileInfo { id, name }
                })
                .collect();

            output::print_list(profiles, global_options.output_format);

            ctx.terminate_app(warpui::platform::TerminationMode::ForceTerminate, None);
        });
    }
}

impl warpui::Entity for ProfilesCommandRunner {
    type Event = ();
}
impl SingletonEntity for ProfilesCommandRunner {}

/// Profile information that's shown in the `list` command.
#[derive(Serialize)]
struct ProfileInfo {
    id: String,
    name: String,
}

impl TableFormat for ProfileInfo {
    fn header() -> Vec<Cell> {
        vec![Cell::new("ID"), Cell::new("Name")]
    }

    fn row(&self) -> Vec<Cell> {
        vec![Cell::new(&self.id), Cell::new(&self.name)]
    }
}

#[cfg(test)]
#[path = "profiles_tests.rs"]
mod tests;
