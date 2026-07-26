//! This module contains all code relevant to Agent Predict within Zap.
//!
//! Agent Predict attempts to predict the next action the user will take in Zap.

pub(crate) mod generate_ai_input_suggestions;
pub(crate) mod generate_am_query_suggestions;
pub mod next_command_model;
// Zap (Wave 3-2): the `predict_am_queries` API module has been physically deleted —
// the original `ServerApi::predict_am_queries` had 0 external consumers and was
// deleted in the same pass; `FeatureFlag::PredictAMQueries` /
// `predict_am_queries_future_handle` in terminal/input.rs are kept only as a
// control-toggle/handle-name placeholder, and no longer need this module.
pub mod prompt_suggestions;
