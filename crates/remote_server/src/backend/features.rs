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

/// The shell arm of one method: `[]` is "not implemented yet" (Unsupported, or Unavailable
/// without a channel); `[serve_x]` calls the inherent `ShellBackend::serve_x` the family's
/// package wrote.
macro_rules! shell_arm {
    ([] $this:ident, $fam:ident, $name:ident, $($arg:ident),+) => {{
        let _ = ($(&$arg,)+);
        let error = $this.not_implemented(Family::$fam, stringify!($name));
        Box::pin(async move { Err(error) })
    }};
    ([$serve:ident] $this:ident, $fam:ident, $name:ident, $($arg:ident),+) => {
        Box::pin(async move { $this.$serve($($arg),+).await })
    };
}

macro_rules! remote_features {
    ($( $fam:ident, $mutates:literal $(, shell = $serve:ident)? : fn $name:ident ( $($arg:ident : $ty:ty),+ ) -> $ret:ty; )+) => {
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
                    shell_arm!([$($serve)?] self, $fam, $name, $($arg),+)
                }
            )+
        }

        impl RemoteFeatures for RoutedBackend {
            $(
                #[allow(clippy::clone_on_copy)]
                fn $name(&self, $($arg: $ty),+) -> BoxFuture<'_, Result<$ret, BackendError>> {
                    Box::pin(async move {
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
    FileContext, false: fn read_file_context(request: ReadFileContextRequest) -> ReadFileContextResponse;
    RepoMetadata, false: fn navigate_to_directory(path: String) -> NavigatedToDirectoryResponse;
    RepoMetadata, false: fn load_repo_metadata_directory(repo_path: String, dir_path: String) -> LoadRepoMetadataDirectoryResponse;
    Buffers, false: fn open_buffer(path: String, force_reload: bool) -> OpenBufferResponse;
    Buffers, true: fn save_buffer(path: String) -> SaveBufferResponse;
    Buffers, true: fn resolve_conflict(path: String, acknowledged_server_version: u64, client_content: String, current_client_version: u64) -> ResolveConflictResponse;
    DiffState, false: fn get_diff_state(request: GetDiffState) -> GetDiffStateResponse;
    GitRead, false: fn get_branches(request: GetBranches) -> GetBranchesResponse;
    GitRead, false: fn get_committed_branch_files(request: GetCommittedBranchFilesRequest) -> GetCommittedBranchFilesResponse;
    RipgrepSearch, false: fn ripgrep_search(request: RipgrepSearchRequest) -> RipgrepSearchResponse;
    GitWrite, true, shell = serve_git_commit_chain: fn git_commit_chain(request: GitCommitChainRequest) -> GitCommitChainResponse;
    GitWrite, true, shell = serve_git_push: fn git_push(request: GitPushRequest) -> GitPushResponse;
    GitWrite, true, shell = serve_git_pull: fn git_pull(request: GitPullRequest) -> GitPullResponse;
    GitWrite, true, shell = serve_git_create_pr: fn git_create_pr(request: GitCreatePrRequest) -> GitCreatePrResponse;
    GitWrite, true, shell = serve_discard_files: fn discard_files(request: DiscardFilesRequest) -> ();
    GitWrite, true, shell = serve_git_stage: fn git_stage(request: GitStageRequest) -> ();
    FileWrite, true: fn write_file(path: String, content: String) -> ();
    FileWrite, true: fn delete_file(path: String) -> ();
    FsBasic, false: fn list_directory(path: String) -> ListDirectoryResponse;
    FsBasic, false: fn resolve_path(path: String) -> ResolvePathResponse;
    FsBasic, true: fn create_directory(path: String) -> CreateDirectoryResponse;
}
