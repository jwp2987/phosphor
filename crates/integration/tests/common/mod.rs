use std::env;
use std::process::Stdio;
use warpui::integration::RERUN_EXIT_CODE;

#[cfg(not(windows))]
use command::blocking::Command;

const MAX_TEST_RUNS: usize = 10;

/// Runs a single integration test.
///
/// This runs the `integration` binary from the `warp` crate, passing it the
/// name of the test to execute as the one positional argument.
#[cfg(not(windows))]
pub fn run_integration_test(name: &str) -> Result<(), String> {
    let mut keep_going = true;
    let mut run_num = 0;
    while keep_going {
        let inherited_envs = env::vars_os().filter(|(k, _v)| {
            let k = k
                .to_str()
                .expect("environment variable keys should contain valid unicode");
            // Propagate the PATH to the integration test
            // process, otherwise the shell it spawns might not
            // be able to find the binaries it needs to execute.
            k == "PATH"
                // Propagate any Rust-related variables.
                || k.starts_with("RUST_")
                // Propagate any Zap-specific variables.
                || k.starts_with("WARP_")
                || k.starts_with("WARPUI_")
                // Propagate any wgpu-specific variables.
                || k.starts_with("WGPU_")
                // Make sure the test knows what X or Wayland server to use.
                || k == "DISPLAY"
                || k == "WAYLAND_DISPLAY"
                // Propagate XDG_RUNTIME_DIR, which is needed for tests to run.
                // We actively _do not_ want to propagate other XDG_ variables,
                // as they tend to encode the home directory, which we override
                // in tests to point to a per-test temporary directory.
                || k == "XDG_RUNTIME_DIR"
                // Propagate XAUTHORITY so we can run headless tests using xvfb.
                || k == "XAUTHORITY"
        });
        keep_going = match Command::new(env!("CARGO_BIN_EXE_integration"))
            .arg(name)
            .env_clear()
            .envs(inherited_envs)
            .env("WARP_INTEGRATION", "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
        {
            Ok(status) => match status.code() {
                Some(0) => {
                    println!("Test exited with success.");
                    false
                }
                Some(RERUN_EXIT_CODE) if run_num < MAX_TEST_RUNS => {
                    println!("Test exited with rerun code, trying again.");
                    run_num += 1;
                    true
                }
                Some(exit_code) => {
                    return std::result::Result::Err(format!(
                        "Test {name} failed with exit code {exit_code}",
                    ));
                }
                None => {
                    #[cfg(unix)]
                    {
                        use std::os::unix::process::ExitStatusExt;
                        let signal = status
                            .signal()
                            .and_then(|signal| nix::sys::signal::Signal::try_from(signal).ok());
                        if let Some(signal) = signal {
                            return std::result::Result::Err(format!(
                                "Test {name} failed due to signal {}",
                                signal.as_str(),
                            ));
                        } else {
                            return std::result::Result::Err(format!(
                                "Test {name} failed for unknown reason",
                            ));
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        return std::result::Result::Err(format!(
                            "Test {name} failed for unknown reason",
                        ));
                    }
                }
            },
            Err(err) => {
                return std::result::Result::Err(format!("Test {name} failed with error {err:#}"));
            }
        }
    }
    Ok(())
}

/// Integration test runner for the Windows platform.
///
/// Uses std::process::Command instead of command::blocking::Command, because
/// the latter injects the CREATE_NO_WINDOW | CREATE_BREAKAWAY_FROM_JOB flags,
/// which prevents the integration test binary from properly creating a GUI
/// window (os error 5).
#[cfg(windows)]
pub fn run_integration_test(name: &str) -> Result<(), String> {
    let mut keep_going = true;
    let mut run_num = 0;
    while keep_going {
        let inherited_envs = env::vars_os().filter(|(k, _v)| {
            let k = k
                .to_str()
                .expect("environment variable keys should contain valid unicode");
            // Propagate the PATH to the integration test
            // process, otherwise the shell it spawns might not
            // be able to find the binaries it needs to execute.
            k == "PATH"
                // Propagate any Rust-related variables.
                || k.starts_with("RUST_")
                // Propagate any Zap-specific variables.
                || k.starts_with("WARP_")
                || k.starts_with("WARPUI_")
                // Propagate any wgpu-specific variables.
                || k.starts_with("WGPU_")
                // Windows: SystemRoot is required for DLL resolution (kernel32, etc.).
                // TEMP/TMP are needed for temporary file operations.
                // USERPROFILE is overridden per-test but must be present initially.
                // LOCALAPPDATA/APPDATA are needed by various Windows APIs.
                // ProgramFiles/ProgramFiles(x86) are needed to locate PowerShell and other tools.
                || k.eq_ignore_ascii_case("SystemRoot")
                || k.eq_ignore_ascii_case("TEMP")
                || k.eq_ignore_ascii_case("TMP")
                || k.eq_ignore_ascii_case("USERPROFILE")
                || k.eq_ignore_ascii_case("LOCALAPPDATA")
                || k.eq_ignore_ascii_case("APPDATA")
                || k.eq_ignore_ascii_case("ProgramFiles")
                || k.eq_ignore_ascii_case("ProgramFiles(x86)")
                || k.eq_ignore_ascii_case("ProgramW6432")
                || k.eq_ignore_ascii_case("OS")
                || k.eq_ignore_ascii_case("COMPUTERNAME")
                || k.eq_ignore_ascii_case("USERNAME")
                || k.eq_ignore_ascii_case("PATHEXT")
                || k.eq_ignore_ascii_case("HOMEDRIVE")
                || k.eq_ignore_ascii_case("HOMEPATH")
                || k.eq_ignore_ascii_case("CommonProgramFiles")
                || k.eq_ignore_ascii_case("CommonProgramFiles(x86)")
                || k.eq_ignore_ascii_case("CommonProgramW6432")
                || k.eq_ignore_ascii_case("PROCESSOR_ARCHITECTURE")
                || k.eq_ignore_ascii_case("PROCESSOR_IDENTIFIER")
                || k.eq_ignore_ascii_case("NUMBER_OF_PROCESSORS")
                || k.eq_ignore_ascii_case("SYSTEMDRIVE")
        });
        keep_going = match std::process::Command::new(env!("CARGO_BIN_EXE_integration"))
            .arg(name)
            .env_clear()
            .envs(inherited_envs)
            .env("WARP_INTEGRATION", "1")
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdin(Stdio::null())
            .status()
        {
            Ok(status) => match status.code() {
                Some(0) => {
                    println!("Test exited with success.");
                    false
                }
                Some(RERUN_EXIT_CODE) if run_num < MAX_TEST_RUNS => {
                    println!("Test exited with rerun code, trying again.");
                    run_num += 1;
                    true
                }
                Some(exit_code) => {
                    return std::result::Result::Err(format!(
                        "Test {name} failed with exit code {exit_code}",
                    ));
                }
                None => {
                    return std::result::Result::Err(format!(
                        "Test {name} failed for unknown reason",
                    ));
                }
            },
            Err(err) => {
                return std::result::Result::Err(format!("Test {name} failed with error {err:#}"));
            }
        }
    }
    Ok(())
}

/// Declares the `#[test]` shims that shell out to the `integration` binary.
///
/// Each entry generates two things:
///
/// 1. a `#[test]` fn that runs the binary with the test's name as its argument, and
/// 2. an anonymous const that names the test's builder function directly.
///
/// (2) exists because (1) resolves the name at RUNTIME, as a string, in a child
/// process: `register_tests()` in `src/bin/integration.rs` looks it up in a
/// `HashMap<&str, _>` and panics with "test not found" on a miss. Nothing tied
/// this list to that map, so deleting a test's implementation and its
/// `register_test!` line while leaving its name here produced a test that
/// compiled, ran, and could only ever fail. That is exactly how 22 permanently
/// red tests (18 `test_sftp_*`, 4 `test_websocket_*`) survived two deliberate
/// feature removals — see 3c657be07 and 26c1ed8b5.
///
/// The const turns that runtime panic into a compile error: a name with no
/// `pub fn <name>() -> Builder` in `integration::test` no longer builds. It does
/// NOT check that the name is wired into `register_tests()`; `script/check_integration_test_registry`
/// covers that half.
#[macro_export]
macro_rules! integration_tests {
(   $(
            $(#[$args:meta])*
            $name:ident,
        )*
    ) => {
        $(
            $(#[$args])*
            // Ignore unused attributes, in case we're marking a test as
            // ignored twice, once via arguments passed to the macro and once
            // below.
            #[allow(unused_attributes)]
            // For right now, we only want to run integration tests on macOS
            // and Linux (iff the run_on_linux feature is enabled).
            #[cfg_attr(not(any(target_os = "macos", feature = "run_on_linux", feature = "run_on_windows")), ignore)]
            #[test]
            fn $name() -> Result<(), String> {
                // Compile-time proof that this name has an implementation. See
                // the doc comment above. It lives inside the fn body so that a
                // `#[cfg(...)]` on the entry gates it too, and so `#[ignore]`
                // (not a valid attribute on a const item) still leaves it
                // compiled -- an ignored test is still a name that must resolve.
                const _: fn() -> ::integration::Builder = ::integration::test::$name;
                $crate::common::run_integration_test(stringify!($name))
            }
        )*
    }
}
