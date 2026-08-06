use std::path::PathBuf;

use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};

use super::*;

/// A scratch subkey under HKCU that tests create/destroy to avoid touching
/// the real `Software\Microsoft\Windows\CurrentVersion\Run` hive.
struct ScratchSubkey {
    path: String,
}

impl ScratchSubkey {
    fn new(name: &str) -> Self {
        let suffix = format!(
            "{}_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            name,
        );
        let path = format!(r"Software\Phosphor\LoginItemTests\{suffix}");
        RegKey::predef(HKEY_CURRENT_USER)
            .create_subkey(&path)
            .expect("create scratch subkey");
        Self { path }
    }

    fn read(&self, value_name: &str) -> Option<String> {
        let key = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(&self.path, KEY_READ)
            .ok()?;
        key.get_value::<String, _>(value_name).ok()
    }
}

impl Drop for ScratchSubkey {
    fn drop(&mut self) {
        let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(&self.path);
    }
}

#[test]
fn register_writes_quoted_path() {
    let scratch = ScratchSubkey::new("register_writes_quoted_path");
    let exe = PathBuf::from(r"C:\Program Files\TestApp\app.exe");
    register_in(HKEY_CURRENT_USER, &scratch.path, "TestApp", &exe).unwrap();
    assert_eq!(
        scratch.read("TestApp").as_deref(),
        Some(r#""C:\Program Files\TestApp\app.exe""#)
    );
}

#[test]
fn register_overwrites_previous_path() {
    let scratch = ScratchSubkey::new("register_overwrites_previous_path");
    register_in(
        HKEY_CURRENT_USER,
        &scratch.path,
        "TestApp",
        &PathBuf::from(r"C:\old\app.exe"),
    )
    .unwrap();
    register_in(
        HKEY_CURRENT_USER,
        &scratch.path,
        "TestApp",
        &PathBuf::from(r"C:\new\app.exe"),
    )
    .unwrap();
    assert_eq!(
        scratch.read("TestApp").as_deref(),
        Some(r#""C:\new\app.exe""#)
    );
}

#[test]
fn unregister_is_idempotent() {
    let scratch = ScratchSubkey::new("unregister_is_idempotent");
    // Never registered: unregister should be Ok.
    unregister_in(HKEY_CURRENT_USER, &scratch.path, "TestApp").unwrap();
    // Register, then unregister twice.
    register_in(
        HKEY_CURRENT_USER,
        &scratch.path,
        "TestApp",
        &PathBuf::from(r"C:\app.exe"),
    )
    .unwrap();
    unregister_in(HKEY_CURRENT_USER, &scratch.path, "TestApp").unwrap();
    unregister_in(HKEY_CURRENT_USER, &scratch.path, "TestApp").unwrap();
    assert!(scratch.read("TestApp").is_none());
}

#[test]
fn unregister_leaves_other_values_alone() {
    let scratch = ScratchSubkey::new("unregister_leaves_other_values_alone");
    register_in(
        HKEY_CURRENT_USER,
        &scratch.path,
        "TestApp",
        &PathBuf::from(r"C:\app.exe"),
    )
    .unwrap();
    register_in(
        HKEY_CURRENT_USER,
        &scratch.path,
        "TestAppPreview",
        &PathBuf::from(r"C:\app-preview.exe"),
    )
    .unwrap();

    unregister_in(HKEY_CURRENT_USER, &scratch.path, "TestApp").unwrap();

    assert!(scratch.read("TestApp").is_none());
    assert_eq!(
        scratch.read("TestAppPreview").as_deref(),
        Some(r#""C:\app-preview.exe""#)
    );
}

#[test]
fn unregister_missing_subkey_is_ok() {
    unregister_in(
        HKEY_CURRENT_USER,
        r"Software\Phosphor\LoginItemTests\does-not-exist",
        "TestApp",
    )
    .unwrap();
}
