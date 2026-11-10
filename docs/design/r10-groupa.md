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

## 2. #1985 flood_scrollback_find false-FAILs when OCR is blind

**Evidence.** `cases_robust.py` `c_flood_find`: the final `r.check(seen and scrolled, ...)` needed OCR to read the marker. `seen` is False only
when OCR read the marker nowhere, not even in the Find field it was typed into (the r18u28 case: "OCR read the marker nowhere"), i.e. OCR is blind. The
case already parses the app's own `hop=find_scroll result=scroll ... moved=... viewport_lines= match_line=` (`find_scroll_geometry`) and notes "the app
placed the match ON screen", but only as a note: the verdict stayed a FAIL.

**Change.** Pure `find_verdict(seen, scrolled, hops, geo)`. OCR read the marker: unchanged (pass iff the view left the flood's tail). OCR blind:
geometry ON screen and `moved=true` -> pass, noted as resting on the app log; geometry OFF screen -> fail; only `result=no_match` hops -> fail (the app
found nothing); no hop, no/`busy` geometry, or ON screen with `moved=false` (self-contradicting: the marker was off screen before) -> `r.unverifiable`
(NOREF, never a pass). The agent-mode Skip for an all-`no_match` log still comes first. Selftest rows cover each of those; the old structural assertion
now orders the Skip before `find_verdict(`.

**Rejected.** Treating ON-screen geometry as a pass when OCR DID read the marker but the tail is still visible (that is the r18u19 false pass; stays a
fail). Failing an OCR-seen run whose geometry says OFF screen (a stricter check than asked; the Find-field hit is weak evidence either way, so left
for the owner). Removing the "no OCR engine" Skip at the top: with no engine the control (marker not on screen before the Find) cannot be checked.

**Risk.** A wrong `match_line` in the app (the rendered vs scroll mode disagreement the case already notes) would now pass a blind-OCR run; the
note says so and the mode mismatch note is kept.
