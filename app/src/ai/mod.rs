//! This module should houses all horizontal/cross-cutting AI functionality throughout
//! Zap (including Agent Mode).
//!
//! The side panel Zap AI implementation lives in `super::ai_assistant`.
pub(crate) mod agent;
pub(crate) mod agent_conversations_model;
pub(crate) mod agent_events;
pub(crate) mod agent_providers;
pub(crate) mod agent_tips;
pub(crate) mod ai_document_view;
pub mod ambient_agents;
pub(crate) mod api_error;
pub(crate) mod artifact_download;
pub mod artifacts;
pub(crate) mod attachment_utils;
#[cfg(not(target_family = "wasm"))]
pub mod aws_credentials;
pub(crate) mod block_context;
pub(crate) mod blocklist;
pub(crate) mod byop_compaction;
pub(crate) mod byop_readiness;
pub mod control_code_parser;
pub(crate) mod conversation_export;
pub(crate) mod conversation_entry;
pub(crate) mod conversation_navigation;
pub(crate) mod conversation_status_ui;
pub(crate) mod conversation_utils;
pub(crate) mod document;
pub(crate) mod harness_display;
pub(crate) mod llms;
pub mod onboarding;
pub(crate) mod option_snapshot;
pub(crate) mod predict;
pub(crate) mod project_rules_persister;
pub mod request_usage_model;
pub(crate) mod restored_conversations;
pub(crate) mod skills;
pub mod usage_cost;
pub(crate) mod voice;
pub use agent_tips::*;
pub use request_usage_model::*;
use warpui::AppContext;
#[cfg(not(target_family = "wasm"))]
pub mod agent_sdk;
// Zap Wave 7-3: `ambient_agent_settings` was physically deleted along with the ambient-agent UI subsystem.
// Zap Wave 7-2: cloud environments' CLI / form / environment-prep chain has been
// deleted; the local object data type still lives here temporarily, for
// ObjectStoreModel's deserialization and existing view filtering to use.
pub mod execution_profiles;
pub mod facts;
// Zap Wave 6-8: `generate_block_title` was removed along with the
// `BlockClient::generate_shared_block_title` stub — its only consumer was the
// BlockClient trait signature, with no other local code path.
pub(crate) mod loading;
pub mod mcp;
pub mod outline;

pub(crate) use ai::paths;

pub fn init(app: &mut AppContext) {
    blocklist::keyboard_navigable_buttons::init(app);
    blocklist::block::number_shortcut_buttons::init(app);
    blocklist::toggleable_items::init(app);
    blocklist::suggested_agent_mode_workflow_modal::init(app);
    blocklist::suggested_rule_modal::init(app);
    ai_document_view::init(app);
}
