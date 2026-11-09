//! Remote server: the wire protocol, the ssh client side, and (feature `daemon`) the small
//! warpui-free `phosphor-remote` helper that runs on the remote host.
//!
//! Features (Wave H, #1804):
//! * `client` (default): everything the app needs -- the manager model, the transport and client,
//!   install/setup, and the conversions to `repo_metadata` types. Depends on warpui.
//! * `daemon`: the helper's server core and `phosphor-remote` binary. Needs only the always-on
//!   modules below, so `--no-default-features --features daemon` builds without warpui.
//!
//! The always-on modules are the protocol (`proto`, `protocol`, `frames`, `host_response`), the
//! ssh command helpers, and the `rt` stand-ins.

pub mod auth;
pub mod capability;
pub mod codebase_index_proto;
pub mod frames;
pub mod host_response;
pub mod naming;
pub mod policy;
pub mod protocol;
pub mod rt;
pub mod shellcmd;
pub mod ssh;

#[cfg(feature = "client")]
pub mod backend;
#[cfg(feature = "client")]
pub mod client;
#[cfg(all(feature = "daemon", unix))]
pub mod daemon;
#[cfg(feature = "client")]
pub mod host_id;
#[cfg(feature = "client")]
pub mod manager;
#[cfg(feature = "client")]
pub mod repo_metadata_proto;
#[cfg(feature = "client")]
pub mod setup;
#[cfg(feature = "client")]
pub mod transport;

#[cfg(feature = "client")]
pub use host_id::HostId;

#[allow(clippy::large_enum_variant)]
pub mod proto {
    include!(concat!(env!("OUT_DIR"), "/remote_server.rs"));

    // ── ClientMessage constructors ──────────────────────────────────
    //
    // These helpers wrap inner message types in the appropriate
    // HostScopedRequest / SessionScopedRequest / Notification envelope
    // so call sites don't need triple-nested struct literals.

    impl ClientMessage {
        /// Build a `ClientMessage` carrying a host-scoped request.
        pub fn host_scoped(request_id: String, inner: host_scoped_request::Message) -> Self {
            Self {
                request_id,
                message: Some(client_message::Message::HostScoped(HostScopedRequest {
                    message: Some(inner),
                })),
            }
        }

        /// Build a `ClientMessage` carrying a session-scoped request.
        pub fn session_scoped(request_id: String, inner: session_scoped_request::Message) -> Self {
            Self {
                request_id,
                message: Some(client_message::Message::SessionScoped(
                    SessionScopedRequest {
                        message: Some(inner),
                    },
                )),
            }
        }

        /// Build a `ClientMessage` carrying a notification (fire-and-forget).
        pub fn notification(inner: notification::Message) -> Self {
            Self {
                request_id: String::new(),
                message: Some(client_message::Message::Notification(Notification {
                    message: Some(inner),
                })),
            }
        }
    }
}
