//! [`RemoteFeatures`]: the one interface, generated once for the trait and its three
//! implementations (helper, shell, routed) so a method added here appears everywhere.
//!
//! Each line is `Family, <mutates>: fn name(args) -> Response;`. The argument and response types
//! are the existing proto types, and the names and argument order are those of the matching
//! [`RemoteServerClient`](crate::client::RemoteServerClient) method, which is what lets the
//! helper arm be a straight pass-through. `mutates` is true for calls that change the host: the
//! router retries those on the shell only when the helper answered `UNSUPPORTED` (the request
//! did not run), never after a lost connection (it may have).

use futures::future::BoxFuture;

use super::helper::{HelperBackend, client_id};
use super::refresh::RemoteRefreshEvent;
use super::router::{HelperProbe, RoutedBackend};
use super::shell::ShellBackend;
use super::{BackendError, Family};
use crate::proto::{
    CreateDirectoryResponse, DiscardFilesRequest, GetBranches, GetBranchesResponse,
    GetCommittedBranchFilesRequest, GetCommittedBranchFilesResponse, GetDiffState,
    GetDiffStateResponse, GitCommitChainRequest, GitCommitChainResponse, GitCreatePrRequest,
    GitCreatePrResponse, GitPullRequest, GitPullResponse, GitPushRequest, GitPushResponse,
    GitStageRequest, ListDirectoryResponse, LoadRepoMetadataDirectoryResponse,
    NavigatedToDirectoryResponse, OpenBufferResponse, ReadFileContextRequest,
    ReadFileContextResponse, ResolveConflictResponse, ResolvePathResponse, RipgrepSearchRequest,
    RipgrepSearchResponse, SaveBufferResponse,
};

/// The shell arm of one method: `unimplemented` answers `Unsupported` (or `Unavailable` without a
/// channel) until the family's package lands; any other name is the `ShellBackend` method that
/// serves it (in `shell/<family>.rs`).
macro_rules! shell_call {
    (unimplemented, $self:ident, $fam:ident, $name:ident, ($($arg:ident),+)) => {{
        let _ = ($(&$arg,)+);
        let error = $self.not_implemented(Family::$fam, stringify!($name));
        Box::pin(async move { Err(error) })
    }};
    ($imp:ident, $self:ident, $fam:ident, $name:ident, ($($arg:ident),+)) => {
        Box::pin($self.$imp($($arg),+))
    };
}

macro_rules! remote_features {
    ($( $fam:ident, $mutates:literal : fn $name:ident ( $($arg:ident : $ty:ty),+ ) -> $ret:ty => $imp:ident; )+) => {
        /// Every request/response feature, served by whichever backend the router picked.
        pub trait RemoteFeatures: Send + Sync {
            $(
                fn $name(&self, $($arg: $ty),+) -> BoxFuture<'_, Result<$ret, BackendError>>;
            )+
        }

        impl RemoteFeatures for HelperBackend {
            $(
                fn $name(&self, $($arg: $ty),+) -> BoxFuture<'_, Result<$ret, BackendError>> {
                    let client = std::sync::Arc::clone(&self.client);
                    Box::pin(async move {
                        client.$name($($arg),+).await.map_err(BackendError::from)
                    })
                }
            )+
        }

        impl RemoteFeatures for ShellBackend {
            $(
                fn $name(&self, $($arg: $ty),+) -> BoxFuture<'_, Result<$ret, BackendError>> {
                    shell_call!($imp, self, $fam, $name, ($($arg),+))
                }
            )+
        }

        impl RemoteFeatures for RoutedBackend {
            $(
                #[allow(clippy::clone_on_copy)]
                fn $name(&self, $($arg: $ty),+) -> BoxFuture<'_, Result<$ret, BackendError>> {
                    Box::pin(async move {
                        self.log_route(Family::$fam);
                        let result = async {
                        let probe = self.helper_client().map(HelperProbe::of);
                        if self.router().choose(self.host(), Family::$fam, probe.as_ref())
                            == super::router::BackendKind::Helper
                        {
                            // `choose` returned Helper, so a client is present.
                            if let Some(client) = self.helper_client() {
                                let helper = HelperBackend::new(std::sync::Arc::clone(client));
                                match helper.$name($($arg.clone()),+).await {
                                    Err(e) if e.is_unsupported() => {
                                        // Not executed: safe to serve on the shell, and every
                                        // later call goes there too.
                                        log::info!(
                                            "remote helper does not support {}; using shell mode",
                                            stringify!($name)
                                        );
                                        self.router().note_fallback(
                                            self.host(), client_id(client), Family::$fam);
                                    }
                                    Err(e) if e.is_connection_lost() => {
                                        self.router().note_fallback(
                                            self.host(), client_id(client), Family::$fam);
                                        if $mutates {
                                            // It may have run; do not run it twice.
                                            return Err(e);
                                        }
                                    }
                                    other => return other,
                                }
                            }
                        }
                        self.shell().$name($($arg),+).await
                        }.await;
                        // The app just changed the host itself: whatever it shows of that host
                        // (a diff, a tree, git status) can be stale, and in shell mode nothing
                        // pushes the change.
                        if $mutates && result.is_ok() {
                            self.router().emit_refresh(
                                self.host(), &RemoteRefreshEvent::AfterOwnWrite);
                        }
                        result
                    })
                }
            )+
        }

        /// Every method with its family and whether it mutates the host, for tests and docs.
        pub const METHODS: &[(&str, Family, bool)] = &[
            $( (stringify!($name), Family::$fam, $mutates), )+
        ];
    };
}

remote_features! {
    FileContext, false: fn read_file_context(request: ReadFileContextRequest) -> ReadFileContextResponse => read_file_context_shell;
    RepoMetadata, false: fn navigate_to_directory(path: String) -> NavigatedToDirectoryResponse => navigate_to_directory_shell;
    RepoMetadata, false: fn load_repo_metadata_directory(repo_path: String, dir_path: String) -> LoadRepoMetadataDirectoryResponse => load_repo_metadata_directory_shell;
    Buffers, false: fn open_buffer(path: String, force_reload: bool) -> OpenBufferResponse => open_buffer_shell;
    Buffers, true: fn save_buffer(path: String) -> SaveBufferResponse => save_buffer_shell;
    Buffers, true: fn resolve_conflict(path: String, acknowledged_server_version: u64, client_content: String, current_client_version: u64) -> ResolveConflictResponse => resolve_conflict_shell;
    DiffState, false: fn get_diff_state(request: GetDiffState) -> GetDiffStateResponse => unimplemented;
    GitRead, false: fn get_branches(request: GetBranches) -> GetBranchesResponse => unimplemented;
    GitRead, false: fn get_committed_branch_files(request: GetCommittedBranchFilesRequest) -> GetCommittedBranchFilesResponse => unimplemented;
    RipgrepSearch, false: fn ripgrep_search(request: RipgrepSearchRequest) -> RipgrepSearchResponse => unimplemented;
    GitWrite, true: fn git_commit_chain(request: GitCommitChainRequest) -> GitCommitChainResponse => git_commit_chain_shell;
    GitWrite, true: fn git_push(request: GitPushRequest) -> GitPushResponse => git_push_shell;
    GitWrite, true: fn git_pull(request: GitPullRequest) -> GitPullResponse => git_pull_shell;
    GitWrite, true: fn git_create_pr(request: GitCreatePrRequest) -> GitCreatePrResponse => git_create_pr_shell;
    GitWrite, true: fn discard_files(request: DiscardFilesRequest) -> () => discard_files_shell;
    GitWrite, true: fn git_stage(request: GitStageRequest) -> () => git_stage_shell;
    FileWrite, true: fn write_file(path: String, content: String) -> () => write_file_shell;
    FileWrite, true: fn delete_file(path: String) -> () => delete_file_shell;
    FsBasic, false: fn read_file_bytes(path: String) -> Vec<u8> => read_file_bytes_shell;
    FsBasic, false: fn list_directory(path: String) -> ListDirectoryResponse => list_directory_shell;
    FsBasic, false: fn resolve_path(path: String) -> ResolvePathResponse => unimplemented;
    FsBasic, true: fn create_directory(path: String) -> CreateDirectoryResponse => unimplemented;
}
