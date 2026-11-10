# Round 10: the four network gate cases on Linux Wayland (#1830, #1987, #2014)

Cases: `network_gate_notice`, `agent_gate_deny_allow`, `agent_gate_off_live`,
`agent_gate_key_never_leaves` (all `agent@fake`). They have failed on every Wayland run
since they were written (rounds 7, 8, 9b) and are skipped on macOS and Windows, so they
have never passed anywhere.

Nothing here was compiled or run. Every statement about what the app WILL do after this
change is from reading the code; the status is "worked, unproven".

## Verdict

| what | verdict |
|---|---|
| all four cases fail at their FIRST step (no denial line) | **TEST fault**: wrong premise. Agent output never loads an http(s) image, so no request was made and the gate was never asked. By design in the app. |
| later steps of `agent_gate_deny_allow` (no `allow_from_log` / `remove_origin` hop, origin 0 times in `settings.toml`, listener silent) | **TEST fault**, consequences of the first, plus the prompts typed into the settings page (below) |
| "Enters lost 13 vs 2", "lost-before-app 40" | **TEST accounting** (the old count, fixed by 8ba8a3f71): all 13 Enters are in the app log |
| `network_gate_notice`: "the app log names the host or the image URL" | **TEST fault** in round 9b (it matched the case's own typed prompt in the key trace). The same check would have found a real **APP defect** once a request is made: #2014 |
| `agent_gate_off_live`: gate switch by pointer UNVERIFIABLE | **TEST fault**: clicked beside the section heading, not the switch row |
| #1987 ("Allow did not persist; the gate did not refuse an unlisted host") | **not an app bug** on this evidence: no request ever reached the gate, so nothing was refused, there was no toast and no log row to press Allow on |
| new: a remote image's full URL in the app log on a failed or refused fetch | **APP defect**, #2014, changed here |

## Evidence, step by step

Logs: `run_slim/logs/agent@fake/case_*.log` of the round 9b Wayland job; screenshots
`agent_network_gate_notice_FAIL.jpg`, and the round 8 page shots (`agent_agnet_kb_open.jpg`).

1. **The fake model's reply reaches the app.** `case_network_gate_notice.log`:
   `hop=enter_begin ... buffer="Please show this image: http://192.0.2.1/a_..." input_type=AI`,
   `[byop] stream opened OK`, `stream stats: ... chunks=10 (55B)` (55 bytes is exactly
   `Here it is:\n\n![img](<url>)`), `hop=turn ... state=success`.
2. **The app does not fetch the image.** The screenshot shows the answer as the text
   `Here it is:` and `img`: the image's alt text, which is what
   `render_image_section` draws when `can_render_blocklist_image` is false
   (`app/src/ai/blocklist/block/view_impl/common.rs:2385-2398`, `image_fallback_text` :2281).
   It is false because `load_renderable_image_asset` returns `None` for a source that
   `is_supported_blocklist_image_source` rejects (:2322-2326), and that function starts with
   `if source.starts_with("http://") || source.starts_with("https://") { return false; }`
   (:2853-2856). The unit test says so in words: "HTTP / HTTPS sources are intentionally
   rejected regardless of extension" (`common_tests.rs:503`). The code is the same at the
   upstream pin (`git log -L` ends at the initial import).
3. **So the gate is never asked.** No line with `remote-asset` exists in any of the four
   logs. `asset_cache::fetch_file_to_memory` (`crates/asset_cache/src/lib.rs:144`), the only
   caller of the gate for that purpose, is never reached. The 30 s wait for the denial line
   runs out (`06:21:11` Enter, next key `06:21:44`).
4. **The keyboard does reach the page.** `case_agent_gate_deny_allow.log`: `ctrl+,` is
   `keydown_keybound ... WorkspaceAction`, the palette entry runs, Tab and Enter are
   `EditorAction` in the page's origin editor (view 1090), and step 4 (typed origin + Enter)
   logs `hop=network_gate_ui action=add_origin origin_hash=db3f053403ad937a`. "Allow on the
   log row" found no row because nothing had been denied: Tab had no next stop.
5. **`close_settings` does not close the settings tab.** Its two Escapes are
   `keydown_keybound key=escape ... EditorAction` in the same editor; the next prompt's
   letters are `kh_editor_elem_typed ... accepted=true` in that editor and its Enter is
   `core_enter ... view=EntityId(1090) ... EditorAction` with `layout_top=...NetworkPage`
   and no `enter_begin`. Every "show this image" prompt after the first was typed into the
   Network page. (Escape not closing the settings TAB is the known fact
   `cases_netless.scripted_session` already handles.)
6. **No Enter was lost.** The log has 13 `core_enter stage=dispatch_result handled=true`
   lines for the 13 Enters sent; 11 were taken by a binding, which the round 9b build's
   accounting did not count.
7. **Gate switch by pointer.** The page shows "Network gate" twice: the section heading
   (y 149 in the 1600x1000 shot) and the switch row's label (y 216); the switch is at
   x 1425 on the label row. `pointer_gate_switch` took the top-most "gate" word and clicked
   260 to 800 px right of it, on the heading's line.
8. **`network_gate_notice` port mismatch** (never reached): the URL had no port (so 80)
   while the case looked for `192.0.2.1:8099` in `settings.toml`; the toast's Allow writes
   the request's own host and port (`network_gate_log::allow_text`).

## The app defect found while reading (#2014)

A refused or failed remote image is logged by
`crates/warpui_core/src/assets/asset_cache.rs` (the `Err(err)` arm of `load_asynchronously`) as
`Asset fetch failed ({asset_source:?}): {err:#}`. `AssetSource::Async` prints its
`AsyncAssetId`, whose `Debug` printed `key`, and `asset_cache::url_source` uses the URL as
the key. reqwest errors also end in `for url (<url>)`. So the first gate-checked image
request would have put the host, port and path into the app log, against the gate's own
rule (`crates/network_gate/src/lib.rs:25`: a denial never carries the URL; logs show the
purpose and a host hash). Not seen in a run, because no run ever made such a request.

## Chosen fix

**Cases (test).** Make the request where the app really makes one: the markdown viewer.
`show_image` writes a one-image `.md` and hands it to the running app with
`cases.open_file_in_app` (the path `md_viewer` uses and passes on Wayland in both modes).
The viewer lays the image out as a `BlockItem::Image` with
`resolve_asset_source` -> `asset_cache::url_source` (`crates/editor/src/content/edit.rs:91-96,
1068`), `RenderableImage` paints it (`crates/editor/src/render/element/image.rs`), the paint
loads the asset, and the fetch asks the gate. All later oracles are unchanged: the denial
line, the silent or reached listener, `settings.toml`, the `hop=network_gate_ui` lines, the
whole-case strace oracle ("no non-loopback destination" stays, so a leak still fails).

- The viewer tab is closed with the close-pane key only after the app log showed
  `dispatching action for "root_view:add_file_pane"`; without that line nothing is pressed
  (the key closes whatever tab is in front) and the step fails with the reason.
- The cases no longer type a prompt between page steps, so the open settings tab cannot
  swallow one.
- `agent_gate_deny_allow` gains step 0: the model's markdown image must reach no listener
  and produce no `"remote-asset"` line. This keeps the agent in the case and pins the
  by-design behaviour from the outside.
- `pointer_gate_switch` uses the label row (`switch_spots`), judged by the `toggle_gate` hop
  as before. The pointer add falls back to a click right of the editor when OCR misses the
  "Add" label, counted only when the `add_origin` hop shows it.
- `network_gate_notice`: the URL carries `:8099`, the origin it then looks for.
- `host_leaks` removes 16-digit hex hashes before searching a gate line for a port: a
  5-digit port can occur inside a hash by chance. A hash names nothing; a host, port or
  token outside a hash is still a leak.

**App (#2014).** `AsyncAssetId`'s `Debug` prints `key_len` and a `key_hash` fingerprint,
never the key. `fetch_file_to_memory` calls `reqwest::Error::without_url` on its three
reqwest errors.

## Rejected

- **Loading http images in agent output so the old cases work.** It would let a model make
  the app contact any allowed origin by writing an image link; the current behaviour is
  deliberate, pinned by a unit test, and the stricter one. Not a test's call to change.
- **The agent's `webfetch` tool as the trigger.** https only, and it refuses loopback and
  private hosts before the gate (`web_runtime.rs:269-273`), so no local listener can ever
  be reached: the "allowed, reaches the listener" half could not be tested.
- **Closing the settings tab with the close key inside `close_settings`.** If the settings
  page did not open, the key would close the agent tab (or the window, when it is the last
  tab). No log line proves the settings tab is in front. Left as is; the cases no longer
  depend on it, and each of the three strace cases relaunches the app at its end.
- **Redacting only at the log call sites.** Five call sites today; the `Debug` impl is the
  one place every present and future `{source:?}` goes through.
- **Restricting `network_gate_notice`'s whole-log host check to gate lines.** Not needed
  now (nothing types the URL), and the whole-log check is what catches #2014.

## Risk

- Cases: the viewer path is read, not run. If the viewer does not paint the image (or
  the close key does not close the viewer tab), the cases fail loudly at a named step;
  they cannot pass by accident, because every pass still needs the denial line, the
  listener and the hops. A viewer tab left open would show up as the post-case
  "pane not local" probe.
- `network_gate_notice` images b and c (origin allowed, gate off) make the app try to
  connect to the TEST-NET address `192.0.2.1:8099`, as the case always intended (it was
  port 80 before). No strace oracle runs in that case.
- App: `Debug` output of an async asset id changes shape. Nothing parses it (grepped); the
  mermaid and data-URI ids were already hashes. `without_url` only removes text from an
  error message.
- OCR-dependent pointer steps can still end `UNVERIFIABLE` (the toast's "Allow", the
  page's "Remove"); they were never reached before, so their OCR is untested.
