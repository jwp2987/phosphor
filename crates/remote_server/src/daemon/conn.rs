//! One proxy connection to the daemon: a reader thread that decodes `ClientMessage`s and a
//! writer thread that drains the connection's outbound queue.
//!
//! Ported from the app daemon's `handle_daemon_connection` (`app/src/remote_server/unix/mod.rs`)
//! with threads and `async_io::block_on` in place of the warpui executor. The reader is never
//! cancelled mid-read, which avoids the framing desynchronisation a read polled inside a
//! `select!` branch would cause.

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Sender;

use futures::AsyncReadExt as _;
use futures::io::{AsyncWriteExt, BufReader};

use crate::frames::{CHUNK_PAYLOAD_BYTES, OutboundScheduler};
use crate::proto::{ErrorCode, ErrorResponse, ServerMessage, server_message};
use crate::protocol::{self, ProtocolError};

use super::server::{ConnId, Event};

/// Starts the reader and writer threads for `stream`. The server learns of the connection
/// through [`Event::Connected`] before any of its messages.
pub(super) fn spawn_connection(stream: UnixStream, conn: ConnId, events: Sender<Event>) {
    let stream = match async_io::Async::new(stream) {
        Ok(s) => s,
        Err(e) => {
            log::error!("Daemon: async stream error for conn {conn}: {e}");
            return;
        }
    };
    let (conn_tx, conn_rx) = async_channel::unbounded::<ServerMessage>();
    // #1244: set by the server when this connection's client announces it reassembles chunked
    // frames; read by the writer on every message.
    let chunking = Arc::new(AtomicBool::new(false));
    if events
        .send(Event::Connected {
            conn,
            tx: conn_tx.clone(),
            chunking: chunking.clone(),
        })
        .is_err()
    {
        return;
    }

    let (read_half, mut write_half) = stream.split();

    let reader_events = events.clone();
    let spawned = std::thread::Builder::new()
        .name(format!("conn-{conn}-read"))
        .spawn(move || {
            async_io::block_on(async move {
                let mut reader = BufReader::new(read_half);
                loop {
                    match protocol::read_client_message(&mut reader).await {
                        Ok(msg) => {
                            if reader_events.send(Event::Message { conn, msg }).is_err() {
                                log::warn!("Daemon: server gone, closing conn {conn}");
                                break;
                            }
                        }
                        Err(ProtocolError::UnexpectedEof) => {
                            log::info!("Daemon: proxy {conn} disconnected (EOF)");
                            break;
                        }
                        Err(e) if e.is_read_recoverable() => {
                            log::warn!("Daemon: skipping malformed message from conn {conn}: {e}");
                        }
                        Err(e) => {
                            if is_disconnect_error(&e) {
                                log::warn!(
                                    "Daemon: read error from conn {conn} (client disconnected): {e}"
                                );
                            } else {
                                log::error!("Daemon: fatal read error from conn {conn}: {e}");
                            }
                            break;
                        }
                    }
                }
            });
            let _ = reader_events.send(Event::Disconnected { conn });
            // Closing the channel ends the writer once it has drained, even while a worker
            // thread still holds a sender for a request of this connection.
            conn_tx.close();
        });
    if let Err(e) = spawned {
        log::error!("Daemon: could not start reader for conn {conn}: {e}");
        let _ = events.send(Event::Disconnected { conn });
        return;
    }

    let spawned = std::thread::Builder::new()
        .name(format!("conn-{conn}-write"))
        .spawn(move || {
            async_io::block_on(async move {
                // #1244: two lanes. Every queued message is encoded and classified by
                // `OutboundScheduler`: small frames go first, in order; a large one goes out
                // one chunk at a time (one whole frame for a client that cannot reassemble), so
                // a small response queued behind a multi-MB one waits for at most one chunk.
                let mut scheduler = OutboundScheduler::new(CHUNK_PAYLOAD_BYTES);
                'writer: loop {
                    if scheduler.is_empty() {
                        match conn_rx.recv().await {
                            Ok(msg) => enqueue_outbound(&mut scheduler, msg, &chunking, conn),
                            Err(_) => break,
                        }
                    }
                    while let Ok(msg) = conn_rx.try_recv() {
                        enqueue_outbound(&mut scheduler, msg, &chunking, conn);
                    }
                    let Some(frame) = scheduler.next_frame() else {
                        continue;
                    };
                    // `write_encoded_message` flushes after each frame.
                    if let Err(e) = protocol::write_encoded_message(&mut write_half, &frame).await {
                        if is_disconnect_protocol_error(&e) {
                            log::warn!(
                                "Daemon: write error on conn {conn} (client disconnected): {e}"
                            );
                        } else {
                            log::error!("Daemon: write error on conn {conn}: {e}");
                        }
                        break 'writer;
                    }
                }
                let _ = write_half.flush().await;
            });
        });
    if let Err(e) = spawned {
        log::error!("Daemon: could not start writer for conn {conn}: {e}");
    }
}

/// Queues `msg` on the connection's lanes. A message that cannot be sent at all (over the
/// wire-level cap, `MessageTooLarge`) is replaced by an `ErrorResponse` so the client does not
/// hang waiting for it; nothing was written, so the stream stays aligned (upstream d9dee18e1e,
/// #10681).
fn enqueue_outbound(
    scheduler: &mut OutboundScheduler,
    msg: ServerMessage,
    chunking: &AtomicBool,
    conn: ConnId,
) {
    let chunking = chunking.load(std::sync::atomic::Ordering::Acquire);
    let Err(e) = scheduler.enqueue(&msg, chunking) else {
        return;
    };
    log::warn!("Daemon: skipping undeliverable message on conn {conn}: {e}");
    if msg.request_id.is_empty() {
        return;
    }
    let error_msg = ServerMessage {
        request_id: msg.request_id,
        message: Some(server_message::Message::Error(ErrorResponse {
            code: ErrorCode::Internal.into(),
            message: format!("Response could not be delivered: {e}"),
        })),
    };
    if let Err(e2) = scheduler.enqueue(&error_msg, chunking) {
        log::warn!("Daemon: failed to queue error response on conn {conn}: {e2}");
    }
}

/// Returns `true` if the IO error represents a normal client disconnect: a broken pipe or a
/// reset is outside the daemon's control and happens on every ordinary "user closed the SSH
/// session" teardown, so it must not log at `error!`.
fn is_disconnect_io_error(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::BrokenPipe
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionAborted
    )
}

/// Returns `true` if the `ProtocolError` wraps a disconnect IO error.
fn is_disconnect_error(e: &ProtocolError) -> bool {
    match e {
        ProtocolError::Io(io_err) => is_disconnect_io_error(io_err),
        _ => false,
    }
}

/// Alias for [`is_disconnect_error`] used in the write path for clarity.
fn is_disconnect_protocol_error(e: &ProtocolError) -> bool {
    is_disconnect_error(e)
}

#[cfg(test)]
#[path = "conn_tests.rs"]
mod tests;
