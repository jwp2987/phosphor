use prost::Message;

use crate::proto::{
    ClientMessage, Initialize, InitializeResponse, ServerMessage, client_message,
    host_scoped_request, notification, server_message, session_scoped_request,
};

use super::*;

#[tokio::test]
async fn round_trip_client_message() {
    let msg = ClientMessage::session_scoped(
        "test-123".to_string(),
        session_scoped_request::Message::Initialize(Initialize {
            auth_token: String::new(),
            codebase_index_limits: None,
            embedding_provider: None,
        }),
    );

    let mut buf = Vec::new();
    write_client_message(&mut buf, &msg).await.unwrap();

    let mut cursor = &buf[..];
    let decoded: ClientMessage = read_client_message(&mut cursor).await.unwrap();

    assert_eq!(decoded.request_id, "test-123");
    match decoded.message {
        Some(client_message::Message::SessionScoped(_)) => {}
        other => panic!("unexpected message variant: {other:?}"),
    }
}

#[tokio::test]
async fn round_trip_client_message_host_scoped() {
    // Direct envelope coverage for #438: a HostScopedRequest round-trips
    // through the same generic read/write path as any other ClientMessage,
    // and decodes back into the HostScoped variant (not SessionScoped or
    // Notification).
    let msg = ClientMessage::host_scoped(
        "host-123".to_string(),
        host_scoped_request::Message::DeleteFile(crate::proto::DeleteFile {
            path: "/tmp/f".to_string(),
        }),
    );

    let mut buf = Vec::new();
    write_client_message(&mut buf, &msg).await.unwrap();

    let mut cursor = &buf[..];
    let decoded: ClientMessage = read_client_message(&mut cursor).await.unwrap();

    assert_eq!(decoded.request_id, "host-123");
    match decoded.message {
        Some(client_message::Message::HostScoped(req)) => {
            assert!(matches!(
                req.message,
                Some(host_scoped_request::Message::DeleteFile(_))
            ));
        }
        other => panic!("unexpected message variant: {other:?}"),
    }
}

#[tokio::test]
async fn round_trip_client_message_notification() {
    // Notifications always carry an empty request_id (fire-and-forget, no
    // response expected) — verify that shape survives the wire round-trip.
    let msg = ClientMessage::notification(notification::Message::Abort(crate::proto::Abort {
        request_id_to_abort: "abc".to_string(),
    }));

    let mut buf = Vec::new();
    write_client_message(&mut buf, &msg).await.unwrap();

    let mut cursor = &buf[..];
    let decoded: ClientMessage = read_client_message(&mut cursor).await.unwrap();

    assert_eq!(decoded.request_id, "");
    match decoded.message {
        Some(client_message::Message::Notification(n)) => {
            assert!(matches!(n.message, Some(notification::Message::Abort(_))));
        }
        other => panic!("unexpected message variant: {other:?}"),
    }
}

/// Direct envelope + wire coverage for #438 dependent feature 1: a full
/// `RemoteAgentContextSnapshot` (with a skill and a global rule file)
/// survives the length-delimited protobuf round trip.
#[tokio::test]
async fn round_trip_remote_agent_context_snapshot() {
    let mut buf = Vec::new();
    write_server_message(
        &mut buf,
        &ServerMessage {
            request_id: String::new(),
            message: Some(server_message::Message::RemoteAgentContextSnapshot(
                crate::proto::RemoteAgentContextSnapshot {
                    revision: 7,
                    home_dir: "/home/user".to_string(),
                    skills: vec![crate::proto::RemoteSkillProto {
                        path: "/home/user/.agents/skills/test/SKILL.md".to_string(),
                        content: "home skill content".to_string(),
                        source: Some(crate::proto::remote_skill_proto::Source::Home(
                            crate::proto::HomeSkillMetadata {},
                        )),
                    }],
                    global_rules: vec![crate::proto::RemoteContextFileProto {
                        path: "/home/user/.agents/AGENTS.md".to_string(),
                        content: "rule content".to_string(),
                    }],
                },
            )),
        },
    )
    .await
    .unwrap();

    let decoded = read_server_message(&mut &buf[..]).await.unwrap();
    match decoded.message {
        Some(server_message::Message::RemoteAgentContextSnapshot(snapshot)) => {
            assert_eq!(snapshot.revision, 7);
            assert_eq!(snapshot.home_dir, "/home/user");
            assert_eq!(snapshot.skills.len(), 1);
            assert_eq!(snapshot.skills[0].content, "home skill content");
            assert!(matches!(
                snapshot.skills[0].source,
                Some(crate::proto::remote_skill_proto::Source::Home(_))
            ));
            assert_eq!(snapshot.global_rules.len(), 1);
            assert_eq!(snapshot.global_rules[0].content, "rule content");
        }
        other => panic!("unexpected message variant: {other:?}"),
    }
}

#[tokio::test]
async fn round_trip_server_message() {
    let msg = ServerMessage {
        request_id: "resp-456".to_string(),
        message: Some(server_message::Message::InitializeResponse(
            InitializeResponse {
                server_version: "0.1.0".to_string(),
                host_id: "test-host".to_string(),
            },
        )),
    };

    let mut buf = Vec::new();
    write_server_message(&mut buf, &msg).await.unwrap();

    let mut cursor = &buf[..];
    let decoded: ServerMessage = read_server_message(&mut cursor).await.unwrap();

    assert_eq!(decoded.request_id, "resp-456");
    match decoded.message {
        Some(server_message::Message::InitializeResponse(resp)) => {
            assert_eq!(resp.server_version, "0.1.0");
        }
        other => panic!("unexpected message variant: {other:?}"),
    }
}

#[tokio::test]
async fn read_unexpected_eof_on_empty_input() {
    let mut cursor: &[u8] = &[];
    let result = read_client_message(&mut cursor).await;
    assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
}

#[tokio::test]
async fn read_truncated_payload() {
    // Write a length prefix claiming 100 bytes, but only provide 4.
    let mut buf = Vec::new();
    buf.extend_from_slice(&100u32.to_le_bytes());
    buf.extend_from_slice(&[0u8; 4]);

    let mut cursor = &buf[..];
    let result = read_client_message(&mut cursor).await;
    assert!(matches!(result, Err(ProtocolError::UnexpectedEof)));
}

#[tokio::test]
async fn round_trip_zero_length_message() {
    // A default ClientMessage with no fields set encodes to zero bytes.
    let msg = ClientMessage::default();

    let mut buf = Vec::new();
    write_client_message(&mut buf, &msg).await.unwrap();

    // The first 4 bytes should be the length (0).
    assert_eq!(&buf[..4], &0u32.to_le_bytes());

    let mut cursor = &buf[..];
    let decoded: ClientMessage = read_client_message(&mut cursor).await.unwrap();
    assert_eq!(decoded.request_id, "");
    assert!(decoded.message.is_none());
}

#[tokio::test]
async fn read_message_too_large() {
    // Write a length prefix exceeding MAX_MESSAGE_SIZE.
    let oversized_len = (MAX_MESSAGE_SIZE as u32) + 1;
    let buf = oversized_len.to_le_bytes();

    let mut cursor = &buf[..];
    let result = read_client_message(&mut cursor).await;
    assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
}

#[tokio::test]
async fn write_message_too_large() {
    // Build a ClientMessage whose encoded size exceeds MAX_MESSAGE_SIZE.
    let msg = ClientMessage {
        request_id: "x".repeat(MAX_MESSAGE_SIZE + 1),
        message: None,
    };

    let mut buf = Vec::new();
    let result = write_client_message(&mut buf, &msg).await;
    assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
    // Nothing should have been written to the stream.
    assert!(buf.is_empty());
}

#[test]
fn try_extract_request_id_from_valid_message() {
    let msg = ClientMessage::session_scoped(
        "abc-123".to_string(),
        session_scoped_request::Message::Initialize(Initialize {
            auth_token: String::new(),
            codebase_index_limits: None,
            embedding_provider: None,
        }),
    );
    let buf = msg.encode_to_vec();
    assert_eq!(try_extract_request_id(&buf), Some("abc-123".to_string()));
}

#[test]
fn try_extract_request_id_from_corrupted_payload_with_valid_id() {
    // Manually construct bytes: valid request_id field followed by
    // corrupt trailing bytes (unterminated varint that would crash
    // a full prost decode but doesn't affect our field-1 extraction).
    let mut buf = Vec::new();
    // Field 1 (string): tag=0x0a, length=7, "req-456"
    buf.push(0x0a);
    buf.push(7);
    buf.extend_from_slice(b"req-456");
    // Corrupt trailing bytes: unterminated varint (all continuation bits set).
    buf.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

    // request_id should still be extractable despite trailing corruption.
    assert_eq!(try_extract_request_id(&buf), Some("req-456".to_string()));
}
#[test]
fn try_extract_request_id_from_empty_bytes() {
    assert_eq!(try_extract_request_id(&[]), None);
}

#[test]
fn try_extract_request_id_from_garbage_bytes() {
    // Completely random bytes that don't form a valid protobuf.
    // This may or may not decode depending on what prost makes of it,
    // but should not panic. If it decodes to an empty request_id, we
    // return None.
    let result = try_extract_request_id(&[0xFF, 0xFF, 0xFF, 0xFF]);
    // We don't assert a specific value — just that it doesn't panic.
    // If prost happens to decode something, it'll be empty or garbage.
    let _ = result;
}

#[tokio::test]
async fn decode_error_extracts_request_id() {
    // Construct a corrupted message with a valid request_id field.
    let mut payload = Vec::new();
    // Field 1 (string): tag=0x0a, length=6, "req-42"
    payload.push(0x0a);
    payload.push(6);
    payload.extend_from_slice(b"req-42");
    // Invalid trailing bytes that cause prost decode failure.
    payload.extend_from_slice(&[0x0F, 0x01]);

    let mut buf = Vec::new();
    buf.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    buf.extend_from_slice(&payload);

    let mut cursor = &buf[..];
    let result = read_client_message(&mut cursor).await;
    match result {
        Err(ProtocolError::Decode(_, Some(id))) => {
            assert_eq!(id.to_string(), "req-42");
        }
        other => panic!("expected Decode error with request_id, got: {other:?}"),
    }
}

#[tokio::test]
async fn decode_error_none_when_no_request_id() {
    // Completely invalid protobuf bytes with no valid field 1.
    let garbage = vec![0xFF, 0xFE, 0xFD, 0xFC];
    let mut buf = Vec::new();
    buf.extend_from_slice(&(garbage.len() as u32).to_le_bytes());
    buf.extend_from_slice(&garbage);

    let mut cursor = &buf[..];
    let result = read_client_message(&mut cursor).await;
    match result {
        Err(ProtocolError::Decode(_, None)) => {}
        other => panic!("expected Decode error with None request_id, got: {other:?}"),
    }
}
