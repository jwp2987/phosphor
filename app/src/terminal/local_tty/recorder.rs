use crate::{auth::AuthState, send_telemetry_on_executor, server::telemetry::TelemetryEvent};
use async_broadcast::Receiver;
use instant::{Duration, Instant};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use warpui::r#async::executor::Background;

/// We want to measure throughput as bytes / sec.
const PTY_THROUGHPUT_TIME_INTERVAL: Duration = Duration::from_secs(1);

/// We don't want to actually emit a metric every second because that
/// will be wasteful. So we aggregate over 10 minute periods.
const PTY_THROUGHPUT_METRIC_INTERVAL: Duration = Duration::from_secs(10);

/// Max bytes/sec tracker. Time is bucketed by the arrival of reads themselves, so there is no
/// per-tab timer: a bucket is closed by the first read that arrives after
/// [`PTY_THROUGHPUT_TIME_INTERVAL`] (idle gaps contribute nothing, which is what the old 1 s
/// interval computed too, since empty buckets never raise the max).
struct ThroughputTracker {
    bucket_start: Instant,
    bytes_in_bucket: usize,
    max_throughput: usize,
    last_emitted: Instant,
}

impl ThroughputTracker {
    fn new(now: Instant) -> Self {
        Self {
            bucket_start: now,
            bytes_in_bucket: 0,
            max_throughput: 0,
            last_emitted: now,
        }
    }

    /// Accounts for `bytes` read at `now`. Returns the max bytes/sec to emit when a metric
    /// interval has elapsed and the max is non-zero.
    fn record(&mut self, now: Instant, bytes: usize) -> Option<usize> {
        let mut emit = None;
        if now.duration_since(self.bucket_start) >= PTY_THROUGHPUT_TIME_INTERVAL {
            self.max_throughput = self.max_throughput.max(self.bytes_in_bucket);
            self.bytes_in_bucket = 0;
            self.bucket_start = now;
            if now.duration_since(self.last_emitted) >= PTY_THROUGHPUT_METRIC_INTERVAL {
                if self.max_throughput > 0 {
                    emit = Some(self.max_throughput);
                }
                self.max_throughput = 0;
                self.last_emitted = now;
            }
        }
        self.bytes_in_bucket += bytes;
        emit
    }
}

/// Records the max PTY bytes/sec throughput observed over every metric period.
///
/// `countable` is published by the terminal model while it processes each read (it is already
/// under its own lock there): true when the bytes are neither in-band command output nor
/// pre-bootstrap. The recorder never touches the model lock (#1377).
pub fn record_pty_throughput(
    mut pty_reads_rx: Receiver<Arc<Vec<u8>>>,
    countable: Arc<AtomicBool>,
    auth_state: Arc<AuthState>,
    executor: Arc<Background>,
) {
    let executor_clone = executor.clone();
    executor
        .spawn(async move {
            let mut tracker = ThroughputTracker::new(Instant::now());
            // Ends when the session closes the channel.
            while let Ok(bytes) = pty_reads_rx.recv().await {
                // Don't care about pre-bootstrap or in-band-command bytes for this calculation.
                let n = if countable.load(Ordering::Relaxed) {
                    bytes.len()
                } else {
                    0
                };
                if let Some(max_bytes_per_second) = tracker.record(Instant::now(), n) {
                    send_telemetry_on_executor!(
                        auth_state,
                        TelemetryEvent::PtyThroughput {
                            max_bytes_per_second,
                        },
                        executor_clone
                    );
                }
            }
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_close_on_reads_and_emit_max() {
        let t0 = Instant::now();
        let mut t = ThroughputTracker::new(t0);
        assert_eq!(t.record(t0, 100), None);
        assert_eq!(t.record(t0 + Duration::from_millis(500), 50), None);
        // Closes bucket 1 (150 bytes).
        assert_eq!(t.record(t0 + Duration::from_secs(2), 10), None);
        assert_eq!(t.max_throughput, 150);
        // Metric interval elapsed: emits the max, resets.
        assert_eq!(t.record(t0 + Duration::from_secs(12), 1), Some(150));
    }

    #[test]
    fn nothing_emitted_when_nothing_counted() {
        let t0 = Instant::now();
        let mut t = ThroughputTracker::new(t0);
        assert_eq!(t.record(t0 + Duration::from_secs(20), 0), None);
    }
}
