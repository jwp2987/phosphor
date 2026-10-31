//! Large frames on the remote-server connection (#1244).
//!
//! The protocol is one length-delimited protobuf frame per message, written by a single writer
//! per direction. A `RunCommandResponse` carrying megabytes of output, an 8 MB `ReadFileChunk`
//! or a whole-repo `RepoMetadataSnapshot` therefore held every smaller frame behind it -- the
//! 1 KB `git branch` result for the Tab the user just pressed waited for the whole file to drain
//! through the proxy, sshd and the channel window. Two pieces fix that, both daemon -> client:
//!
//! * [`chunk_frame`] / [`ChunkReassembler`]: a frame larger than [`CHUNK_PAYLOAD_BYTES`] is split
//!   into `FrameChunk` messages the client concatenates and decodes as the original message. Only
//!   a client that announced `chunked_responses` in `Initialize` receives chunks; an old client
//!   gets whole frames as before, and an old daemon never sends chunks.
//! * [`OutboundScheduler`]: the daemon's writer keeps two lanes. Frames up to the chunk size are
//!   interactive and go out first, in order; larger frames are bulk and go out one chunk (or, for
//!   an old client, one whole frame) at a time whenever the interactive lane is empty. An
//!   interactive frame therefore waits for at most one chunk already being written.

use std::collections::{HashMap, VecDeque};

use prost::Message;

use crate::proto::{FrameChunk, ServerMessage, server_message};
use crate::protocol::{MAX_MESSAGE_SIZE, ProtocolError, RequestId};

/// Payload bytes per `FrameChunk`, and the size above which a frame is bulk. Well under a TCP
/// window on any link that matters, so an interactive frame queued behind one chunk is delayed by
/// about one chunk's transfer time; large enough that framing overhead (a few dozen bytes) and the
/// per-frame flush stay negligible for a multi-MB response.
pub const CHUNK_PAYLOAD_BYTES: usize = 64 * 1024;

/// Splits an encoded `ServerMessage` into `FrameChunk` messages of at most `chunk_bytes` payload
/// each, the last one marked. An empty frame yields one empty, last chunk.
pub fn chunk_frame(encoded: &[u8], stream_id: &str, chunk_bytes: usize) -> Vec<ServerMessage> {
    let chunk_bytes = chunk_bytes.max(1);
    let mut chunks: Vec<ServerMessage> = encoded
        .chunks(chunk_bytes)
        .map(|payload| frame_chunk(stream_id, payload.to_vec(), false))
        .collect();
    match chunks.last_mut() {
        Some(last) => set_last(last),
        None => chunks.push(frame_chunk(stream_id, Vec::new(), true)),
    }
    chunks
}

fn frame_chunk(stream_id: &str, payload: Vec<u8>, last: bool) -> ServerMessage {
    ServerMessage {
        request_id: String::new(),
        message: Some(server_message::Message::FrameChunk(FrameChunk {
            stream_id: stream_id.to_owned(),
            payload,
            last,
        })),
    }
}

fn set_last(msg: &mut ServerMessage) {
    if let Some(server_message::Message::FrameChunk(chunk)) = &mut msg.message {
        chunk.last = true;
    }
}

/// Client side: collects the pieces of each stream and yields the decoded message when its last
/// piece arrives. A stream that grows past [`MAX_MESSAGE_SIZE`] is dropped with an error, so a
/// corrupt or hostile daemon cannot make the client allocate without bound.
#[derive(Debug, Default)]
pub struct ChunkReassembler {
    streams: HashMap<String, Vec<u8>>,
}

impl ChunkReassembler {
    /// Accepts one chunk. `Ok(None)` while a stream is incomplete; `Ok(Some(msg))` when this chunk
    /// completed it; `Err` for a stream that exceeded the size cap (dropped) or whose bytes do not
    /// decode (the error carries the request id when field 1 of the bytes is readable, so the
    /// caller can fail that request instead of letting it time out).
    pub fn accept(&mut self, chunk: FrameChunk) -> Result<Option<ServerMessage>, ProtocolError> {
        let FrameChunk {
            stream_id,
            payload,
            last,
        } = chunk;
        let buffer = self.streams.entry(stream_id.clone()).or_default();
        if buffer.len() + payload.len() > MAX_MESSAGE_SIZE {
            let size = buffer.len() + payload.len();
            self.streams.remove(&stream_id);
            return Err(ProtocolError::MessageTooLarge {
                size,
                max: MAX_MESSAGE_SIZE,
            });
        }
        buffer.extend_from_slice(&payload);
        if !last {
            return Ok(None);
        }
        let bytes = self.streams.remove(&stream_id).unwrap_or_default();
        ServerMessage::decode(bytes.as_slice())
            .map(Some)
            .map_err(|e| {
                let request_id =
                    crate::protocol::try_extract_request_id(&bytes).map(RequestId::from);
                ProtocolError::Decode(e, request_id)
            })
    }

    /// How many streams are mid-flight.
    pub fn pending_streams(&self) -> usize {
        self.streams.len()
    }
}

/// One bulk item on the daemon's writer: a chunked frame still being drained, or a whole large
/// frame for a client that cannot reassemble chunks.
#[derive(Debug)]
enum BulkItem {
    Chunks {
        frames: VecDeque<Vec<u8>>,
        push: bool,
    },
    Whole {
        frame: Vec<u8>,
        push: bool,
    },
}

impl BulkItem {
    fn is_push(&self) -> bool {
        match self {
            BulkItem::Chunks { push, .. } | BulkItem::Whole { push, .. } => *push,
        }
    }
}

/// Daemon side: the two outbound lanes of one connection. `enqueue` encodes and classifies a
/// message; `next_frame` hands the writer the next encoded frame to write.
///
/// Ordering: interactive frames keep their order among themselves, bulk items keep theirs, and
/// a response (non-empty request id) may overtake an earlier bulk response because requests are
/// independent. Push messages (empty request id: snapshots, status pushes) are different -- an
/// update must not arrive before the snapshot it amends -- so while any bulk push is pending,
/// later pushes queue behind it in the bulk lane.
#[derive(Debug)]
pub struct OutboundScheduler {
    interactive: VecDeque<Vec<u8>>,
    bulk: VecDeque<BulkItem>,
    chunk_bytes: usize,
    bulk_pushes_pending: usize,
}

impl OutboundScheduler {
    pub fn new(chunk_bytes: usize) -> Self {
        Self {
            interactive: VecDeque::new(),
            bulk: VecDeque::new(),
            chunk_bytes: chunk_bytes.max(1),
            bulk_pushes_pending: 0,
        }
    }

    /// Encodes `msg` and queues it. `chunking` is whether the connection's client reassembles
    /// `FrameChunk`s. A message that cannot be sent at all (over [`MAX_MESSAGE_SIZE`]) is
    /// rejected here, before anything is queued, exactly as the writer used to reject it.
    pub fn enqueue(&mut self, msg: &ServerMessage, chunking: bool) -> Result<(), ProtocolError> {
        let encoded = msg.encode_to_vec();
        if encoded.len() > MAX_MESSAGE_SIZE {
            return Err(ProtocolError::MessageTooLarge {
                size: encoded.len(),
                max: MAX_MESSAGE_SIZE,
            });
        }
        let push = msg.request_id.is_empty();
        let small = encoded.len() <= self.chunk_bytes;
        if small && !(push && self.bulk_pushes_pending > 0) {
            self.interactive.push_back(encoded);
            return Ok(());
        }
        let item = if chunking && !small {
            let stream_id = uuid::Uuid::new_v4().to_string();
            let frames = chunk_frame(&encoded, &stream_id, self.chunk_bytes)
                .iter()
                .map(Message::encode_to_vec)
                .collect();
            BulkItem::Chunks { frames, push }
        } else {
            BulkItem::Whole {
                frame: encoded,
                push,
            }
        };
        if push {
            self.bulk_pushes_pending += 1;
        }
        self.bulk.push_back(item);
        Ok(())
    }

    /// The next encoded frame to write: an interactive frame if any, else one piece of the oldest
    /// bulk item, else `None`.
    pub fn next_frame(&mut self) -> Option<Vec<u8>> {
        if let Some(frame) = self.interactive.pop_front() {
            return Some(frame);
        }
        let item = self.bulk.front_mut()?;
        let (frame, done) = match item {
            BulkItem::Whole { frame, .. } => (std::mem::take(frame), true),
            BulkItem::Chunks { frames, .. } => {
                let frame = frames.pop_front().unwrap_or_default();
                (frame, frames.is_empty())
            }
        };
        if done {
            let finished = self.bulk.pop_front();
            if finished.is_some_and(|item| item.is_push()) {
                self.bulk_pushes_pending = self.bulk_pushes_pending.saturating_sub(1);
            }
        }
        Some(frame)
    }

    pub fn is_empty(&self) -> bool {
        self.interactive.is_empty() && self.bulk.is_empty()
    }
}

#[cfg(test)]
#[path = "frames_tests.rs"]
mod tests;
