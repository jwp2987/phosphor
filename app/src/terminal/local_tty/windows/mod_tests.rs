//! Unit tests for the Windows pseudoconsole spawn helpers.

use windows::Win32::System::Threading::{STARTF_USESTDHANDLES, STARTUPINFOEXW};

use super::new_pty_startup_info;

/// The pseudoconsole child must be launched with `STARTF_USESTDHANDLES` set and
/// all three standard handles left null.
///
/// Without the flag, `CreateProcessW` copies the *parent's* standard handles
/// into the child, so a Zap launched with redirected stdio (a terminal, a test
/// runner, CI) hands its shell a redirected stdin. PowerShell then starts its
/// REPL, emits the `InitShell` prompt hook, reads EOF on the first stdin read
/// and exits 0 — which the terminal sees as a bootstrap that never completes.
/// The flag with null handles is what makes the console subsystem give the
/// child the pseudoconsole's own handles instead. This matches Warp at the
/// pinned oracle and Windows Terminal's `ConptyConnection`.
///
/// A previous fork commit deleted the flag; this test exists so that deletion
/// cannot recur silently.
#[test]
fn pty_startup_info_requests_null_std_handles() {
    let startup_info = new_pty_startup_info();

    assert_eq!(
        startup_info.StartupInfo.cb as usize,
        std::mem::size_of::<STARTUPINFOEXW>(),
        "cb must describe the extended struct, since we pass EXTENDED_STARTUPINFO_PRESENT"
    );
    assert_eq!(
        startup_info.StartupInfo.dwFlags.0 & STARTF_USESTDHANDLES.0,
        STARTF_USESTDHANDLES.0,
        "STARTF_USESTDHANDLES must be set so the child inherits no stdio from Zap"
    );
    assert!(
        startup_info.StartupInfo.hStdInput.0.is_null(),
        "hStdInput must stay null so the pseudoconsole supplies the child's stdin"
    );
    assert!(
        startup_info.StartupInfo.hStdOutput.0.is_null(),
        "hStdOutput must stay null so the pseudoconsole supplies the child's stdout"
    );
    assert!(
        startup_info.StartupInfo.hStdError.0.is_null(),
        "hStdError must stay null so the pseudoconsole supplies the child's stderr"
    );
}
