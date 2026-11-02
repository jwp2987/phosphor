use base64::{engine::general_purpose::STANDARD_NO_PAD, Engine as _};
use prost::Message;
use warp_multi_agent_api::ResponseEvent;

/// Encodes a ResponseEvent by protobuf-encoding it and base64-encoding the bytes.
pub fn encode_agent_response_event(event: &ResponseEvent) -> String {
    let bytes = event.encode_to_vec();
    STANDARD_NO_PAD.encode(bytes)
}
