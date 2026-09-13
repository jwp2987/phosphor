//! The host registry (`docs/design/moth-parliament.md`, "Requirement 5 needs a
//! surface, and a registry that does not exist"): a first-class record of
//! every remote host the app has been told about or has observed, and the
//! named groups a host can belong to.
//!
//! **Model only.** No settings page, no panel -- the design doc's own build
//! order puts explicit install/upgrade/remove UI next, and the surface itself
//! after that. This file is step one: the thing both later pieces read from
//! and write to.
//!
//! # What this reuses, and why it does not go the other way
//!
//! Install state distinguishes not-installed / installed-at-version-X /
//! unsupported-with-a-reason / unknown. The "unsupported" and "unknown" halves
//! of that reuse [`remote_server::setup::UnsupportedReason`] and mirror
//! [`remote_server::setup::PreinstallStatus`] directly, rather than inventing
//! a second vocabulary for the same question. [`HostInstallState`] adds only
//! what `PreinstallStatus` has no room for: `PreinstallStatus` describes what
//! a *preinstall* check found, before anything is installed, so it cannot
//! represent "installed, at this version" -- that fact only exists after a
//! successful handshake (`InitializeResponse::{server_version, host_id}`).
//!
//! # Groups vs. hosts
//!
//! A host is a session target -- something `ssh` can open a shell on. A group
//! is a query target -- a name for "these hosts, considered together" -- and
//! is never a session target (`docs/design/moth-parliament.md`, "Host
//! groups"). [`HostGroup`] is deliberately a separate type from
//! [`RemoteHostEntry`] with no target string, no `HostId`, and no install
//! state of its own, so nothing about its shape suggests it is something you
//! can open a shell on. Group *queries* and fan-out are explicitly later
//! work; this file models membership only.
//!
//! # Persistence lives in settings, not SQLite
//!
//! **Maintainer decision.** Both the registry and its groups are structured
//! `Vec<T>` settings (`PersistedRemoteHost` / `PersistedHostGroup` in
//! `terminal::warpify::settings`), following the precedent already in this
//! codebase for a structured list living in settings rather than a database
//! table: `Vec<CustomSecretRegex>` (`app/src/settings/privacy.rs`) and
//! `Vec<HostFooterColorRule>` (`app/src/workspace/tab_settings.rs`). No
//! migration exists for this and none should be added.
//!
//! # Convergence with `warpify.ssh.remote_hosts`
//!
//! `warpify.ssh.remote_hosts` is the user's declared list of session targets
//! (`docs/design/moth-parliament.md` §4a); this registry is the observed
//! state of those same targets, in a second settings list
//! (`remote_host_registry_entries`). Now that both live in settings, having
//! two independent lists of hosts would be exactly the "settings list and a
//! status panel that disagree" the design doc warns against, so this file
//! keeps them from being independent: every target declared in
//! `remote_hosts` is guaranteed a `remote_host_registry_entries` entry (a stub
//! `Unknown`-state one, if nothing has observed that host yet), enforced on
//! construction and on every settings change, in either direction.
//!
//! **Why `remote_hosts` was not folded away entirely.** The stronger
//! consolidation -- deleting `remote_hosts` and deriving the new-session
//! menu's target list from the registry instead -- would be the cleaner end
//! state, but it means changing `workspace/view.rs`'s new-session menu and
//! `workspace/action.rs`'s `AddRemoteHostTab`, both shipped, tested UI
//! behavior outside "the model, no UI" this task is scoped to, and neither
//! can be verified here (no compile, no test run against the UI). So
//! `remote_hosts` keeps its existing role as the session-target list those
//! two files already read, and this file makes it converge one-directionally
//! into the registry instead of duplicating it silently. **Anyone who has
//! already hand-edited `warpify.ssh.remote_hosts` needs no migration**: the
//! first read after this change converges their existing targets into
//! `remote_host_registry_entries` automatically, all as `Unknown` until
//! re-probed.
//!
//! This is deliberately one-directional in the other sense too: removing a
//! target from `remote_hosts` does **not** delete its registry entry. A
//! registry entry is the only record that a host was ever installed onto,
//! and silently discarding that because a user edited a text field would
//! throw away exactly the fact this registry exists to keep.
//!
//! # Observed state is advisory, not authoritative
//!
//! Settings are user-editable and can be hand-edited or arrive synced from
//! another machine. `install_state`, `last_reached_at`, `os` and `arch` are
//! therefore treated as a **cache of the last observation**, not verified
//! fact: nothing in this file re-validates them against the actual remote
//! host, and no code here should ever skip a real probe because the cache
//! already claims an answer. `target` is the only field that is a plain
//! declaration rather than an observation. See `PersistedRemoteHost`'s doc
//! comment in `terminal::warpify::settings` for the same point made at the
//! wire-format level.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use remote_server::setup::{GlibcVersion, RemoteArch, RemoteOs, UnsupportedReason};
use warp_core::HostId;
use warpui::{Entity, ModelContext, SingletonEntity};

// `Setting::value`/`set_value` are trait methods; the trait must be in scope to call
// them. Imported anonymously, matching `appearance.rs` and `wasm_nux_dialog.rs`.
use settings::Setting as _;

use crate::terminal::warpify::settings::{
    PersistedHostGroup, PersistedRemoteHost, WarpifySettings, WarpifySettingsChangedEvent,
};

/// Install state of the remote-server binary on a host, as tracked by the
/// registry. See the module docs for what is reused from
/// `remote_server::setup` and why.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostInstallState {
    /// No successful install has ever been observed on this host.
    NotInstalled,
    /// Installed, as of the last successful handshake
    /// (`InitializeResponse::server_version`).
    Installed { version: String },
    /// A preinstall check or install attempt classified this host as unable
    /// to run the prebuilt remote-server binary.
    Unsupported { reason: UnsupportedReason },
    /// Nothing has been probed yet.
    Unknown,
}

impl Default for HostInstallState {
    fn default() -> Self {
        Self::Unknown
    }
}

impl HostInstallState {
    fn column_value(&self) -> &'static str {
        match self {
            Self::NotInstalled => "not_installed",
            Self::Installed { .. } => "installed",
            Self::Unsupported { .. } => "unsupported",
            Self::Unknown => "unknown",
        }
    }
}

fn parse_remote_os(value: &str) -> Option<RemoteOs> {
    match value {
        "linux" => Some(RemoteOs::Linux),
        "macos" => Some(RemoteOs::MacOs),
        _ => None,
    }
}

fn parse_remote_arch(value: &str) -> Option<RemoteArch> {
    match value {
        "x86_64" => Some(RemoteArch::X86_64),
        "aarch64" => Some(RemoteArch::Aarch64),
        _ => None,
    }
}

/// One host the registry knows about: the target as configured, its resolved
/// identity (once known), install state, when it was last reached, and its
/// probed platform.
///
/// A session target, in the sense `docs/design/moth-parliament.md`'s "Host
/// groups" section means it: something `ssh <target>` can open a shell on.
/// Contrast [`HostGroup`], which carries no target string at all.
#[derive(Clone, Debug, PartialEq)]
pub struct RemoteHostEntry {
    /// The target string as configured -- e.g. what the user typed into
    /// `warpify.ssh.remote_hosts` or the new-session menu ("build-box",
    /// "user@1.2.3.4"). Resolving that string to a real address is the
    /// system's job (`~/.ssh/config`, `ssh-agent`), not this registry's --
    /// see `DECLINED.md`, "SSH connection management -- the system owns it,
    /// not the app".
    pub target: String,
    /// The identity the remote daemon reported on its last successful
    /// handshake. `None` until the first successful `Initialize`. Advisory,
    /// like every field below -- see the module docs.
    pub host_id: Option<HostId>,
    pub install_state: HostInstallState,
    /// When this host last answered a probe or connected successfully.
    pub last_reached_at: Option<DateTime<Utc>>,
    pub os: Option<RemoteOs>,
    pub arch: Option<RemoteArch>,
}

impl RemoteHostEntry {
    pub fn new(target: impl Into<String>) -> Self {
        Self {
            target: target.into(),
            host_id: None,
            install_state: HostInstallState::Unknown,
            last_reached_at: None,
            os: None,
            arch: None,
        }
    }

    /// Converts to the settings-storage shape. Column-level, not
    /// domain-level -- see [`PersistedRemoteHost`]'s own doc comment.
    fn to_persisted(&self) -> PersistedRemoteHost {
        let mut installed_version = None;
        let mut unsupported_reason_kind = None;
        let mut unsupported_glibc_detected_major = None;
        let mut unsupported_glibc_detected_minor = None;
        let mut unsupported_glibc_required_major = None;
        let mut unsupported_glibc_required_minor = None;
        let mut unsupported_non_glibc_name = None;

        match &self.install_state {
            HostInstallState::Installed { version } => {
                installed_version = Some(version.clone());
            }
            HostInstallState::Unsupported { reason } => match reason {
                UnsupportedReason::GlibcTooOld { detected, required } => {
                    unsupported_reason_kind = Some("glibc_too_old".to_string());
                    unsupported_glibc_detected_major = Some(detected.major);
                    unsupported_glibc_detected_minor = Some(detected.minor);
                    unsupported_glibc_required_major = Some(required.major);
                    unsupported_glibc_required_minor = Some(required.minor);
                }
                UnsupportedReason::NonGlibc { name } => {
                    unsupported_reason_kind = Some("non_glibc".to_string());
                    unsupported_non_glibc_name = Some(name.clone());
                }
            },
            HostInstallState::NotInstalled | HostInstallState::Unknown => {}
        }

        PersistedRemoteHost {
            target: self.target.clone(),
            host_id: self.host_id.as_ref().map(|id| id.as_str().to_string()),
            install_state: self.install_state.column_value().to_string(),
            installed_version,
            unsupported_reason_kind,
            unsupported_glibc_detected_major,
            unsupported_glibc_detected_minor,
            unsupported_glibc_required_major,
            unsupported_glibc_required_minor,
            unsupported_non_glibc_name,
            last_reached_at_unix_millis: self.last_reached_at.map(|ts| ts.timestamp_millis()),
            os: self.os.as_ref().map(|os| os.as_str().to_string()),
            arch: self.arch.as_ref().map(|arch| arch.as_str().to_string()),
        }
    }

    /// Reconstructs an entry from a persisted settings value. An
    /// `install_state` this build does not recognize (a downgrade reading a
    /// future value, or the "unsupported" case losing its reason fields)
    /// degrades to `Unknown` rather than failing the whole read --
    /// consistent with `PreinstallCheckResult::parse`'s own fail-open stance
    /// on data it cannot classify. This is also the boundary that treats
    /// settings as advisory: nothing here re-verifies the value against the
    /// real host, it is read as a cached last observation.
    fn from_persisted(persisted: PersistedRemoteHost) -> Self {
        let install_state = match persisted.install_state.as_str() {
            "not_installed" => HostInstallState::NotInstalled,
            "installed" => HostInstallState::Installed {
                version: persisted.installed_version.unwrap_or_default(),
            },
            "unsupported" => match persisted.unsupported_reason_kind.as_deref() {
                Some("glibc_too_old") => HostInstallState::Unsupported {
                    reason: UnsupportedReason::GlibcTooOld {
                        detected: GlibcVersion::new(
                            persisted.unsupported_glibc_detected_major.unwrap_or(0),
                            persisted.unsupported_glibc_detected_minor.unwrap_or(0),
                        ),
                        required: GlibcVersion::new(
                            persisted.unsupported_glibc_required_major.unwrap_or(0),
                            persisted.unsupported_glibc_required_minor.unwrap_or(0),
                        ),
                    },
                },
                Some("non_glibc") => HostInstallState::Unsupported {
                    reason: UnsupportedReason::NonGlibc {
                        name: persisted.unsupported_non_glibc_name.unwrap_or_default(),
                    },
                },
                _ => HostInstallState::Unknown,
            },
            _ => HostInstallState::Unknown,
        };

        Self {
            target: persisted.target,
            host_id: persisted.host_id.map(HostId::new),
            install_state,
            last_reached_at: persisted
                .last_reached_at_unix_millis
                .and_then(DateTime::<Utc>::from_timestamp_millis),
            os: persisted.os.as_deref().and_then(parse_remote_os),
            arch: persisted.arch.as_deref().and_then(parse_remote_arch),
        }
    }
}

/// A named set of hosts (`docs/design/moth-parliament.md`, "Host groups: a
/// service is rarely one machine"). A query target, never a session target --
/// see the module docs. Carries membership only; group queries and fan-out
/// are later work.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct HostGroup {
    pub name: String,
    /// Target strings, referencing [`RemoteHostEntry::target`]. Not
    /// validated against the registry at insertion time -- a host can be
    /// added to a group before it has ever been observed.
    pub members: Vec<String>,
}

impl HostGroup {
    fn to_persisted(&self) -> PersistedHostGroup {
        PersistedHostGroup {
            name: self.name.clone(),
            members: self.members.clone(),
        }
    }

    fn from_persisted(persisted: PersistedHostGroup) -> Self {
        Self {
            name: persisted.name,
            members: persisted.members,
        }
    }
}

/// Emitted when a host's state changes, or a group's membership does. No
/// payload beyond the identity that changed -- readers re-fetch through
/// [`HostRegistryModel::host`] / [`HostRegistryModel::group`].
#[derive(Clone, Debug)]
pub enum HostRegistryEvent {
    HostChanged { target: String },
    GroupChanged { name: String },
}

/// The host registry singleton. See the module docs for the model this
/// implements, what it reuses, where it persists, and how it converges with
/// `warpify.ssh.remote_hosts`.
#[derive(Default)]
pub struct HostRegistryModel {
    hosts: HashMap<String, RemoteHostEntry>,
    groups: HashMap<String, HostGroup>,
}

impl Entity for HostRegistryModel {
    type Event = HostRegistryEvent;
}

impl SingletonEntity for HostRegistryModel {}

impl HostRegistryModel {
    /// Builds the registry from `WarpifySettings`, converges it with the
    /// currently-declared `remote_hosts` targets, and subscribes to that
    /// settings group so later changes converge too.
    ///
    /// The subscription deliberately treats "the declared list changed" and
    /// "the registry itself changed" differently -- see
    /// [`Self::converge_and_persist`]'s doc comment for why conflating them
    /// would make this model recurse into its own write.
    pub fn new(ctx: &mut ModelContext<Self>) -> Self {
        let mut model = Self::default();
        model.refresh_from_settings(ctx);
        model.converge_and_persist(ctx);

        let warpify_settings = WarpifySettings::handle(ctx);
        ctx.subscribe_to_model(&warpify_settings, |me, event, ctx| {
            if matches!(
                event,
                WarpifySettingsChangedEvent::RemoteHostRegistryEntries { .. }
                    | WarpifySettingsChangedEvent::RemoteHostGroups { .. }
            ) {
                me.refresh_from_settings(ctx);
            } else if matches!(event, WarpifySettingsChangedEvent::RemoteHosts { .. }) {
                me.refresh_from_settings(ctx);
                me.converge_and_persist(ctx);
            }
        });

        model
    }

    pub fn hosts(&self) -> impl Iterator<Item = &RemoteHostEntry> {
        self.hosts.values()
    }

    pub fn host(&self, target: &str) -> Option<&RemoteHostEntry> {
        self.hosts.get(target)
    }

    pub fn groups(&self) -> impl Iterator<Item = &HostGroup> {
        self.groups.values()
    }

    pub fn group(&self, name: &str) -> Option<&HostGroup> {
        self.groups.get(name)
    }

    /// Records that `target` answered a probe or connected successfully,
    /// optionally learning its resolved identity and platform for the first
    /// time. Creates the entry if this is the first time `target` has been
    /// seen at all.
    pub fn record_reached(
        &mut self,
        target: &str,
        host_id: Option<HostId>,
        platform: Option<(RemoteOs, RemoteArch)>,
        ctx: &mut ModelContext<Self>,
    ) {
        let entry = self
            .hosts
            .entry(target.to_string())
            .or_insert_with(|| RemoteHostEntry::new(target));
        entry.last_reached_at = Some(Utc::now());
        if let Some(host_id) = host_id {
            entry.host_id = Some(host_id);
        }
        if let Some((os, arch)) = platform {
            entry.os = Some(os);
            entry.arch = Some(arch);
        }
        self.persist_hosts(ctx);
        ctx.emit(HostRegistryEvent::HostChanged {
            target: target.to_string(),
        });
    }

    /// Records the observed install state for `target`, creating the entry
    /// if this is the first time `target` has been seen at all.
    pub fn record_install_state(
        &mut self,
        target: &str,
        install_state: HostInstallState,
        ctx: &mut ModelContext<Self>,
    ) {
        let entry = self
            .hosts
            .entry(target.to_string())
            .or_insert_with(|| RemoteHostEntry::new(target));
        entry.install_state = install_state;
        self.persist_hosts(ctx);
        ctx.emit(HostRegistryEvent::HostChanged {
            target: target.to_string(),
        });
    }

    /// Adds `target` to `group_name`, creating the group if it does not
    /// already exist. A host may belong to any number of groups.
    pub fn add_host_to_group(
        &mut self,
        group_name: &str,
        target: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        let group = self
            .groups
            .entry(group_name.to_string())
            .or_insert_with(|| HostGroup {
                name: group_name.to_string(),
                members: Vec::new(),
            });
        if !group.members.iter().any(|member| member == target) {
            group.members.push(target.to_string());
        }
        self.persist_groups(ctx);
        ctx.emit(HostRegistryEvent::GroupChanged {
            name: group_name.to_string(),
        });
    }

    /// Removes `target` from `group_name`. The group itself is not deleted
    /// even if this was its last member -- an empty group is valid.
    pub fn remove_host_from_group(
        &mut self,
        group_name: &str,
        target: &str,
        ctx: &mut ModelContext<Self>,
    ) {
        if let Some(group) = self.groups.get_mut(group_name) {
            group.members.retain(|member| member != target);
        }
        self.persist_groups(ctx);
        ctx.emit(HostRegistryEvent::GroupChanged {
            name: group_name.to_string(),
        });
    }

    /// Rebuilds in-memory state from `WarpifySettings`. Read-only -- never
    /// writes back -- so it is always safe to call from a subscription
    /// reacting to a settings change, including one this model's own
    /// `persist_hosts`/`persist_groups` just caused.
    fn refresh_from_settings(&mut self, ctx: &mut ModelContext<Self>) {
        let settings = WarpifySettings::as_ref(ctx);

        self.hosts = settings
            .remote_host_registry_entries
            .value()
            .iter()
            .cloned()
            .map(|persisted| {
                (
                    persisted.target.clone(),
                    RemoteHostEntry::from_persisted(persisted),
                )
            })
            .collect();

        self.groups = settings
            .remote_host_groups
            .value()
            .iter()
            .cloned()
            .map(|persisted| (persisted.name.clone(), HostGroup::from_persisted(persisted)))
            .collect();
    }

    /// Converges any newly-declared `remote_hosts` targets into the
    /// (already-refreshed) in-memory registry, persisting only if that added
    /// something.
    ///
    /// Deliberately **not** called in response to a
    /// `RemoteHostRegistryEntries` change -- that event fires *because* this
    /// function just wrote to it (or because of an external edit), and
    /// converging again there would mean this function's own write
    /// synchronously re-triggers itself through the settings-change
    /// subscription. Convergence only needs to run when the *declared* list
    /// (`remote_hosts`) changes, or once at construction; the registry write
    /// this produces is a one-way trigger, never a loop, because the write's
    /// own change event is handled by [`Self::refresh_from_settings`] alone
    /// (see [`Self::new`]).
    fn converge_and_persist(&mut self, ctx: &mut ModelContext<Self>) {
        let declared = WarpifySettings::as_ref(ctx).remote_hosts.value().clone();
        let newly_added = converge_declared_targets(&mut self.hosts, &declared);
        if !newly_added.is_empty() {
            self.persist_hosts(ctx);
        }
    }

    fn persist_hosts(&self, ctx: &mut ModelContext<Self>) {
        let entries: Vec<PersistedRemoteHost> = self
            .hosts
            .values()
            .map(RemoteHostEntry::to_persisted)
            .collect();
        WarpifySettings::handle(ctx).update(ctx, |settings, ctx| {
            if let Err(err) = settings
                .remote_host_registry_entries
                .set_value(entries, ctx)
            {
                log::error!("Failed to persist remote host registry: {err}");
            }
        });
    }

    fn persist_groups(&self, ctx: &mut ModelContext<Self>) {
        let groups: Vec<PersistedHostGroup> =
            self.groups.values().map(HostGroup::to_persisted).collect();
        WarpifySettings::handle(ctx).update(ctx, |settings, ctx| {
            if let Err(err) = settings.remote_host_groups.set_value(groups, ctx) {
                log::error!("Failed to persist remote host groups: {err}");
            }
        });
    }
}

/// Inserts a stub `Unknown`-state [`RemoteHostEntry`] into `hosts` for every
/// target in `declared` not already present, returning the newly-added
/// targets. Pure and free of `ModelContext`/settings so the convergence rule
/// itself -- every declared target ends up in the registry -- is directly
/// testable.
fn converge_declared_targets(
    hosts: &mut HashMap<String, RemoteHostEntry>,
    declared: &[String],
) -> Vec<String> {
    let mut newly_added = Vec::new();
    for target in declared {
        if !hosts.contains_key(target) {
            hosts.insert(target.clone(), RemoteHostEntry::new(target.clone()));
            newly_added.push(target.clone());
        }
    }
    newly_added
}

#[cfg(test)]
#[path = "host_registry_tests.rs"]
mod tests;
