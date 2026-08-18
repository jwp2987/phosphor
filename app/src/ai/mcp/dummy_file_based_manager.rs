use crate::ai::mcp::templatable_installation::TemplatableMCPServerInstallation;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use super::MCPProvider;
use warpui::{AppContext, Entity, ModelContext, SingletonEntity};

pub struct FileBasedMCPManager {}

impl FileBasedMCPManager {
    pub fn new(_ctx: &mut ModelContext<Self>) -> Self {
        Self {}
    }

    pub fn get_servers_for_working_directory(
        &self,
        _cwd: &Path,
        _app: &AppContext,
    ) -> Vec<&TemplatableMCPServerInstallation> {
        vec![]
    }

    pub fn file_based_servers(&self) -> Vec<&TemplatableMCPServerInstallation> {
        vec![]
    }

    pub fn get_installation_by_uuid(
        &self,
        _uuid: Uuid,
    ) -> Option<&TemplatableMCPServerInstallation> {
        None
    }

    pub fn directory_paths_for_installation_and_provider(
        &self,
        _uuid: Uuid,
        _provider: MCPProvider,
    ) -> Vec<PathBuf> {
        vec![]
    }

    #[cfg(any(feature = "tui", test))]
    pub fn config_diagnostics(&self) -> Vec<FileMCPConfigDiagnostic> {
        vec![]
    }

    #[cfg(any(feature = "tui", test))]
    pub fn file_based_servers_with_sources(&self) -> Vec<FileBasedMCPServerWithSources> {
        vec![]
    }

    #[cfg(any(feature = "tui", test))]
    pub fn installation_by_hash(&self, _hash: u64) -> Option<&TemplatableMCPServerInstallation> {
        None
    }

    /// Mirrors `file_based_manager::FileBasedMCPManager::activate_global_warp_servers`
    /// so the TUI entry point compiles without `local_fs`. Without the filesystem
    /// layer there are no file-based servers to defer or release.
    #[cfg(any(feature = "tui", test))]
    pub fn activate_global_warp_servers(&mut self, _ctx: &mut ModelContext<Self>) {}
}

/// The fields of a config diagnostic the TUI catalog reads. Mirrors
/// `file_mcp_watcher::FileMCPConfigDiagnostic` (minus its `kind`, which no
/// frontend consumes) so callers compile with or without `local_fs`.
#[cfg(any(feature = "tui", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMCPConfigDiagnostic {
    pub config_path: PathBuf,
    pub provider: MCPProvider,
    pub message: String,
}

/// Whether a file-based config source is a global one or a project one.
/// Mirrors `file_based_manager::FileBasedMCPServerScope` so callers compile
/// with or without `local_fs`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileBasedMCPServerScope {
    Global,
    Project,
}

#[cfg(any(feature = "tui", test))]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileBasedMCPServerSource {
    pub provider: MCPProvider,
    pub root_path: PathBuf,
    pub scope: FileBasedMCPServerScope,
}

#[cfg(any(feature = "tui", test))]
#[derive(Clone, Debug)]
pub struct FileBasedMCPServerWithSources {
    pub installation: TemplatableMCPServerInstallation,
    pub sources: Vec<FileBasedMCPServerSource>,
}

impl Entity for FileBasedMCPManager {
    type Event = ();
}

impl SingletonEntity for FileBasedMCPManager {}
