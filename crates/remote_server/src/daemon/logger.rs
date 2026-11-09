//! A tiny file logger for the helper (no `env_logger`, no `tracing`): appends to
//! `{daemon dir}/helper.log`, which is truncated when it passes 1 MiB at startup. The daemon's
//! stderr is /dev/null, so without this its `log::` calls would go nowhere.
//!
//! Level from `PHOSPHOR_REMOTE_LOG` (`error|warn|info|debug|trace|off`, default `info`).

use std::io::Write as _;
use std::path::Path;
use std::sync::Mutex;

use log::{LevelFilter, Log, Metadata, Record};

const MAX_LOG_BYTES: u64 = 1024 * 1024;

struct FileLogger {
    file: Mutex<std::fs::File>,
}

impl Log for FileLogger {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.level() <= log::max_level()
    }

    fn log(&self, record: &Record<'_>) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if let Ok(mut file) = self.file.lock() {
            let _ = writeln!(
                file,
                "{secs} {:<5} [{}] {}",
                record.level(),
                std::process::id(),
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

/// Parses `PHOSPHOR_REMOTE_LOG`; anything unrecognised is the default.
pub fn level_from_env(value: Option<&str>) -> LevelFilter {
    match value.map(|v| v.trim().to_ascii_lowercase()).as_deref() {
        Some("off") => LevelFilter::Off,
        Some("error") => LevelFilter::Error,
        Some("warn") => LevelFilter::Warn,
        Some("debug") => LevelFilter::Debug,
        Some("trace") => LevelFilter::Trace,
        _ => LevelFilter::Info,
    }
}

/// Installs the file logger at `path`. Failure to open the file is not an error: the helper
/// runs without logging rather than not at all.
pub fn init(path: &Path) {
    let level = level_from_env(std::env::var("PHOSPHOR_REMOTE_LOG").ok().as_deref());
    if level == LevelFilter::Off {
        return;
    }
    if std::fs::metadata(path).is_ok_and(|m| m.len() > MAX_LOG_BYTES) {
        let _ = std::fs::remove_file(path);
    }
    let Ok(file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    else {
        return;
    };
    let logger = Box::new(FileLogger {
        file: Mutex::new(file),
    });
    if log::set_boxed_logger(logger).is_ok() {
        log::set_max_level(level);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_parse_and_default_to_info() {
        assert_eq!(level_from_env(None), LevelFilter::Info);
        assert_eq!(level_from_env(Some("DEBUG")), LevelFilter::Debug);
        assert_eq!(level_from_env(Some(" off ")), LevelFilter::Off);
        assert_eq!(level_from_env(Some("loud")), LevelFilter::Info);
    }
}
