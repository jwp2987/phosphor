use comfy_table::Cell;
use serde::Serialize;
use warp_cli::{agent::AgentProfileCommand, GlobalOptions};
use warpui::{AppContext, ModelContext, SingletonEntity};

use crate::ai::agent_sdk::output::{self, TableFormat};
use crate::ai::execution_profiles::profiles::AIExecutionProfilesModel;
use crate::cloud_object::model::generic_string_model::StringModel;
use crate::cloud_object::model::persistence::ObjectStoreModel;
use crate::server::ids::{ClientId, HashableId as _, ServerId, SyncId};

/// The ID `agent profile list` prints for, and `--profile` accepts as, the default
/// profile when it has no sync ID (the case for every CLI run's default profile).
pub(super) const DEFAULT_PROFILE_CLI_ID: &str = "default";

/// The profile a `--profile <ID>` argument selects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProfileSelector {
    /// The default profile, spelled [`DEFAULT_PROFILE_CLI_ID`].
    Default,
    /// A profile identified by its sync ID.
    Sync(SyncId),
}

/// The ID `agent profile list` prints for a profile: its sync ID, or
/// [`DEFAULT_PROFILE_CLI_ID`] for the unsynced default profile.
///
/// Every value this returns is accepted by [`parse_profile_selector`] (#637). The list
/// used to print `Unsynced` for any profile without a legacy 22-character server ID —
/// with no server, every locally created profile — and `--profile` accepted only server
/// IDs, so the command that lists profiles could not name one the flag would take.
pub(super) fn profile_cli_id(sync_id: Option<SyncId>) -> String {
    match sync_id {
        Some(sync_id) => sync_id.to_string(),
        None => DEFAULT_PROFILE_CLI_ID.to_string(),
    }
}

/// Parse a `--profile` argument: `default`, a locally created profile's
/// `Client-<uuid>`, or a legacy 22-character server ID.
pub(super) fn parse_profile_selector(raw: &str) -> Option<ProfileSelector> {
    let raw = raw.trim();
    if raw.eq_ignore_ascii_case(DEFAULT_PROFILE_CLI_ID) {
        return Some(ProfileSelector::Default);
    }
    if let Some(client_id) = ClientId::from_hash(raw) {
        return Some(ProfileSelector::Sync(SyncId::ClientId(client_id)));
    }
    ServerId::try_from(raw)
        .ok()
        .map(|server_id| ProfileSelector::Sync(SyncId::ServerId(server_id)))
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
