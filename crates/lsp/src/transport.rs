use std::sync::Arc;

use async_process::{Child, ChildStdin, ChildStdout, Stdio};
use async_trait::async_trait;
use command::r#async::Command;
use futures::future::FutureExt;
use futures::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader, BufWriter};
use futures::lock::Mutex;
use jsonrpc::Transport;
use simple_logger::SimpleLogger;
use warp_core::report_error;
use warpui_core::r#async::Timer;
use warpui_core::r#async::executor::{Background, BackgroundTask};

/// Give-up bound for a server to exit after SIGTERM before it is killed.
#[cfg(unix)]
const SIGTERM_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Sends SIGTERM and returns as soon as the child has exited, which is the signal the old fixed
/// 2 s sleep was standing in for (#1364). `grace` is only a give-up for a server that ignores
/// SIGTERM; when it fires it is logged and the caller kills the process. Returns whether the
/// child exited on its own.
#[cfg(unix)]
async fn terminate_and_reap(child: &mut Child, grace: std::time::Duration) -> bool {
    use nix::sys::signal::{Signal, kill};
    use nix::unistd::Pid;
    if kill(Pid::from_raw(child.id() as i32), Signal::SIGTERM).is_err() {
        return false;
    }
    futures::select! {
        _ = child.status().fuse() => true,
        _ = Timer::after(grace).fuse() => {
            log::warn!("LSP: server did not exit within {grace:?} of SIGTERM; killing it.");
            false
        }
    }
}

/// Transport implementation for LSP communication over process stdin/stdout.
/// Also manages the LSP server process lifecycle with graceful shutdown capabilities.
#[derive(Clone)]
pub struct ProcessTransport {
    input: Arc<Mutex<BufReader<ChildStdout>>>,
    output: Arc<Mutex<BufWriter<ChildStdin>>>,
    child: Arc<Mutex<Option<Child>>>,
    stderr_task: Arc<Mutex<Option<BackgroundTask>>>,
}

impl ProcessTransport {
    /// Creates a new ProcessTransport.
    ///
    /// If `logger` is provided, stderr output will be written to that logger's file
    /// in addition to being logged via `log::debug!`.
    pub fn new(
        mut command: Command,
        executor: Arc<Background>,
        logger: Option<SimpleLogger>,
    ) -> anyhow::Result<Self> {
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = command
            .spawn()
            .map_err(|e| anyhow::anyhow!("Failed to spawn process: {}", e))?;

        let child_pid = child.id();
        log::info!("ProcessTransport: Spawned process with pid {child_pid}");

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get child stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get child stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| anyhow::anyhow!("Failed to get child stderr"))?;

        // stderr -> logger background task
        let stderr_task = executor.spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut buffer = String::new();
            loop {
                buffer.clear();
                match reader.read_line(&mut buffer).await {
                    Ok(0) => break, // EOF
                    Ok(_) => {
                        let message = buffer.trim_end();
                        // Log to file if logger is available
                        if let Some(ref logger) = logger {
                            logger.log(format!("[stderr] {message}"));
                        }
                        // Also log via standard logging at debug level
                        log::debug!("ProcessTransport [pid: {child_pid}] stderr: {message}");
                    }
                    Err(e) => {
                        if let Some(ref logger) = logger {
                            logger.log(format!("[error] Error reading stderr: {e}"));
                        }
                        report_error!(
                            anyhow::Error::new(e)
                                .context("ProcessTransport: Error reading stderr"),
                            extra: { "pid" => %child_pid }
                        );
                        break;
                    }
                }
            }
        });

        Ok(Self {
            input: Arc::new(Mutex::new(BufReader::new(stdout))),
            output: Arc::new(Mutex::new(BufWriter::new(stdin))),
            child: Arc::new(Mutex::new(Some(child))),
            stderr_task: Arc::new(Mutex::new(Some(stderr_task))),
        })
    }
}

#[async_trait]
impl Transport for ProcessTransport {
    async fn read(&self) -> anyhow::Result<String> {
        let mut content_length: Option<usize> = None;
        loop {
            let mut header_line = String::new();
            let bytes_read = {
                let mut reader = self.input.lock().await;
                reader.read_line(&mut header_line).await?
            };
            if bytes_read == 0 {
                return Ok("".to_string());
            }

            let header_line = header_line.trim_end();
            if header_line.is_empty() {
                break;
            }

            if let Some(value) = header_line.strip_prefix("Content-Length:") {
                content_length = Some(value.trim().parse()?);
            }
        }

        let length =
            content_length.ok_or_else(|| anyhow::anyhow!("Missing Content-Length header"))?;

        let mut buffer = vec![0u8; length];
        {
            let mut reader = self.input.lock().await;
            reader.read_exact(&mut buffer).await?;
        }

        let result = String::from_utf8(buffer)?;
        Ok(result)
    }

    async fn write(&self, message: &str) -> anyhow::Result<()> {
        let header = format!("Content-Length: {}\r\n\r\n", message.len());
        {
            let mut writer = self.output.lock().await;
            writer.write_all(header.as_bytes()).await?;
            writer.write_all(message.as_bytes()).await?;
            writer.flush().await?;
        }
        Ok(())
    }

    async fn shutdown(&self, timeout: std::time::Duration) -> anyhow::Result<()> {
        log::info!("LSP: Shutting down server.");

        let child = {
            let mut child_guard = self.child.lock().await;
            match child_guard.take() {
                Some(c) => c,
                None => {
                    log::warn!("LSP: Server already shut down.");
                    return Ok(());
                }
            }
        };

        let mut child = child;
        let shutdown = child.status();
        let timeout_future = Timer::after(timeout);
        futures::select! {
            _ = shutdown.fuse() => {},
            _ = timeout_future.fuse() => {
                // On *nix platforms, send a SIGTERM and wait for the process's own exit
                // (bounded by a logged give-up) before killing it.
                #[cfg(unix)]
                terminate_and_reap(&mut child, SIGTERM_TIMEOUT).await;

                let _ = child.kill();
            }
        }

        // Wait for the stderr task because it owns the last logger clone.
        // Joining it ensures that clone is dropped before restart so the same
        // log path can be registered again without colliding with a stale entry.
        if let Some(stderr_task) = self.stderr_task.lock().await.take()
            && let Err(e) = stderr_task.await
        {
            log::warn!("LSP: Failed to join stderr task: {e}");
        }
        log::info!("LSP: Server shut down.");
        Ok(())
    }
}

#[cfg(all(test, unix))]
#[path = "transport_tests.rs"]
mod tests;
