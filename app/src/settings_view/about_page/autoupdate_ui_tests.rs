use super::*;
use crate::autoupdate::DownloadProgress;

#[test]
fn format_bytes_uses_bytes_below_one_kb() {
    assert_eq!(format_bytes(0), "0 B");
    assert_eq!(format_bytes(1023), "1023 B");
}

#[test]
fn format_bytes_uses_kb_below_one_mb() {
    assert_eq!(format_bytes(1024), "1 KB");
    assert_eq!(format_bytes(1024 * 1024 - 1), "1024 KB");
}

#[test]
fn format_bytes_uses_mb_at_and_above_one_mb() {
    assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
    assert_eq!(format_bytes(3 * 1024 * 1024 + 512 * 1024), "3.5 MB");
}

#[test]
fn format_download_progress_shows_only_downloaded_when_total_unknown() {
    let progress = DownloadProgress {
        downloaded: 512 * 1024,
        total: None,
    };
    assert_eq!(format_download_progress(&progress), "512 KB");
}

#[test]
fn format_download_progress_shows_downloaded_of_total_with_percent() {
    let progress = DownloadProgress {
        downloaded: 1024 * 1024,
        total: Some(2 * 1024 * 1024),
    };
    assert_eq!(format_download_progress(&progress), "1.0 MB / 2.0 MB (50%)");
}

#[test]
fn format_download_progress_treats_zero_total_as_unknown() {
    let progress = DownloadProgress {
        downloaded: 1024,
        total: Some(0),
    };
    assert_eq!(format_download_progress(&progress), "1 KB");
}
