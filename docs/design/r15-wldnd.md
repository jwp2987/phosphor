# r15-wldnd: a drop target on native Wayland (#2085)

Owner 2026-11-11: "Fix the Wayland drag and drop". Nothing here was compiled or run when written; see "Compile doubts".

## Cause (confirmed by reading, integ/r14 fae4cecfd)

- The app turns a drop into text from `WindowEvent::DroppedFile` only (`crates/warpui/src/windowing/winit/event_loop/mod.rs:1916`): the paths are collected, debounced, and sent as `Event::DragAndDropFiles { paths, location }` with `location = last_cursor_position` (`:2010`). `HoveredFile` / `HoveredFileCancelled` are not used by the app. The receivers check the location against their bounds (`app/src/terminal/terminal_size_element.rs:93`, `app/src/editor/view/element.rs:2328`).
- The vendored winit (`lib/winit`, 0.30.13) emits those events from `platform_impl/linux/x11/event_processor.rs:475-572` (XDND) and `platform_impl/windows/drop_handler.rs`. `platform_impl/linux/wayland/` had no `wl_data_device` code: no manager bind in `state.rs`, nothing per seat in `seat/mod.rs`. A compositor therefore had no data device of ours to send `enter` / `drop` to, and the source showed "no drop".
- Wayland stack in `Cargo.lock`: smithay-client-toolkit 0.19.2 (feature `calloop`), wayland-client 0.31.15, wayland-backend 0.3.17 (`client_system`), calloop 0.13.0. SCTK 0.19.2 has `data_device_manager` (`DataDeviceManagerState`, `DataDeviceHandler`, `DataOfferHandler`, `DragOffer::{accept_mime_type, set_actions, receive, finish, destroy}`, `ReadPipe` as a calloop source); its `examples/data_device.rs` is the shape followed.
- The clipboard is not involved: winit has none, and the app's clipboard (`lib/arboard` with `wl-clipboard-rs`, data-control protocol) opens its own connection. There was no data device on winit's connection before this change, so no existing selection routing can be disturbed; the new device's `selection` handler does nothing.

## Fix

`lib/winit/src/platform_impl/linux/wayland/dnd/`:

- `logic.rs` (only `std`): the kill switch, the mime choice, the uri-list parser, and `Drag`, the order of events of one accepted drag as a small state machine that returns `Effect`s. All unit tests are here.
- `mod.rs`: the SCTK glue on `WinitState`. `DndState` (in `WinitState::dnd`, `None` when off) binds `wl_data_device_manager` (v1-3) and holds one `DataDevice` per seat (created in `WinitState::new` for the seats present and in `SeatHandler::new_seat`; released in `remove_seat`).

One drag:

1. `enter`: the surface is mapped to its window (a decoration subsurface maps to its parent, as the pointer code does). If the offer has `text/uri-list` (exact spelling first, else a spelling with parameters or other case; the source's string is what is sent back), `accept(enter serial, mime)` and `set_actions(copy, copy)`; otherwise `accept(serial, None)`. The data is asked for at once (`receive`, a pipe) because `HoveredFile` carries a path and the paths are only in the data.
2. The pipe's read end is a calloop source on winit's loop (`ReadPipe`, level-triggered). Each readiness does ONE `read` (the descriptor is blocking; a second read could wait for the source), so the UI thread never blocks. At the end of the data: parse, emit `HoveredFile` per path if the drag still hovers.
3. `leave` without a drop: `HoveredFileCancelled`. The read, if still running, goes on to its end and reports nothing (a pipe is never closed early: a source must not get SIGPIPE from us).
4. `drop`: if the paths are known, emit now; if the read is still running, emit when it ends. A drop whose answer held no file asks ONE more time (a source that only gives data after the drop), then gives up. Emitted: one `CursorMoved` at the drop position, then `DroppedFile` per path. Then `finish` (only when files were taken and the compositor's selected action is copy: `finish` without an action is a protocol error) and `destroy`.
5. The 1 MiB cap and read errors make the data count as empty; the pipe is still drained.

Kill switch: `PHOSPHOR_WAYLAND_DND=0` (exactly `0`). `DndState::new` returns `None`: no bind, no data device, the handlers return at once. Behaviour as before.

Logging: `log` crate, info, `wayland_dnd hop=...` (counts, never a path). `log` moves from winit's Windows-only dependencies to the unix ones as well; winit's lock entry already lists it.

## Decisions inside the fix

- **`CursorMoved` at the drop position.** Not sent on X11, added here on purpose. During a drag the compositor sends no pointer events to the target, so the app's `last_cursor_position` is wherever the pointer last was over the window (in the app case: where it crossed toward the drag source), and the app refuses a drop whose location is outside the element. With the event, the drop lands in the pane under the pointer. It is only sent when the drag is over the window's own surface (decoration coordinates are not window coordinates).
- **Copy only.** A terminal inserts the path and does not take the file. Offering move could let a source delete its file after `finish`.
- **`text/uri-list` only.** `text/plain` with `file://` lines (the task's optional part) is refused: accepting every text drag would show "drop possible" for dragged text and then insert nothing. X11 accepts the same single type.
- **No `canonicalize`.** X11's parser resolves each path against the file system and drops the whole list if one fails. Here the path is the decoded URI: no blocking file-system call on the UI thread, symlinks stay as dragged, and one bad entry does not lose the others.
- **Other hosts are skipped**, `localhost` and the empty host are local (task). The machine's own host name is not compared (that needs a `gethostname` call the pure parser does not make); such a URI is skipped.
- **Reading on enter, not on drop.** Needed for `HoveredFile(path)`; the answer is reused for the drop, so the drop itself usually completes without waiting.

## Rejected

- A reader thread per drop: needs a channel and a loop wake-up; the calloop source does the same with no thread.
- Making the pipe non-blocking and looping `read` to `WouldBlock`: one read per level-triggered readiness is as safe and needs no `fcntl`.
- A give-up timer for a source that never closes its pipe: the idle descriptor costs nothing and blocks nothing; a later drag is independent of it.
- SCTK's `delegate_data_device!`: it also delegates `wl_data_source`, which needs a `DataSourceHandler` for a drag-source role winit does not have. Three `delegate_dispatch!` lines (manager, offer, device) are used instead.
- Changing the app (`event_loop/mod.rs`): not needed, it already handles `DroppedFile`.

## Tests

- Unit: 20 tests in `dnd/logic.rs`. Because `lib/winit` is a separate workspace that no CI job tests, the file is also compiled into `warpui`'s tests (`#[path]` in `crates/warpui/src/windowing/winit/linux/mod.rs`, `#[cfg(test)]`), so they run in the ordinary unit suite.
- App: `drag_drop_file` (PROVING) no longer announces a Wayland failure; on Wayland it also requires that the GTK source is not told `failed`, and it reports the app's `wayland_dnd` hops. X11 expectations are unchanged.
- Not covered without a compositor: everything in `dnd/mod.rs`.

## Risk

- New protocol code on the UI connection. A wrong request would be a protocol error, which ends the connection (the app exits). The two requests with such rules are guarded: `finish` (only after a drop, with an accepted mime and the copy action selected) and nothing is sent on an offer after `finish` except `destroy`. Kill switch if a compositor disagrees.
- Reading the data on every hover makes the source produce its uri-list when the pointer merely crosses the window. GTK and Qt sources serve that; other targets (browsers) do the same.
- If a compositor delivers a drag to only one of a client's data devices this is still fine: winit's is the only one on this connection.
- A drop on the client-side title bar (GNOME) inserts the files without the `CursorMoved`, i.e. at the app's last pointer position.

## Compile doubts (nothing was compiled)

1. `delegate_dispatch!` for `WlDataOffer` / `WlDataDevice` / `WlDataDeviceManager` to `DataDeviceManagerState` without the `WlDataSource` line: the SCTK impls' bounds (`DataDeviceHandler + DataOfferHandler + 'static`) are read as satisfied; unverified.
2. The calloop callback `move |_, file, state: &mut WinitState| state.dnd_read_ready(id, &**file)`: `file` is `&mut NoIoDrop<File>`, `&**file` is `&File`; reading uses `impl Read for &File`.
3. `logic.rs` is compiled twice: edition 2021 in winit (MSRV 1.70 in its manifest; `trim_ascii` was avoided for that) and edition 2024 in `warpui`'s tests. The `#[path]` has six `..` from `crates/warpui/src/windowing/winit/linux/`.
4. `log::` macros in winit's unix code rely on the `log = "0.4"` line added to the unix target dependencies of `lib/winit/Cargo.toml`.
5. Borrow shapes in `dnd_apply` (a `&mut` into `self.dnd` alive while `self.events_sink` / `self.windows` are used: distinct fields, read as fine).
