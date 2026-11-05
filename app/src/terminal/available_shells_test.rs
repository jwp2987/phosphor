use super::*;
use crate::terminal::shell::ShellType;
use crate::test_util::{Stub, VirtualFS};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use warp_core::features::FeatureFlag;

fn make_available_shells(shells: Vec<AvailableShell>) -> AvailableShells {
    AvailableShells {
        result: Some(ScanResult {
            shells,
            shell_counts: HashMap::new(),
        }),
        empty: ScanResult::default(),
        scan_generation: 1,
        scanned_path: std::ffi::OsString::new(),
        wsl_names: Vec::new(),
    }
}

fn unscanned_available_shells() -> AvailableShells {
    AvailableShells {
        result: None,
        empty: ScanResult::default(),
        scan_generation: 1,
        scanned_path: std::ffi::OsString::new(),
        wsl_names: Vec::new(),
    }
}

fn bash_scan() -> ScanResult {
    ScanResult {
        shells: vec![AvailableShell::new_local_executable(
            "bash".to_string(),
            PathBuf::from("/bin/bash"),
            ShellType::Bash,
        )],
        shell_counts: HashMap::new(),
    }
}

#[test]
fn unfinished_scan_never_waits_and_completion_is_announced_by_event() {
    warpui::App::test((), |mut app| async move {
        let model = app.add_model(|_| unscanned_available_shells());
        let events = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        app.update(|ctx| {
            let events = events.clone();
            ctx.subscribe_to_model(&model, move |_, event, _| {
                assert_eq!(*event, AvailableShellsEvent::ScanComplete);
                events.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            });
        });

        // An unfinished scan answers "not known yet" immediately: no 300 ms condvar wait.
        let start = std::time::Instant::now();
        model.read(&app, |shells, _| {
            assert_eq!(shells.get_available_shells().count(), 0);
        });
        assert!(start.elapsed() < std::time::Duration::from_millis(100));
        assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 0);

        // Completion is pushed to subscribers, and from then on the list is served.
        model.update(&mut app, |shells, ctx| {
            assert!(shells.adopt_scan(1, bash_scan(), ctx));
        });
        assert_eq!(events.load(std::sync::atomic::Ordering::SeqCst), 1);
        model.read(&app, |shells, _| {
            assert_eq!(shells.get_available_shells().count(), 1);
        });
    });
}

#[test]
fn superseded_scan_completion_is_ignored() {
    warpui::App::test((), |mut app| async move {
        let model = app.add_model(|_| unscanned_available_shells());
        model.update(&mut app, |shells, ctx| {
            shells.scan_generation = 2; // a rescan started after generation 1
            assert!(!shells.adopt_scan(1, bash_scan(), ctx));
            assert_eq!(shells.get_available_shells().count(), 0);
            assert!(shells.adopt_scan(2, bash_scan(), ctx));
            assert_eq!(shells.get_available_shells().count(), 1);
        });
    });
}

#[test]
fn rescan_only_when_path_changes() {
    warpui::App::test((), |mut app| async move {
        let model = app.add_model(|_| make_available_shells(vec![]));
        model.update(&mut app, |shells, ctx| {
            shells.scanned_path = std::env::var_os("PATH").unwrap_or_default();
            assert!(!shells.rescan_if_path_changed(ctx));
            shells.scanned_path = std::ffi::OsString::from("/definitely/not/the/path");
            assert!(shells.rescan_if_path_changed(ctx));
            assert!(!shells.rescan_if_path_changed(ctx));
        });
    });
}

#[test]
fn test_load_known_shells_with_empty_path_var() {
    let _shell_selector_guard = FeatureFlag::ShellSelector.override_enabled(true);

    // First assert that if there is no fallback, and the env var is empty, we do not load ANY shells
    let paths_to_search = vec![];
    let fallback_shells = AvailableShells::load_known_shells(
        &paths_to_search,
        Some(Path::new("/some/nonexistent/path")),
    );
    assert!(
        fallback_shells.is_empty(),
        "expected there to be no shells, but shells contained {fallback_shells:?}"
    );

    VirtualFS::test(
        "test_load_known_shells_with_empty_path_var",
        |dirs, mut sandbox| {
            let bash = dirs.tests().join("bin").join("bash");
            let zsh = dirs.tests().join("bin").join("zsh");
            let fallback_shells_path = dirs.tests().join("etc").join("shells");
            // Now assert that if there is a fallback, and the env var is empty, we load the fallback
            sandbox.mkdir("etc");
            sandbox.mkdir("bin");
            sandbox.with_files(vec![
                Stub::FileWithContent(
                    "etc/shells",
                    format!("{}\n{}\n", bash.display(), zsh.display()).as_str(),
                ),
                Stub::MockExecutable("bin/bash"),
                Stub::MockExecutable("bin/zsh"),
            ]);

            let fallback_shells = AvailableShells::load_known_shells(
                &paths_to_search,
                Some(fallback_shells_path.as_path()),
            );

            assert_eq!(fallback_shells.len(), 2);
            // note about this test; current impl is that we add shells in the following order:
            //   zsh, bash, fish, pwsh, powershell
            // so it is important to assert that even though `/bin/bash` is located before `/bin/zsh`
            // in the fallback file, we still list `zsh` first.
            assert_eq!(
                fallback_shells,
                vec![
                    AvailableShell {
                        id: Some(format!("local:{}", zsh.display())),
                        state: Arc::new(Config::KnownLocal(LocalConfig {
                            command: "zsh".to_string(),
                            executable_path: zsh,
                            shell_type: ShellType::Zsh,
                        }))
                    },
                    AvailableShell {
                        id: Some(format!("local:{}", bash.display())),
                        state: Arc::new(Config::KnownLocal(LocalConfig {
                            command: "bash".to_string(),
                            executable_path: bash,
                            shell_type: ShellType::Bash,
                        }))
                    }
                ]
            )
        },
    );
}

#[test]
fn test_dedupe_symlinks_when_discovering_paths() {
    let _shell_selector_guard = FeatureFlag::ShellSelector.override_enabled(true);
    VirtualFS::test(
        "test_dedupe_symlinks_when_discovering_paths",
        |dirs, mut sandbox| {
            let bin = dirs.tests().join("bin");
            let bin_bash = bin.join("bash");
            let usr_bin = dirs.tests().join("usr").join("bin");
            let usr_bin_bash = usr_bin.join("bash");
            let etc_shells = dirs.tests().join("etc").join("shells");

            let paths_to_search = vec![bin, usr_bin];
            let etc_shells_content =
                format!("{}\n{}\n", bin_bash.display(), usr_bin_bash.display());

            sandbox.mkdir("etc");
            sandbox.mkdir("usr/bin");
            sandbox.ln("usr/bin", "bin");
            sandbox.with_files(vec![
                Stub::FileWithContent("etc/shells", etc_shells_content.as_str()),
                Stub::MockExecutable("usr/bin/bash"),
            ]);

            let fallback_shells =
                AvailableShells::load_known_shells(&paths_to_search, Some(etc_shells.as_path()));

            // We should expect there to be only one shell, with the path and id for that shell being
            // the canonical path to the executable.
            assert_eq!(
                fallback_shells,
                vec![AvailableShell {
                    id: Some(format!("local:{}", usr_bin_bash.display())),
                    state: Arc::new(Config::KnownLocal(LocalConfig {
                        command: "bash".to_string(),
                        executable_path: usr_bin_bash,
                        shell_type: ShellType::Bash,
                    }))
                }]
            )
        },
    );
}

#[test]
fn test_find_by_command_name_matches_known_shell() {
    let zsh_path = PathBuf::from("/bin/zsh");
    let pwsh_path = PathBuf::from("/opt/homebrew/bin/pwsh");
    let shells = make_available_shells(vec![
        AvailableShell::new_local_executable("zsh".to_string(), zsh_path.clone(), ShellType::Zsh),
        AvailableShell::new_local_executable(
            "pwsh".to_string(),
            pwsh_path.clone(),
            ShellType::PowerShell,
        ),
    ]);

    let matched = shells
        .find_by_command_name("pwsh")
        .expect("should find pwsh by command name");
    assert_eq!(
        matched.id(),
        Some(format!("local:{}", pwsh_path.display()).as_str()),
    );

    let matched = shells
        .find_by_command_name("zsh")
        .expect("should find zsh by command name");
    assert_eq!(
        matched.id(),
        Some(format!("local:{}", zsh_path.display()).as_str()),
    );
}

#[test]
fn test_find_by_command_name_returns_none_for_unknown_name() {
    let shells = make_available_shells(vec![AvailableShell::new_local_executable(
        "zsh".to_string(),
        PathBuf::from("/bin/zsh"),
        ShellType::Zsh,
    )]);

    assert!(shells.find_by_command_name("pwsh").is_none());
    assert!(shells.find_by_command_name("").is_none());
}

#[test]
fn test_find_by_command_name_is_case_sensitive_on_unix() {
    // File names on Unix are case-sensitive, so an uppercase request should
    // not match a lowercase stored command.
    let shells = make_available_shells(vec![AvailableShell::new_local_executable(
        "pwsh".to_string(),
        PathBuf::from("/opt/homebrew/bin/pwsh"),
        ShellType::PowerShell,
    )]);

    assert!(shells.find_by_command_name("pwsh").is_some());
    assert!(shells.find_by_command_name("PWSH").is_none());
    assert!(shells.find_by_command_name("PowerShell").is_none());
}

#[test]
fn test_find_by_command_name_skips_system_default() {
    // A SystemDefault entry should never be matched: it has no command name.
    let shells = make_available_shells(vec![
        AvailableShell::default(),
        AvailableShell::new_local_executable(
            "zsh".to_string(),
            PathBuf::from("/bin/zsh"),
            ShellType::Zsh,
        ),
    ]);

    let matched = shells
        .find_by_command_name("zsh")
        .expect("should find zsh past the SystemDefault entry");
    assert_eq!(
        matched.id(),
        Some(format!("local:{}", PathBuf::from("/bin/zsh").display()).as_str()),
    );
}

#[test]
fn test_find_by_command_name_matches_msys2_shell() {
    // Construct an MSYS2 shell directly so the `Config::MSYS2` arm of
    // `find_by_command_name` is exercised from any platform — its
    // `AvailableShell::new_msys2` constructor is gated to Windows, but the
    // match arm is platform-independent.
    let path = PathBuf::from("/tmp/msys64/usr/bin/bash-msys2");
    let msys2_shell = AvailableShell {
        id: Some(format!("msys2:{}", path.display())),
        state: Arc::new(Config::MSYS2(LocalConfig {
            command: "bash-msys2".to_string(),
            executable_path: path.clone(),
            shell_type: ShellType::Bash,
        })),
    };
    let shells = make_available_shells(vec![msys2_shell]);

    let matched = shells
        .find_by_command_name("bash-msys2")
        .expect("should find MSYS2 shell by command name");
    assert_eq!(
        matched.id(),
        Some(format!("msys2:{}", path.display()).as_str()),
    );
}

#[test]
fn test_command_name_matches_unix() {
    // Unix matching is a plain case-sensitive equality check: no case
    // folding, no `.exe` suffix handling.
    assert!(command_name_matches("pwsh", "pwsh", false));
    assert!(command_name_matches("zsh", "zsh", false));

    assert!(!command_name_matches("pwsh", "PWSH", false));
    assert!(!command_name_matches("pwsh", "pwsh.exe", false));
    assert!(!command_name_matches("pwsh.exe", "pwsh", false));
    assert!(!command_name_matches("pwsh", "powershell", false));
    assert!(!command_name_matches("", "pwsh", false));
}

#[test]
fn test_command_name_matches_windows() {
    // Windows matching is case-insensitive and allows an optional trailing
    // `.exe` on either side.
    assert!(command_name_matches("pwsh", "pwsh", true));
    assert!(command_name_matches("pwsh", "PWSH", true));
    assert!(command_name_matches("PWSH", "pwsh", true));
    assert!(command_name_matches("PwSh", "pWsH", true));

    // `.exe` is optional on either side.
    assert!(command_name_matches("pwsh.exe", "pwsh", true));
    assert!(command_name_matches("pwsh", "pwsh.exe", true));
    assert!(command_name_matches("pwsh.exe", "PWSH.EXE", true));
    assert!(command_name_matches("powershell.exe", "PowerShell", true));

    // Distinct shells should not collide.
    assert!(!command_name_matches("pwsh", "powershell", true));
    assert!(!command_name_matches("pwsh.exe", "powershell.exe", true));
    assert!(!command_name_matches("bash.exe", "zsh", true));
}

#[test]
fn test_nu_is_a_selectable_shell_with_a_friendly_name() {
    let nu_path = PathBuf::from("/opt/homebrew/bin/nu");
    let shell =
        AvailableShell::new_local_executable("nu".to_string(), nu_path.clone(), ShellType::Nu);
    assert_eq!(shell.short_name(), "Nushell");
    assert_eq!(
        shell.id(),
        Some(format!("local:{}", nu_path.display()).as_str())
    );
    // Saving the selection in the legacy startup-shell setting round-trips through `nu`.
    assert_eq!(StartupShell::from(shell), StartupShell::Nu);
    assert_eq!(StartupShell::Nu.shell_command(), Some("nu"));
    assert_eq!(StartupShell::from(Some("nu".to_owned())), StartupShell::Nu);

    let shells = make_available_shells(vec![AvailableShell::new_local_executable(
        "nu".to_string(),
        nu_path.clone(),
        ShellType::Nu,
    )]);
    assert!(shells.find_by_command_name("nu").is_some());
}

#[test]
fn test_nu_is_searched_for_by_name_when_loading_known_shells() {
    let shell_types = AvailableShells::get_shell_types();
    let expected = if cfg!(windows) { "nu.exe" } else { "nu" };
    assert!(
        shell_types.contains(&(ShellType::Nu, expected)),
        "{shell_types:?}"
    );
}

#[test]
fn saved_shell_is_used_while_the_scan_is_unfinished() {
    VirtualFS::test("saved_shell_while_scan_unfinished", |dirs, mut sandbox| {
        sandbox.mkdir("bin");
        sandbox.with_files(vec![Stub::MockExecutable("bin/zsh")]);
        let zsh = dirs.tests().join("bin").join("zsh");
        let shells = unscanned_available_shells(); // scan has not completed
        let pref = NewSessionShell::Executable(zsh.display().to_string());
        let start = std::time::Instant::now();
        let got = shells
            .resolve_preferred_directly(&pref)
            .expect("shell exists");
        assert!(start.elapsed() < std::time::Duration::from_millis(250));
        assert!(got.matches_preference(&pref));
        assert!(shells.scanned_if_ready().is_none());

        // A saved shell that no longer exists falls back to the default.
        let gone = NewSessionShell::Executable(
            dirs.tests().join("bin").join("fish").display().to_string(),
        );
        assert!(shells.resolve_preferred_directly(&gone).is_none());
    });
}

#[test]
fn saved_shell_the_finished_scan_does_not_list_is_still_used() {
    // r18u19 (Windows Git Bash pass): the scan lists Git Bash only as the MSYS2
    // `Git\usr\bin\bash.exe`, so a saved `Git\bin\bash.exe` matched nothing once the scan had
    // finished and new tabs started PowerShell. A finished scan must give the same answer as an
    // unfinished one: the saved shell when it exists, the default only when it is gone.
    VirtualFS::test("saved_shell_not_in_finished_scan", |dirs, mut sandbox| {
        sandbox.mkdir("bin");
        sandbox.with_files(vec![Stub::MockExecutable("bin/zsh")]);
        let zsh = dirs.tests().join("bin").join("zsh");
        let pref = NewSessionShell::Executable(zsh.display().to_string());

        let scanned = make_available_shells(bash_scan().shells); // finished, lacks the saved zsh
        let unscanned = unscanned_available_shells();
        let got = scanned.shell_for_preference(pref.clone());
        assert!(
            got.matches_preference(&pref),
            "the saved shell, not the default"
        );
        assert!(
            unscanned
                .shell_for_preference(pref.clone())
                .matches_preference(&pref)
        );

        // A shell the scan does list is still the scanned entry.
        let bash = NewSessionShell::Executable("/bin/bash".to_string());
        assert!(
            scanned
                .shell_for_preference(bash.clone())
                .matches_preference(&bash)
        );

        // A saved shell that no longer exists falls back to the default, scanned or not.
        let gone = NewSessionShell::Executable(
            dirs.tests().join("bin").join("fish").display().to_string(),
        );
        for shells in [&scanned, &unscanned] {
            let got = shells.shell_for_preference(gone.clone());
            assert!(matches!(got.state.as_ref(), Config::SystemDefault));
        }
    });
}
