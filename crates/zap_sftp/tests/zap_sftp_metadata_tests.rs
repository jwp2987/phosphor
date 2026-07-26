//! Unit tests for the zap_sftp::types::Metadata::from_ssh2 module
//!
//! Verifies the logic for creating Metadata from ssh2::FileStat, focusing on
//! the symlink-detection fix and each field's Some/None fallback.
//! author: logic
//! date: 2026-05-27

use std::time::{Duration, SystemTime};

use zap_sftp::types::*;

/// Builds an empty ssh2::FileStat with all fields set to None
fn empty_stat() -> ssh2::FileStat {
    ssh2::FileStat {
        size: None,
        uid: None,
        gid: None,
        perm: None,
        atime: None,
        mtime: None,
    }
}

// ============================================================
// Metadata::from_ssh2 — file type detection
// ============================================================

/// Verifies file_type is Dir when perm contains the directory mode bits
#[test]
fn test_metadata_from_ssh2_dir() {
    let stat = ssh2::FileStat {
        perm: Some(0o040755),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::Dir);
}

/// Verifies file_type is File when perm contains regular-file mode bits
#[test]
fn test_metadata_from_ssh2_file() {
    let stat = ssh2::FileStat {
        perm: Some(0o100644),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::File);
}

/// Verifies file_type is Symlink when perm contains symlink mode bits (fix verification)
#[test]
fn test_metadata_from_ssh2_symlink() {
    let stat = ssh2::FileStat {
        perm: Some(0o120755),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::Symlink);
}

/// Verifies file_type falls back to Other when perm is None
#[test]
fn test_metadata_from_ssh2_perm_none() {
    let stat = ssh2::FileStat {
        perm: None,
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::Other);
}

/// Verifies file_type is Other for an unknown mode-bit combination
#[test]
fn test_metadata_from_ssh2_unknown_mode() {
    let stat = ssh2::FileStat {
        perm: Some(0o050000),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::Other);
}

// ============================================================
// Metadata::from_ssh2 — permission fields
// ============================================================

/// Verifies permission bits are parsed correctly
#[test]
fn test_metadata_from_ssh2_permissions() {
    let stat = ssh2::FileStat {
        perm: Some(0o100755),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert!(meta.permissions.owner_read);
    assert!(meta.permissions.owner_write);
    assert!(meta.permissions.owner_exec);
    assert!(meta.permissions.group_read);
    assert!(!meta.permissions.group_write);
    assert!(meta.permissions.group_exec);
}

/// Verifies all permissions are false when perm is None
#[test]
fn test_metadata_from_ssh2_permissions_none() {
    let stat = ssh2::FileStat {
        perm: None,
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert!(!meta.permissions.owner_read);
    assert!(!meta.permissions.owner_write);
}

// ============================================================
// Metadata::from_ssh2 — numeric field fallbacks
// ============================================================

/// Verifies normal size/uid/gid values
#[test]
fn test_metadata_from_ssh2_fields_present() {
    let stat = ssh2::FileStat {
        perm: Some(0o100644),
        size: Some(4096),
        uid: Some(1000),
        gid: Some(100),
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.size, 4096);
    assert_eq!(meta.uid, 1000);
    assert_eq!(meta.gid, 100);
}

/// Verifies size/uid/gid fall back to 0 when None
#[test]
fn test_metadata_from_ssh2_fields_absent() {
    let stat = ssh2::FileStat {
        perm: Some(0o100644),
        size: None,
        uid: None,
        gid: None,
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.size, 0);
    assert_eq!(meta.uid, 0);
    assert_eq!(meta.gid, 0);
}

// ============================================================
// Metadata::from_ssh2 — timestamps
// ============================================================

/// Verifies atime/mtime are correctly converted to SystemTime
#[test]
fn test_metadata_from_ssh2_timestamps_present() {
    let stat = ssh2::FileStat {
        perm: Some(0o100644),
        atime: Some(1609459200), // 2021-01-01 00:00:00 UTC
        mtime: Some(1609545600), // 2021-01-02 00:00:00 UTC
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    let expected_atime = SystemTime::UNIX_EPOCH + Duration::from_secs(1609459200);
    let expected_mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1609545600);
    assert_eq!(meta.accessed, Some(expected_atime));
    assert_eq!(meta.modified, Some(expected_mtime));
}

/// Verifies accessed/modified are None when atime/mtime are None
#[test]
fn test_metadata_from_ssh2_timestamps_absent() {
    let stat = ssh2::FileStat {
        perm: Some(0o100644),
        atime: None,
        mtime: None,
        ..empty_stat()
    };
    let meta = Metadata::from_ssh2(stat);
    assert!(meta.accessed.is_none());
    assert!(meta.modified.is_none());
}

// ============================================================
// Metadata::from_ssh2 — full field combination
// ============================================================

/// Verifies the full scenario with all fields set at once
#[test]
fn test_metadata_from_ssh2_full_stat() {
    let stat = ssh2::FileStat {
        perm: Some(0o120777), // symlink + 777
        size: Some(11),
        uid: Some(501),
        gid: Some(20),
        atime: Some(1000000),
        mtime: Some(2000000),
    };
    let meta = Metadata::from_ssh2(stat);
    assert_eq!(meta.file_type, FileType::Symlink);
    assert_eq!(meta.size, 11);
    assert_eq!(meta.uid, 501);
    assert_eq!(meta.gid, 20);
    assert!(meta.permissions.other_exec);
    assert!(meta.accessed.is_some());
    assert!(meta.modified.is_some());
}
