//! The agent conversation protocol: `Message`, `Task`, `Request`, `ResponseEvent` and the
//! rest of the `warp.multi_agent.v1` package, generated from `proto/*.proto` by `build.rs`.
//!
//! Brought in-repo from github.com/warpdotdev/warp-proto-apis at f0028fa6d (AGPL-3.0-only,
//! like this repository) so upstream server changes no longer arrive with a dependency bump.
//! The proto package name stays `warp.multi_agent.v1`: it is part of the descriptor names,
//! so renaming it is a separate, deliberate change. Field numbers must never be reused --
//! saved conversations are encoded with them.

use std::sync::LazyLock;

use prost_reflect::DescriptorPool;

static FILE_DESCRIPTOR_SET: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/file_descriptor_set.bin"));

static DESCRIPTOR_POOL: LazyLock<DescriptorPool> = LazyLock::new(|| {
    DescriptorPool::decode(FILE_DESCRIPTOR_SET).expect("Failed to load file descriptor set")
});

pub static MESSAGE_DESCRIPTOR: LazyLock<prost_reflect::MessageDescriptor> = LazyLock::new(|| {
    get_descriptor_pool()
        .get_message_by_name("warp.multi_agent.v1.Message")
        .expect("Proto definition exists.")
});

// Re-export all generated types.
include!(concat!(env!("OUT_DIR"), "/warp.multi_agent.v1.rs"));

/// Returns the descriptor pool for the generated types.
///
/// This enables clients to use reflection APIs on the types defined
/// in this crate.
pub fn get_descriptor_pool() -> &'static DescriptorPool {
    &DESCRIPTOR_POOL
}
