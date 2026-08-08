//! Helpers for interpreting raw `ServerMessage` responses to file-operation
//! requests (`WriteFile`, `SaveBuffer`, `DeleteFile`, `DiscardFiles`).
//!
//! [`crate::client::RemoteServerClient`] already inlines this parsing at each
//! call site; these helpers exist so the response-interpretation logic for
//! the standard `Success | Error | (unset)` result shape can be exercised in
//! isolation, without spinning up a mock server round-trip for every
//! success/error/malformed-response combination.
//!
//! Each helper returns `Ok(())` on success or `Err(message)` with the
//! server-provided error message on failure. Failure includes both an
//! `Error` variant and a missing (`None`) `result`: the daemon always
//! populates exactly one of `success`/`error`, so an unset result is a
//! malformed/never-populated response, never a benign success.

use crate::proto::{server_message, ServerMessage};

/// Interprets a per-operation response with the standard
/// `Success | Error | (unset)` result shape. A missing `result` is an error
/// (see module docs).
macro_rules! file_op_result {
    ($msg:expr, $variant:path, $result:path, $op:literal) => {{
        use $result as R;
        match &$msg.message {
            Some($variant(resp)) => match &resp.result {
                Some(R::Success(_)) => Ok(()),
                Some(R::Error(e)) => Err(e.message.clone()),
                None => Err(format!("Empty {} response", $op)),
            },
            other => Err(unexpected_variant($op, other)),
        }
    }};
}

/// Interprets a [`ServerMessage`] as the result of a `WriteFile` request.
pub fn write_file_result(msg: &ServerMessage) -> Result<(), String> {
    file_op_result!(
        msg,
        server_message::Message::WriteFileResponse,
        crate::proto::write_file_response::Result,
        "WriteFile"
    )
}

/// Interprets a [`ServerMessage`] as the result of a `SaveBuffer` request.
pub fn save_buffer_result(msg: &ServerMessage) -> Result<(), String> {
    file_op_result!(
        msg,
        server_message::Message::SaveBufferResponse,
        crate::proto::save_buffer_response::Result,
        "SaveBuffer"
    )
}

/// Interprets a [`ServerMessage`] as the result of a `DeleteFile` request.
pub fn delete_file_result(msg: &ServerMessage) -> Result<(), String> {
    file_op_result!(
        msg,
        server_message::Message::DeleteFileResponse,
        crate::proto::delete_file_response::Result,
        "DeleteFile"
    )
}

/// Interprets a [`ServerMessage`] as the result of a `DiscardFiles` request.
pub fn discard_files_result(msg: &ServerMessage) -> Result<(), String> {
    file_op_result!(
        msg,
        server_message::Message::DiscardFilesResponse,
        crate::proto::discard_files_response::Result,
        "DiscardFiles"
    )
}

fn unexpected_variant(op: &str, other: &Option<server_message::Message>) -> String {
    format!("Unexpected response variant for {op}: {other:?}")
}

#[cfg(test)]
#[path = "host_response_tests.rs"]
mod tests;
