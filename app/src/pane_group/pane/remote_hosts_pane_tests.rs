use super::*;
use remote_server::setup::{GlibcVersion, UnsupportedReason};

fn host(target: &str, install_state: HostInstallState) -> RemoteHostEntry {
    RemoteHostEntry {
        target: target.to_string(),
        host_id: None,
        install_state,
        last_reached_at: None,
        os: None,
        arch: None,
    }
}

/// The central claim of this pane: a host that has never been probed reads as "Never reached",
/// never as "Not installed". Fails if `host_install_display` ever maps
/// `HostInstallState::Unknown` to `HostInstallDisplay::NotInstalled` (or to anything sharing
/// `NotInstalled`'s label) -- collapsing the two would render an unprobed host as though it had
/// been checked and found missing the remote server. See the module doc comment.
#[test]
fn never_reached_is_distinct_from_not_installed() {
    let never_reached = host_install_display(&HostInstallState::Unknown);
    let not_installed = host_install_display(&HostInstallState::NotInstalled);

    assert_eq!(never_reached, HostInstallDisplay::NeverReached);
    assert_eq!(not_installed, HostInstallDisplay::NotInstalled);
    assert_ne!(
        never_reached, not_installed,
        "an unprobed host and a host confirmed missing the remote server must not collapse \
         into the same display state"
    );
    assert_ne!(
        host_install_label(&never_reached),
        host_install_label(&not_installed),
        "the two states must render different text, not just different enum variants"
    );
}

#[test]
fn installed_state_carries_its_version_into_the_label() {
    let display = host_install_display(&HostInstallState::Installed {
        version: Some("1.2.3".to_string()),
    });
    assert_eq!(
        display,
        HostInstallDisplay::Installed {
            version: Some("1.2.3".to_string())
        }
    );
    assert!(host_install_label(&display).contains("1.2.3"));
}

/// `HostInstallState::Installed { version: None }` is a real value: install can complete
/// before any handshake has reported a real `InitializeResponse::server_version` -- see the
/// doc comment on `host_install_label`. Fails if a missing version is ever rendered as a
/// version (e.g. "Installed (v)") instead of its own "unknown" label, and fails if that label
/// is ever confused with `NotInstalled`'s or `NeverReached`'s.
#[test]
fn installed_with_no_version_reads_as_unknown_not_as_a_blank_version() {
    let display = host_install_display(&HostInstallState::Installed { version: None });
    let label = host_install_label(&display);

    assert!(
        !label.contains("(v)"),
        "a missing version must never render as though it were a real version: got {label:?}"
    );
    assert_ne!(label, host_install_label(&HostInstallDisplay::NotInstalled));
    assert_ne!(label, host_install_label(&HostInstallDisplay::NeverReached));
}

#[test]
fn unsupported_state_names_the_reason() {
    let display = host_install_display(&HostInstallState::Unsupported {
        reason: UnsupportedReason::GlibcTooOld {
            detected: GlibcVersion::new(2, 17),
            required: GlibcVersion::new(2, 28),
        },
    });
    let label = host_install_label(&display);
    assert!(label.contains("2.17"));
    assert!(label.contains("2.28"));

    let non_glibc = host_install_display(&HostInstallState::Unsupported {
        reason: UnsupportedReason::NonGlibc {
            name: "musl".to_string(),
        },
    });
    assert!(host_install_label(&non_glibc).contains("musl"));
}

/// `last_reached_label` draws the same never-vs-observed distinction as install state, over the
/// registry's other advisory field. Fails if `None` and `Some(_)` ever produce the same text.
#[test]
fn last_reached_label_distinguishes_never_from_a_real_timestamp() {
    let never = last_reached_label(None);
    let observed = last_reached_label(Some(Utc::now()));
    assert_eq!(never, "Never");
    assert_ne!(never, observed);
}

#[test]
fn platform_label_reports_unknown_only_when_nothing_is_observed() {
    assert_eq!(platform_label(None, None), "Unknown");
    assert_eq!(
        platform_label(Some("linux"), Some("x86_64")),
        "linux / x86_64"
    );
    assert_eq!(platform_label(Some("linux"), None), "linux");
    assert_eq!(platform_label(None, Some("x86_64")), "x86_64");
}

/// `host_row_text` is what the pane actually renders per host; this locks in that a
/// never-reached host's row never contains the word "installed" on its own (it says "Never
/// reached", not some substring collision), while a positively-installed host's row does.
#[test]
fn host_row_text_reflects_install_state_distinctly() {
    let never_reached = host("build-box", HostInstallState::Unknown);
    let not_installed = host("build-box", HostInstallState::NotInstalled);
    let installed = host(
        "build-box",
        HostInstallState::Installed {
            version: Some("9.9.9".to_string()),
        },
    );

    assert!(host_row_text(&never_reached).contains("Never reached"));
    assert!(!host_row_text(&never_reached).contains("Not installed"));
    assert!(host_row_text(&not_installed).contains("Not installed"));
    assert!(host_row_text(&installed).contains("Installed (v9.9.9)"));
}

#[test]
fn group_row_text_lists_members_or_says_so_when_empty() {
    let empty = HostGroup {
        name: "prod-api".to_string(),
        members: vec![],
    };
    let populated = HostGroup {
        name: "prod-api".to_string(),
        members: vec!["web-1".to_string(), "web-2".to_string()],
    };

    assert!(group_row_text(&empty).contains("no members"));
    let populated_text = group_row_text(&populated);
    assert!(populated_text.contains("web-1"));
    assert!(populated_text.contains("web-2"));
}

// --- Sessions ---------------------------------------------------------------
//
// The mapping from `RemoteSessionSummary::exit` to a displayed run state, and
// the "not connected" / "no sessions" split. Both are pure; neither needs a
// daemon. What cannot be tested here is that a listing ever arrives:
// `RemoteSessionsModel::start_fetch` issues a real `list_sessions` RPC over an
// SSH connection, so only its decisions are covered (see
// `app/src/remote_server/remote_sessions_model_tests.rs`), not the round trip.

fn session(id: &str, exit: Option<RemoteSessionExit>) -> RemoteSessionSummary {
    RemoteSessionSummary {
        remote_session_id: id.to_string(),
        cwd: "/srv/app".to_string(),
        shell: Some("/bin/zsh".to_string()),
        rows: 40,
        cols: 120,
        exit,
        buffered_bytes: Some(2048),
        dropped_bytes_total: Some(0),
    }
}

fn exited(code: i32) -> Option<RemoteSessionExit> {
    Some(RemoteSessionExit {
        exit_code: Some(code),
        signal_killed: false,
    })
}

/// A `Fetched` display over `sessions`, for the reap-affordance tests.
fn fetched(sessions: &[RemoteSessionSummary]) -> HostSessionsDisplay<'_> {
    HostSessionsDisplay::Fetched {
        fetched_at: Utc::now(),
        sessions,
    }
}

/// The distinction the `exit` field was added to the proto to carry: before it
/// existed, a client could enumerate sessions but not tell a live one from a
/// dead one.
///
/// // Breaks if: `session_run_state` stops treating the absence of `exit` as
/// "running", or ever renders a running session with the same text as an exited
/// one.
#[test]
fn a_session_with_no_exit_reads_as_running_and_an_exited_one_does_not() {
    let running = session_run_state(None);
    let exited = session_run_state(Some(&RemoteSessionExit {
        exit_code: Some(0),
        signal_killed: false,
    }));

    assert_eq!(running, SessionRunState::Running);
    assert_eq!(exited, SessionRunState::Exited { code: 0 });
    assert_ne!(
        session_run_label(&running),
        session_run_label(&exited),
        "a live session and a session that exited cleanly must not render the same text"
    );
}

/// A signalled process has no exit code of its own. Reporting one would be a
/// fabrication -- `SessionExitStatus::signalled()` exists in the daemon for
/// exactly this reason.
///
/// // Breaks if: `session_run_state` ever folds a signalled session into
/// `Exited { code: 0 }`, or its label starts claiming a code.
#[test]
fn a_signalled_session_never_reports_a_fabricated_exit_code() {
    let state = session_run_state(Some(&RemoteSessionExit {
        exit_code: None,
        signal_killed: true,
    }));

    assert_eq!(state, SessionRunState::Signalled);
    let label = session_run_label(&state);
    assert!(
        !label.contains('0'),
        "a signalled session has no exit code; got {label:?}"
    );
    assert_ne!(
        label,
        session_run_label(&SessionRunState::Exited { code: 0 }),
        "killed by a signal and exited with status 0 are different outcomes"
    );
}

/// `signal_killed` and a present `exit_code` are contradictory wire data. The
/// signal is the half that is load-bearing, because a genuinely signalled
/// process cannot have produced a code.
///
/// // Breaks if: `session_run_state` ever prefers the code over `signal_killed`
/// and reports a clean exit for a killed session.
#[test]
fn signal_killed_wins_over_a_contradictory_exit_code() {
    let state = session_run_state(Some(&RemoteSessionExit {
        exit_code: Some(0),
        signal_killed: true,
    }));

    assert_eq!(state, SessionRunState::Signalled);
}

/// Proto3 defaults both fields of `RemoteSessionExit`, so a bare `exit {}` is
/// representable on the wire even though today's daemon never sends one
/// (`SessionExitStatus` is only ever built by `exited(code)` or `signalled()`).
///
/// // Breaks if: an exit that says neither how nor with what is ever rendered
/// as `Exited { code: 0 }`, reporting a successful exit nothing observed.
#[test]
fn an_exit_that_says_nothing_is_not_reported_as_a_clean_exit() {
    let state = session_run_state(Some(&RemoteSessionExit {
        exit_code: None,
        signal_killed: false,
    }));

    assert_eq!(state, SessionRunState::ExitedWithUnknownStatus);
    assert_ne!(state, SessionRunState::Exited { code: 0 });
    assert_ne!(state, SessionRunState::Running);
    assert_ne!(
        session_run_label(&state),
        session_run_label(&SessionRunState::Exited { code: 0 })
    );
}

/// A dashboard that renders an empty list for a host it never asked is lying.
///
/// // Breaks if: `host_sessions_label` ever produces the same text for a host
/// with no live connection and a connected host that answered with zero
/// sessions.
#[test]
fn not_connected_and_no_sessions_read_differently() {
    let not_connected = host_sessions_label(&HostSessionsDisplay::NotConnected);
    let none_running = host_sessions_label(&HostSessionsDisplay::Fetched {
        fetched_at: Utc::now(),
        sessions: &[],
    });
    let never_checked = host_sessions_label(&HostSessionsDisplay::NeverFetched);

    assert_ne!(not_connected, none_running);
    assert_ne!(not_connected, never_checked);
    assert_ne!(never_checked, none_running);
}

/// A failed listing must not read as an empty one.
///
/// // Breaks if: the failure label drops the reason, or collapses into the
/// zero-sessions label.
#[test]
fn an_unavailable_listing_says_why_and_is_not_an_empty_one() {
    let label = host_sessions_label(&HostSessionsDisplay::Failed {
        failed_at: Utc::now(),
        reason: "request timed out after 30s",
    });

    assert!(label.contains("request timed out after 30s"));
    assert_ne!(
        label,
        host_sessions_label(&HostSessionsDisplay::Fetched {
            fetched_at: Utc::now(),
            sessions: &[],
        })
    );
}

/// // Breaks if: `host_sessions_checked_at` ever reports a read time for a
/// state where nothing was read -- which would render "checked 0 sec ago" under
/// a host this pane never contacted.
#[test]
fn only_a_state_that_actually_read_something_carries_a_checked_time() {
    assert_eq!(
        host_sessions_checked_at(&HostSessionsDisplay::NotConnected),
        None
    );
    assert_eq!(
        host_sessions_checked_at(&HostSessionsDisplay::NeverFetched),
        None
    );
    assert_eq!(
        host_sessions_checked_at(&HostSessionsDisplay::Loading),
        None
    );

    let at = Utc::now();
    assert_eq!(
        host_sessions_checked_at(&HostSessionsDisplay::Fetched {
            fetched_at: at,
            sessions: &[],
        }),
        Some(at)
    );
    assert_eq!(
        host_sessions_checked_at(&HostSessionsDisplay::Failed {
            failed_at: at,
            reason: "nope",
        }),
        Some(at)
    );
}

/// The row has to carry the fields the summary actually has, and only those.
///
/// The buffer figures are now among them: `RemoteSessionSummary` gained
/// `buffered_bytes` and `dropped_bytes_total` (fields 7 and 8), so the
/// assertion this test used to make -- that the row must never say "dropped" --
/// was pinning a fact about the wire that has since changed. What replaces it
/// is the constraint that survived: the row may say "dropped", and must never
/// say "detached", because the figure it has is the lifetime total and not the
/// per-gap one. See `session_buffer_label`.
///
/// // Breaks if: `session_row_text` drops the run state, the cwd, the size or
/// the buffer figures -- or starts labelling the lifetime drop count as a gap.
#[test]
fn a_session_row_shows_what_the_summary_carries() {
    let mut summary = session("9f1c", exited(3));
    summary.buffered_bytes = Some(4096);
    summary.dropped_bytes_total = Some(1024);
    let text = session_row_text(&summary);

    assert!(text.contains("9f1c"), "{text:?}");
    assert!(text.contains("exited (3)"), "{text:?}");
    assert!(text.contains("/srv/app"), "{text:?}");
    assert!(text.contains("/bin/zsh"), "{text:?}");
    assert!(text.contains("40x120"), "{text:?}");
    assert!(text.contains("4.0 KiB"), "{text:?}");
    assert!(text.contains("1.0 KiB"), "{text:?}");
    assert!(
        !text.to_lowercase().contains("detached"),
        "the listing carries the lifetime total, not the per-gap \"dropped while detached\" \
         figure; nothing may imply otherwise: {text:?}"
    );
}

/// // Breaks if: a summary with no shell recorded starts rendering as though a
/// shell named "unknown" had been observed, or as an empty gap.
#[test]
fn a_session_with_no_recorded_shell_says_so() {
    let mut summary = session("abcd", None);
    summary.shell = None;

    let text = session_row_text(&summary);
    assert!(text.contains("shell not reported"), "{text:?}");
}

// --- Buffer pressure --------------------------------------------------------
//
// `SessionStore::list()` always computed these; the protocol change that added
// `RemoteSessionSummary` fields 7 and 8 is what let them reach a client. What
// cannot be tested here is that a daemon ever sends real ones -- that needs a
// live daemon holding a real pty -- so these cover the rendering decisions only.

/// // Breaks if: `format_byte_count` switches to decimal units, which would
/// render the store's own 256 KiB bound as "262.1 KB" and invite a reader to
/// compare it against a bound written in the other base.
#[test]
fn byte_counts_render_in_the_same_binary_units_the_store_measures_in() {
    assert_eq!(format_byte_count(0), "0 B");
    assert_eq!(format_byte_count(512), "512 B");
    assert_eq!(format_byte_count(1024), "1.0 KiB");
    assert_eq!(format_byte_count(256 * 1024), "256.0 KiB");
    assert_eq!(format_byte_count(1024 * 1024), "1.0 MiB");
}

/// The defect `PeekedOutput`'s docs in `session_store.rs` are emphatic about:
/// the lifetime eviction total and the per-reattach gap are different numbers,
/// and showing one under the other's name misreports by the whole history of
/// the session. `ListSessions` carries only the lifetime figure -- `list()`
/// reads no per-client watermark -- so this label must name it as such.
///
/// // Breaks if: the wording ever implies a detached gap ("dropped while
/// detached", "dropped since you left"), which would be reporting a number
/// nothing computed.
#[test]
fn the_dropped_figure_is_labelled_as_a_lifetime_total_not_as_a_detached_gap() {
    let label = session_buffer_label(Some(4096), Some(1024 * 1024));

    assert!(label.contains("1.0 MiB"), "{label:?}");
    assert!(label.contains("in total"), "{label:?}");
    let lowered = label.to_lowercase();
    assert!(!lowered.contains("detached"), "{label:?}");
    assert!(!lowered.contains("since"), "{label:?}");
}

/// A daemon built before fields 7 and 8 existed sends neither. They are
/// `optional` on the wire precisely so that silence stays distinguishable from
/// a measured zero -- a bare proto3 scalar would have decoded it as `0`, and
/// this pane would have reported "buffering nothing, lost nothing" about a
/// machine that never said so.
///
/// // Breaks if: `session_buffer_label` renders `None` and `Some(0)` the same
/// way, in either direction.
#[test]
fn an_unreported_figure_never_renders_as_a_measured_zero() {
    let unknown = session_buffer_label(None, None);
    let measured_zero = session_buffer_label(Some(0), Some(0));

    assert!(unknown.contains("not reported"), "{unknown:?}");
    assert_eq!(measured_zero, "buffered 0 B");
    assert_ne!(unknown, measured_zero);

    // And independently per field: an old daemon is the only source of `None`,
    // but a future one could carry one and not the other.
    assert!(
        session_buffer_label(Some(10), None).contains("not reported"),
        "an unknown drop count must say so even when the buffer size is known"
    );
    assert!(
        session_buffer_label(None, Some(10)).contains("not reported"),
        "an unknown buffer size must say so even when the drop count is known"
    );
}

/// A zero drop count earns no clause: every healthy session would otherwise
/// carry "0 B dropped in total", which trains the reader to skip the field that
/// only matters when it is non-zero. The absence is unambiguous only because
/// the unknown case above says something instead.
///
/// // Breaks if: `Some(0)` starts rendering a dropped clause, or a non-zero
/// count stops rendering one.
#[test]
fn a_zero_drop_count_is_silent_and_a_real_one_is_not() {
    assert_eq!(
        session_buffer_label(Some(2048), Some(0)),
        "buffered 2.0 KiB"
    );
    assert!(
        session_buffer_label(Some(2048), Some(1)).contains("dropped"),
        "a session that has lost even one byte must say so"
    );
}

// --- Reaping exited sessions ------------------------------------------------
//
// The first control this pane has ever had -- see the module docs' "The one
// exception" for why it is admitted and why it is per host rather than per row.
// What cannot be tested here is the reap itself: `forget_exited_sessions`
// issues real `ForgetSession` RPCs over an SSH connection, and whether the
// daemon accepts them is a fact about a live daemon. Covered here is only what
// decides whether the control is drawn, what it says, and what is said
// afterwards.

/// The bound that makes this control safe: `handle_forget_session` refuses a
/// running session outright, so a control that could name one would be offering
/// a guaranteed refusal.
///
/// // Breaks if: `reap_affordance` counts anything but exited sessions, or
/// offers a control for a host whose listing holds only live ones.
#[test]
fn the_reap_control_counts_only_exited_sessions() {
    let only_running = [session("a", None), session("b", None)];
    assert_eq!(
        reap_affordance(&fetched(&only_running), None),
        ReapAffordance::Hidden,
        "nothing has ended, so there is nothing to clear"
    );

    let mixed = [
        session("a", None),
        session("b", exited(0)),
        session("c", exited(1)),
    ];
    assert_eq!(
        reap_affordance(&fetched(&mixed), None),
        ReapAffordance::Offer { count: 2 },
        "the running session must not be counted into the control's promise"
    );
}

/// The count is on the control so the blast radius is visible before the click.
///
/// // Breaks if: the label stops stating the count or stops saying "exited" --
/// those two words are the whole of what bounds the action.
#[test]
fn the_reap_control_states_its_count_and_its_bound() {
    assert_eq!(reap_button_label(1), "Forget 1 exited session");
    assert_eq!(reap_button_label(3), "Forget 3 exited sessions");
    assert!(reap_working_label(3).contains('3'));
}

/// "Not connected" wins over everything, the same precedence
/// `host_sessions_display` applies: there is no client to carry a
/// `ForgetSession`, so the control could not work.
///
/// // Breaks if: `reap_affordance` consults the reap state before the display,
/// leaving a control (or a "forgetting..." line) under a host whose connection
/// is gone.
#[test]
fn a_disconnected_host_offers_no_reap_control() {
    assert_eq!(
        reap_affordance(&HostSessionsDisplay::NotConnected, None),
        ReapAffordance::Hidden
    );
    assert_eq!(
        reap_affordance(
            &HostSessionsDisplay::NotConnected,
            Some(&ReapState::InFlight { count: 2 })
        ),
        ReapAffordance::Hidden
    );
}

/// Without a listing there is no set of exited ids, so there is no number to
/// put on a control and no defensible thing for it to do.
///
/// // Breaks if: any state short of `Fetched` starts offering a control.
#[test]
fn a_host_with_no_listing_offers_no_reap_control() {
    for display in [
        HostSessionsDisplay::NeverFetched,
        HostSessionsDisplay::Loading,
        HostSessionsDisplay::Failed {
            failed_at: Utc::now(),
            reason: "timed out",
        },
    ] {
        assert_eq!(
            reap_affordance(&display, None),
            ReapAffordance::Hidden,
            "{display:?} has no listing to take a count from"
        );
    }
}

/// A second click during a reap would start an overlapping one over ids read
/// from the same listing; the loser would report failures for sessions the
/// winner had already removed.
///
/// // Breaks if: an in-flight reap still renders an `Offer`.
#[test]
fn a_reap_in_flight_replaces_the_control_rather_than_repeating_it() {
    let sessions = [session("a", exited(0)), session("b", exited(0))];

    assert_eq!(
        reap_affordance(&fetched(&sessions), Some(&ReapState::InFlight { count: 2 })),
        ReapAffordance::Working { count: 2 }
    );
}

/// The sessions stay listed whether a reap worked or not, so a failure that
/// said nothing would look exactly like a click that did nothing. A success
/// says nothing on purpose: the refreshed listing is the evidence.
///
/// // Breaks if: `reap_error_label` goes quiet on a failure, or starts
/// announcing states that are not failures.
#[test]
fn only_a_failed_reap_says_anything_afterwards() {
    assert_eq!(reap_error_label(None), None);
    assert_eq!(
        reap_error_label(Some(&ReapState::InFlight { count: 1 })),
        None
    );

    let failed = ReapState::Failed {
        failed_at: Utc::now(),
        reason: "1 of 2 could not be forgotten: s1: Connection was dropped".to_string(),
    };
    let label = reap_error_label(Some(&failed)).expect("a failure must say so");
    assert!(label.contains("Connection was dropped"), "{label:?}");
    assert!(reap_failed_at(Some(&failed)).is_some());
}

/// A failure does not take the control away: the sessions it could not forget
/// are still listed, and the user must be able to try again.
///
/// // Breaks if: `reap_affordance` starts treating `Failed` as a reason to hide
/// the control, stranding the rows with no way to clear them.
#[test]
fn a_failed_reap_still_offers_to_try_again() {
    let sessions = [session("a", exited(0))];
    let failed = ReapState::Failed {
        failed_at: Utc::now(),
        reason: "nope".to_string(),
    };

    assert_eq!(
        reap_affordance(&fetched(&sessions), Some(&failed)),
        ReapAffordance::Offer { count: 1 }
    );
}
