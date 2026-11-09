//! The helper (or legacy full daemon) arm: every call is the matching [`RemoteServerClient`]
//! request, unchanged. The [`RemoteFeatures`](super::RemoteFeatures) impl is generated in
//! `features.rs` so the three backends cannot drift apart.

use std::sync::Arc;

use crate::client::RemoteServerClient;

#[derive(Clone)]
pub struct HelperBackend {
    pub(super) client: Arc<RemoteServerClient>,
}

impl std::fmt::Debug for HelperBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HelperBackend").finish_non_exhaustive()
    }
}

impl HelperBackend {
    pub fn new(client: Arc<RemoteServerClient>) -> Self {
        Self { client }
    }

    /// Identity of the underlying connection, to tell a reconnect from the same connection.
    pub(super) fn client_id(&self) -> usize {
        client_id(&self.client)
    }
}

pub(super) fn client_id(client: &Arc<RemoteServerClient>) -> usize {
    Arc::as_ptr(client) as *const () as usize
}
