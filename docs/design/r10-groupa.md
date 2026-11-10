# Round 10, group A: #1966 #1985 #2004 #1907 #1938

Branch `fix/r10-groupa`, base integ/r10 `cfe7249e8`. Nothing here was compiled or run (laptop: rustfmt, `run.py --self-test`
and `script/check_input_path_timers` only). Wording: merged-untested work is "worked, unproven".

## 1. #1966 Windows Git Bash agent question cards read as hung

**Evidence.** `cases_agent_question.verdict` (`script/realapp/appci/cases_agent_question.py:34-70`) calls `AR.action_hops`, i.e. it reads
`hop=agent_action` lines. They come from `app/src/ai/blocklist/action_model.rs:761` (`log::info!`, module
`warp::ai::blocklist::action_model`). Linux and macOS launch with no base level (`app.py` INNER script, `warp::input_order=debug,...`), and
env_logger's default with no bare level shows only what a directive names, but the app's own default applies there; on Windows
`WindowsApp.env` sets `RUST_LOG="warn,winit=info,...`, a `warn` base that hides every `info` line from a module with no directive of its own.
Only `agent_providers` and `blocklist::controller` had one, so a card that WAS shown reads as `hung` ("no hop=agent_action").

Other hidden modules, found by scanning the Rust sources for plain `log::info!/debug!` calls whose string carries a `hop=` name some appci case
reads (13 hops in 13 modules): `cli_controller` (`handover`), `queued_query` (`queued_row`), `diff_state_remote`, `git_status_update_remote`,
`github_repo_model::remote` (`diff_state`, `git_chip`, `gh_chip`), `pushed_listing` (`cwd_listing`, `path_detect`), `remote_index_driver`
(`remote_index_sync`, `remote_index_embed`), `network_page` (`network_gate_ui`), `crates/remote_server/src/manager.rs` (`agent_context`,
`file_tree`) and `router.rs` (`backend_route`, logged through `route_log_line`). Every `input_order` hop (the other ~80 names) goes through
`warp::input_order=debug`, already enabled.

**Change.** `app.WINDOWS_RUST_LOG` (one constant the launcher uses) now has `=info` directives for all of those modules. `app.rust_log_enabled`
implements the env_logger prefix rule. Self-test `windows_rust_log_enables_every_hop_module_1966` rescans the sources and fails if any such module
is hidden (so a new plain-log hop forces a new directive), and proves the matcher can detect the original defect (the pre-fix string hides
`action_model`).

**Rejected.** Dropping the `warn` base on Windows (it exists because Windows runs were flooded; r18d2 comment in `app.py`). Making the app log
`hop=` lines at `warn` (changes product log levels for a harness problem). Moving the hop to the `input_order` target (app change, and the
module-enabled check is the more general guard).

**Risk.** Log volume on Windows grows by the info lines of 13 modules: low rate (state changes). A module path derived from the file path is wrong
for a `#[path]` or re-exported module; the scan would then pass a hidden module. None of the 13 uses either.
