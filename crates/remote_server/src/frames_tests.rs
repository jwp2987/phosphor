use prost::Message;

use crate::proto::{RunCommandResponse, RunCommandSuccess, ServerMessage, server_message};
use crate::protocol::{MAX_MESSAGE_SIZE, ProtocolError, read_server_message};

use super::*;

const CHUNK: usize = 1024;

fn response(request_id: &str, stdout_len: usize) -> ServerMessage {
    ServerMessage {
        request_id: request_id.to_owned(),
        message: Some(server_message::Message::RunCommandResponse(
            RunCommandResponse {
                result: Some(crate::proto::run_command_response::Result::Success(
                    RunCommandSuccess {
                        stdout: vec![b'x'; stdout_len],
                        stderr: Vec::new(),
                        exit_code: Some(0),
                    },
                )),
            },
        )),
    }
}

fn push(payload_len: usize) -> ServerMessage {
    ServerMessage {
        request_id: String::new(),
        ..response("", payload_len)
    }
}

fn request_id_of(frame: &[u8]) -> String {
    ServerMessage::decode(frame).unwrap().request_id
}

fn is_chunk(frame: &[u8]) -> bool {
    matches!(
        ServerMessage::decode(frame).unwrap().message,
        Some(server_message::Message::FrameChunk(_))
    )
}

/// Feeds every frame the scheduler emits through the real wire reader and the client-side
/// reassembler: what an up-to-date client sees.
async fn receive_all(frames: Vec<Vec<u8>>) -> Vec<ServerMessage> {
    let mut wire = Vec::new();
    for frame in &frames {
        crate::protocol::write_encoded_message(&mut wire, frame)
            .await
            .unwrap();
    }
    let mut cursor = &wire[..];
    let mut reassembler = ChunkReassembler::default();
    let mut received = Vec::new();
    while !cursor.is_empty() {
        let msg = read_server_message(&mut cursor).await.unwrap();
        match msg.message {
            Some(server_message::Message::FrameChunk(chunk)) => {
                if let Some(whole) = reassembler.accept(chunk).unwrap() {
                    received.push(whole);
                }
            }
            _ => received.push(msg),
        }
    }
    assert_eq!(reassembler.pending_streams(), 0);
    received
}

fn drain(scheduler: &mut OutboundScheduler) -> Vec<Vec<u8>> {
    let mut frames = Vec::new();
    while let Some(frame) = scheduler.next_frame() {
        frames.push(frame);
    }
    frames
}

#[test]
fn a_large_frame_round_trips_through_chunks() {
    let original = response("big", 10 * CHUNK + 17);
    let encoded = original.encode_to_vec();
    let chunks = chunk_frame(&encoded, "s1", CHUNK);
    assert_eq!(chunks.len(), 11);
    let mut reassembler = ChunkReassembler::default();
    for (i, chunk) in chunks.into_iter().enumerate() {
        let Some(server_message::Message::FrameChunk(chunk)) = chunk.message else {
            panic!("not a chunk");
        };
        assert!(chunk.payload.len() <= CHUNK);
        assert_eq!(chunk.last, i == 10);
        let out = reassembler.accept(chunk).unwrap();
        if i < 10 {
            assert!(out.is_none());
            assert_eq!(reassembler.pending_streams(), 1);
        } else {
            assert_eq!(out, Some(original.clone()));
        }
    }
    assert_eq!(reassembler.pending_streams(), 0);
}

#[test]
fn an_empty_frame_is_one_last_chunk() {
    let chunks = chunk_frame(&[], "s", CHUNK);
    assert_eq!(chunks.len(), 1);
    let Some(server_message::Message::FrameChunk(chunk)) =
        chunks.into_iter().next().unwrap().message
    else {
        panic!("not a chunk");
    };
    assert!(chunk.last && chunk.payload.is_empty());
}

#[test]
fn interleaved_streams_reassemble_independently() {
    let a = response("a", 3 * CHUNK);
    let b = response("b", 2 * CHUNK);
    let a_chunks = chunk_frame(&a.encode_to_vec(), "a", CHUNK);
    let b_chunks = chunk_frame(&b.encode_to_vec(), "b", CHUNK);
    let mut reassembler = ChunkReassembler::default();
    let mut out = Vec::new();
    for pair in a_chunks
        .into_iter()
        .zip(b_chunks.into_iter().chain(std::iter::repeat_with(|| {
            frame_chunk("unused", Vec::new(), true)
        })))
    {
        for msg in [pair.0, pair.1] {
            let Some(server_message::Message::FrameChunk(chunk)) = msg.message else {
                panic!("not a chunk");
            };
            if let Some(whole) = reassembler.accept(chunk).unwrap() {
                out.push(whole.request_id);
            }
        }
    }
    // `b` (2 chunks) completes during the second pair, `a` during the third; the filler stream
    // "unused" completes as empty bytes, which decode to a default message.
    assert_eq!(out.iter().filter(|id| *id == "a").count(), 1);
    assert_eq!(out.iter().filter(|id| *id == "b").count(), 1);
    assert_eq!(reassembler.pending_streams(), 0);
}

#[test]
fn a_stream_over_the_message_cap_is_dropped_with_an_error() {
    let mut reassembler = ChunkReassembler::default();
    let first = FrameChunk {
        stream_id: "s".into(),
        payload: vec![0; MAX_MESSAGE_SIZE],
        last: false,
    };
    assert!(reassembler.accept(first).unwrap().is_none());
    let overflow = FrameChunk {
        stream_id: "s".into(),
        payload: vec![0; 1],
        last: true,
    };
    assert!(matches!(
        reassembler.accept(overflow),
        Err(ProtocolError::MessageTooLarge { .. })
    ));
    assert_eq!(reassembler.pending_streams(), 0);
}

#[test]
fn undecodable_stream_bytes_report_the_request_id_when_readable() {
    let mut reassembler = ChunkReassembler::default();
    // Field 1 (request_id = "r1") followed by garbage for the oneof.
    let mut bytes = vec![0x0a, 0x02, b'r', b'1'];
    bytes.extend_from_slice(&[0x12, 0xff, 0xff, 0xff, 0xff, 0xff]);
    let result = reassembler.accept(FrameChunk {
        stream_id: "s".into(),
        payload: bytes,
        last: true,
    });
    match result {
        Err(ProtocolError::Decode(_, Some(id))) => assert_eq!(id.to_string(), "r1"),
        other => panic!("expected a decode error with the request id, got {other:?}"),
    }
}

/// #1244: the interactive lane goes first; a bulk response is written one chunk at a time, so an
/// interactive frame queued while a bulk frame drains waits for at most one chunk.
#[tokio::test]
async fn an_interactive_frame_waits_for_at_most_one_chunk() {
    let mut scheduler = OutboundScheduler::new(CHUNK);
    scheduler
        .enqueue(&response("big", 8 * CHUNK), true)
        .unwrap();
    // The writer takes the first chunk...
    let first = scheduler.next_frame().unwrap();
    assert!(is_chunk(&first));
    // ...meanwhile a small response arrives: it is the very next frame.
    scheduler.enqueue(&response("small", 10), true).unwrap();
    let second = scheduler.next_frame().unwrap();
    assert!(!is_chunk(&second));
    assert_eq!(request_id_of(&second), "small");
    // Then the rest of the bulk item, in order, and everything decodes on the client.
    let mut frames = vec![first, second];
    frames.extend(drain(&mut scheduler));
    assert!(scheduler.is_empty());
    let received = receive_all(frames).await;
    let ids: Vec<_> = received.iter().map(|m| m.request_id.as_str()).collect();
    assert_eq!(ids, ["small", "big"]);
    assert_eq!(received[1], response("big", 8 * CHUNK));
}

#[test]
fn small_frames_keep_their_order_and_bulk_items_keep_theirs() {
    let mut scheduler = OutboundScheduler::new(CHUNK);
    scheduler.enqueue(&response("s1", 1), true).unwrap();
    scheduler.enqueue(&response("b1", 2 * CHUNK), true).unwrap();
    scheduler.enqueue(&response("s2", 1), true).unwrap();
    scheduler.enqueue(&response("b2", 2 * CHUNK), true).unwrap();
    let frames = drain(&mut scheduler);
    assert_eq!(request_id_of(&frames[0]), "s1");
    assert_eq!(request_id_of(&frames[1]), "s2");
    let received = futures::executor::block_on(receive_all(frames));
    let ids: Vec<_> = received.iter().map(|m| m.request_id.as_str()).collect();
    assert_eq!(ids, ["s1", "s2", "b1", "b2"]);
}

/// Old client, new daemon: the client never asked for chunks, so a large frame is written whole
/// (one frame an old reader decodes as the original message), but still after the interactive
/// frames already waiting.
#[tokio::test]
async fn a_client_without_chunking_gets_whole_frames_after_the_interactive_lane() {
    let mut scheduler = OutboundScheduler::new(CHUNK);
    scheduler
        .enqueue(&response("big", 5 * CHUNK), false)
        .unwrap();
    scheduler.enqueue(&response("small", 1), false).unwrap();
    let frames = drain(&mut scheduler);
    assert_eq!(frames.len(), 2);
    assert!(frames.iter().all(|f| !is_chunk(f)));
    // Exactly what an old client's reader sees: two plain ServerMessages, small first.
    let mut wire = Vec::new();
    for frame in &frames {
        crate::protocol::write_encoded_message(&mut wire, frame)
            .await
            .unwrap();
    }
    let mut cursor = &wire[..];
    let first = read_server_message(&mut cursor).await.unwrap();
    let second = read_server_message(&mut cursor).await.unwrap();
    assert_eq!(first.request_id, "small");
    assert_eq!(second, response("big", 5 * CHUNK));
}

/// New client, old daemon: whole frames (the daemon ignores the capability) pass through the
/// client's path untouched -- `receive_all` only reassembles FrameChunks.
#[tokio::test]
async fn a_new_client_still_reads_whole_frames_from_an_old_daemon() {
    let whole = response("legacy", 5 * CHUNK).encode_to_vec();
    let received = receive_all(vec![whole]).await;
    assert_eq!(received, vec![response("legacy", 5 * CHUNK)]);
}

/// Pushes amend each other (a snapshot, then updates), so a small push never overtakes a bulk
/// push that is still draining; responses to independent requests may.
#[test]
fn pushes_keep_their_order_behind_a_bulk_push() {
    let mut scheduler = OutboundScheduler::new(CHUNK);
    scheduler.enqueue(&push(3 * CHUNK), true).unwrap();
    scheduler.enqueue(&push(5), true).unwrap();
    scheduler.enqueue(&response("r", 5), true).unwrap();
    let frames = drain(&mut scheduler);
    // The response goes first (interactive lane); the two pushes follow in order.
    assert_eq!(request_id_of(&frames[0]), "r");
    let received = futures::executor::block_on(receive_all(frames));
    assert_eq!(received.len(), 3);
    assert_eq!(received[0].request_id, "r");
    assert_eq!(received[1], push(3 * CHUNK));
    assert_eq!(received[2], push(5));

    // Once the bulk push has drained, small pushes are interactive again.
    scheduler.enqueue(&push(6), true).unwrap();
    scheduler
        .enqueue(&response("big", 2 * CHUNK), true)
        .unwrap();
    let frames = drain(&mut scheduler);
    assert_eq!(request_id_of(&frames[0]), "");
}

#[test]
fn an_oversized_message_is_rejected_before_anything_is_queued() {
    let mut scheduler = OutboundScheduler::new(CHUNK);
    let result = scheduler.enqueue(&response("huge", MAX_MESSAGE_SIZE + 1), true);
    assert!(matches!(result, Err(ProtocolError::MessageTooLarge { .. })));
    assert!(scheduler.is_empty());
}
