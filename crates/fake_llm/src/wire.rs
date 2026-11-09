//! The wire gaps llmposter leaves (L0 and, later, L2): a scripted raw HTTP/SSE server and a
//! byte-level chaos proxy.
//!
//! * [`WireServer`] answers each connection with a scripted response made of [`Piece`]s. Every
//!   piece is written as its own HTTP chunk and flushed, so a test controls what arrives
//!   together: tool-call arguments streamed as many deltas, a frame cut between `data:` and
//!   `\n\n`, a `Disconnect` that leaves the chunked body unterminated (which a client must report
//!   as an error, not as a short reply).
//! * [`ChaosProxy`] sits in front of any upstream (llmposter, or the scripted server) and
//!   re-cuts the response byte stream at arbitrary offsets, including inside a UTF-8 sequence or a
//!   JSON string.
//!
//! Plain tokio sockets, no HTTP crate: the point is to put bytes on the wire exactly as scripted.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::json;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;

use crate::scenario::Split;

/// One step of a scripted response body.
#[derive(Debug, Clone)]
pub enum Piece {
    /// Bytes written as one HTTP chunk.
    Bytes(Vec<u8>),
    Delay(Duration),
    /// Drop the connection here without the chunked terminator.
    Disconnect,
    /// Go silent: keep the connection open and send nothing more.
    Stall,
}

#[derive(Debug, Clone)]
pub struct WireResponse {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<Piece>,
}

impl WireResponse {
    pub fn sse(body: Vec<Piece>) -> Self {
        Self {
            status: 200,
            content_type: "text/event-stream",
            body,
        }
    }

    pub fn error(status: u16, content_type: &'static str, body: &str) -> Self {
        Self {
            status,
            content_type,
            body: vec![Piece::Bytes(body.as_bytes().to_vec())],
        }
    }
}

/// A raw server: the nth accepted request gets the nth response (the last one repeats).
pub struct WireServer {
    addr: SocketAddr,
    requests: Arc<Mutex<Vec<String>>>,
    task: JoinHandle<()>,
}

impl WireServer {
    pub async fn start(responses: Vec<WireResponse>) -> Self {
        assert!(
            !responses.is_empty(),
            "a WireServer needs at least one response"
        );
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let seen = requests.clone();
        let task = tokio::spawn(async move {
            let mut n = 0usize;
            loop {
                let Ok((socket, _)) = listener.accept().await else {
                    return;
                };
                let _ = socket.set_nodelay(true);
                let response = responses[n.min(responses.len() - 1)].clone();
                n += 1;
                let seen = seen.clone();
                tokio::spawn(async move {
                    let _ = serve_connection(socket, response, seen).await;
                });
            }
        });
        Self {
            addr,
            requests,
            task,
        }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The `base_url` for a provider: `http://127.0.0.1:<port>/v1`.
    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }

    /// The request bodies received so far, in order.
    pub fn request_bodies(&self) -> Vec<String> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for WireServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn read_request(socket: &mut TcpStream) -> std::io::Result<Option<String>> {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 4096];
    let header_end = loop {
        if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break pos + 4;
        }
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            return Ok(None);
        }
        buf.extend_from_slice(&tmp[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..header_end]).to_ascii_lowercase();
    let len = head
        .lines()
        .find_map(|l| l.strip_prefix("content-length:"))
        .and_then(|v| v.trim().parse::<usize>().ok())
        .unwrap_or(0);
    while buf.len() < header_end + len {
        let n = socket.read(&mut tmp).await?;
        if n == 0 {
            break;
        }
        buf.extend_from_slice(&tmp[..n]);
    }
    Ok(Some(
        String::from_utf8_lossy(&buf[header_end..]).into_owned(),
    ))
}

async fn serve_connection(
    mut socket: TcpStream,
    response: WireResponse,
    seen: Arc<Mutex<Vec<String>>>,
) -> std::io::Result<()> {
    let Some(body) = read_request(&mut socket).await? else {
        return Ok(());
    };
    seen.lock().unwrap().push(body);
    let head = format!(
        "HTTP/1.1 {} X\r\ncontent-type: {}\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n",
        response.status, response.content_type
    );
    socket.write_all(head.as_bytes()).await?;
    socket.flush().await?;
    for piece in response.body {
        match piece {
            Piece::Bytes(bytes) if bytes.is_empty() => {}
            Piece::Bytes(bytes) => {
                socket
                    .write_all(format!("{:x}\r\n", bytes.len()).as_bytes())
                    .await?;
                socket.write_all(&bytes).await?;
                socket.write_all(b"\r\n").await?;
                socket.flush().await?;
                // Let the reader see this chunk on its own.
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
            Piece::Delay(d) => tokio::time::sleep(d).await,
            Piece::Disconnect => return Ok(()),
            Piece::Stall => {
                tokio::time::sleep(Duration::from_secs(3600)).await;
                return Ok(());
            }
        }
    }
    socket.write_all(b"0\r\n\r\n").await?;
    socket.flush().await
}

// ---- SSE builders -----------------------------------------------------------------------

/// Cuts `s` into at most `parts` non-empty pieces on char boundaries.
pub fn split_str(s: &str, parts: usize) -> Vec<String> {
    let chars: Vec<char> = s.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }
    let parts = parts.clamp(1, chars.len());
    let len = chars.len().div_ceil(parts);
    chars.chunks(len).map(|c| c.iter().collect()).collect()
}

fn frame(delta: serde_json::Value, finish: Option<&str>) -> Vec<u8> {
    let payload = json!({
        "id": "chatcmpl-fake", "object": "chat.completion.chunk", "created": 0,
        "model": "fake-model",
        "choices": [{"index": 0, "delta": delta, "finish_reason": finish}],
    });
    format!("data: {payload}\n\n").into_bytes()
}

/// A tool call as the provider streams it: `args` arrives as `arg_parts` deltas.
#[derive(Debug, Clone)]
pub struct WireCall {
    pub id: String,
    pub name: String,
    /// The raw argument text, which may be deliberately invalid JSON.
    pub args: String,
    pub arg_parts: usize,
}

/// The OpenAI chat-completions SSE frames for a reply, one `Vec<u8>` per frame (terminal
/// `[DONE]` last unless `terminate` is false).
pub fn openai_frames(text: &str, calls: &[WireCall], terminate: bool) -> Vec<Vec<u8>> {
    let mut frames = vec![frame(json!({"role": "assistant"}), None)];
    for piece in split_str(text, 3) {
        frames.push(frame(json!({"content": piece}), None));
    }
    for (index, call) in calls.iter().enumerate() {
        frames.push(frame(
            json!({"tool_calls": [{"index": index, "id": call.id, "type": "function",
                "function": {"name": call.name, "arguments": ""}}]}),
            None,
        ));
        for piece in split_str(&call.args, call.arg_parts) {
            frames.push(frame(
                json!({"tool_calls": [{"index": index, "function": {"arguments": piece}}]}),
                None,
            ));
        }
    }
    let finish = if calls.is_empty() {
        "stop"
    } else {
        "tool_calls"
    };
    frames.push(frame(json!({}), Some(finish)));
    if terminate {
        frames.push(b"data: [DONE]\n\n".to_vec());
    }
    frames
}

/// Re-cuts a byte stream per `split`. The pieces concatenate back to `bytes` exactly.
pub fn cut(bytes: &[u8], split: &Split) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut at = 0usize;
    let mut i = 0usize;
    while at < bytes.len() {
        let step = match split {
            Split::Bytes { n } => (*n).max(1),
            Split::Offsets { cuts } if cuts.is_empty() => bytes.len(),
            Split::Offsets { cuts } => cuts[i % cuts.len()].max(1),
        };
        let end = (at + step).min(bytes.len());
        out.push(bytes[at..end].to_vec());
        at = end;
        i += 1;
    }
    out
}

// ---- the chaos proxy --------------------------------------------------------------------

/// Forwards each connection to `upstream`, re-cutting every upstream read per `split` and
/// pausing `delay` between pieces. Requests pass through untouched.
pub struct ChaosProxy {
    addr: SocketAddr,
    task: JoinHandle<()>,
}

impl ChaosProxy {
    pub async fn start(upstream: SocketAddr, split: Split, delay: Duration) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind loopback");
        let addr = listener.local_addr().expect("local addr");
        let task = tokio::spawn(async move {
            loop {
                let Ok((client, _)) = listener.accept().await else {
                    return;
                };
                let split = split.clone();
                tokio::spawn(async move {
                    let _ = proxy_connection(client, upstream, split, delay).await;
                });
            }
        });
        Self { addr, task }
    }

    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    pub fn base_url(&self) -> String {
        format!("http://{}/v1", self.addr)
    }
}

impl Drop for ChaosProxy {
    fn drop(&mut self) {
        self.task.abort();
    }
}

async fn proxy_connection(
    client: TcpStream,
    upstream: SocketAddr,
    split: Split,
    delay: Duration,
) -> std::io::Result<()> {
    let server = TcpStream::connect(upstream).await?;
    let _ = client.set_nodelay(true);
    let _ = server.set_nodelay(true);
    let (mut client_r, mut client_w) = client.into_split();
    let (mut server_r, mut server_w) = server.into_split();
    let up = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut client_r, &mut server_w).await;
        let _ = server_w.shutdown().await;
    });
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let n = server_r.read(&mut buf).await?;
        if n == 0 {
            break;
        }
        for piece in cut(&buf[..n], &split) {
            client_w.write_all(&piece).await?;
            client_w.flush().await?;
            if delay.is_zero() {
                tokio::task::yield_now().await;
            } else {
                tokio::time::sleep(delay).await;
            }
        }
    }
    let _ = client_w.shutdown().await;
    up.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cut_concatenates_back() {
        let data: Vec<u8> = (0..=255u8).collect();
        for split in [
            Split::Bytes { n: 1 },
            Split::Bytes { n: 7 },
            Split::Offsets {
                cuts: vec![3, 1, 50],
            },
            Split::Offsets { cuts: vec![] },
        ] {
            let pieces = cut(&data, &split);
            assert_eq!(pieces.concat(), data);
        }
    }

    #[test]
    fn split_str_keeps_every_char() {
        for parts in 1..9 {
            assert_eq!(split_str("héllo wörld ✓", parts).concat(), "héllo wörld ✓");
        }
    }

    #[test]
    fn frames_end_with_done_only_when_terminated() {
        assert_eq!(
            openai_frames("hi", &[], true).last().unwrap(),
            b"data: [DONE]\n\n"
        );
        assert_ne!(
            openai_frames("hi", &[], false).last().unwrap(),
            b"data: [DONE]\n\n"
        );
    }
}
