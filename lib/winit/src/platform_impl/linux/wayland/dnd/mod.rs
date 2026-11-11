//! Drop target for files dragged onto a window (Phosphor #2085).
//!
//! Upstream winit emits `HoveredFile` / `HoveredFileCancelled` / `DroppedFile` on X11, Windows
//! and macOS only. This is the Wayland side: one `wl_data_device` per seat, a drag that offers
//! `text/uri-list` is accepted with the copy action, its data is read through a pipe on the
//! event loop (one `read` per readiness, never a blocking read) and the same three events are
//! emitted. `logic.rs` holds everything that needs no compositor.
//!
//! The data device also receives the seat's selection (clipboard) offers. They are tracked by
//! SCTK and never read here: winit has no clipboard, and Phosphor's clipboard uses its own
//! connection.
//!
//! Kill switch: `PHOSPHOR_WAYLAND_DND=0` (no `wl_data_device_manager` bind, no data device).

use std::collections::VecDeque;
use std::fs::File;
use std::io::{ErrorKind, Read};

use ahash::AHashMap;

use sctk::compositor::SurfaceData;
use sctk::data_device_manager::data_device::{DataDevice, DataDeviceData, DataDeviceHandler};
use sctk::data_device_manager::data_offer::{DataOfferData, DataOfferHandler, DragOffer};
use sctk::data_device_manager::DataDeviceManagerState;
use sctk::globals::GlobalData;
use sctk::reexports::calloop::PostAction;
use sctk::reexports::client::backend::ObjectId;
use sctk::reexports::client::globals::GlobalList;
use sctk::reexports::client::protocol::wl_data_device::WlDataDevice;
use sctk::reexports::client::protocol::wl_data_device_manager::{DndAction, WlDataDeviceManager};
use sctk::reexports::client::protocol::wl_data_offer::WlDataOffer;
use sctk::reexports::client::protocol::wl_seat::WlSeat;
use sctk::reexports::client::protocol::wl_surface::WlSurface;
use sctk::reexports::client::{delegate_dispatch, Connection, Proxy, QueueHandle};

use crate::dpi::{LogicalPosition, PhysicalPosition};
use crate::event::WindowEvent;
use crate::platform_impl::wayland::state::WinitState;
use crate::platform_impl::wayland::{self, DeviceId, WindowId};

mod logic;

use logic::{Drag, Effect};

/// The drop-target state of the Wayland backend. Absent (`WinitState::dnd` is `None`) when the
/// kill switch is set or the compositor has no `wl_data_device_manager`.
#[derive(Debug)]
pub struct DndState {
    manager: DataDeviceManagerState,

    /// The data device of each seat, by the seat's id. Dropping one releases it.
    devices: AHashMap<ObjectId, DataDevice>,

    /// The accepted drags that are not over yet: at most one hovering per data device, plus the
    /// ones whose pipe is still being read.
    drags: Vec<ActiveDrag>,

    next_drag_id: u64,
}

#[derive(Debug)]
struct ActiveDrag {
    /// What the pipe callbacks find this drag by.
    id: u64,

    /// The `wl_data_device` the drag came through.
    device: ObjectId,

    /// The offer as it was on enter. Only used to send requests and to compare.
    offer: DragOffer,

    /// The offered mime type that was accepted, in the source's spelling.
    mime: String,

    window_id: WindowId,

    /// The drag is over the window's own surface (not a decoration subsurface), so its position
    /// is in the window's coordinates.
    on_window_surface: bool,

    /// The last position of the drag, in surface-local logical coordinates.
    position: (f64, f64),

    /// The action the compositor selected last (`wl_data_offer.action`).
    action: DndAction,

    core: Drag,

    /// The bytes read from the current pipe so far.
    buf: Vec<u8>,

    /// The current pipe's data is not usable (too large, or a read error): it is still read to
    /// its end and then counts as empty.
    unusable: bool,
}

impl DndState {
    pub fn new(
        globals: &GlobalList,
        queue_handle: &QueueHandle<WinitState>,
        seats: impl Iterator<Item = WlSeat>,
    ) -> Option<Self> {
        if !logic::dnd_enabled(std::env::var_os(logic::KILL_SWITCH_ENV).as_deref()) {
            log::info!("wayland_dnd hop=off reason=kill_switch");
            return None;
        }

        let manager = match DataDeviceManagerState::bind(globals, queue_handle) {
            Ok(manager) => manager,
            Err(error) => {
                log::warn!("wayland_dnd hop=off reason=no_data_device_manager error={error}");
                return None;
            },
        };

        let mut state =
            Self { manager, devices: Default::default(), drags: Vec::new(), next_drag_id: 0 };
        for seat in seats {
            state.add_seat(queue_handle, &seat);
        }

        Some(state)
    }

    pub fn add_seat(&mut self, queue_handle: &QueueHandle<WinitState>, seat: &WlSeat) {
        let device = self.manager.get_data_device(queue_handle, seat);
        self.devices.insert(seat.id(), device);
    }

    /// Forget the seat's data device and hand it back: the caller ends a hover that came through
    /// it, then drops it (which releases it). A drag that is still being read ends with its pipe.
    pub fn remove_seat(&mut self, seat: &ObjectId) -> Option<DataDevice> {
        self.devices.remove(seat)
    }

    fn drag_mut(&mut self, id: u64) -> Option<&mut ActiveDrag> {
        self.drags.iter_mut().find(|drag| drag.id == id)
    }

    /// The id of the drag that hovers through `device`, if one does.
    fn hovering(&self, device: &WlDataDevice) -> Option<u64> {
        let device = device.id();
        self.drags
            .iter()
            .find(|drag| drag.device == device && drag.core.is_hovering())
            .map(|drag| drag.id)
    }
}

impl DataDeviceHandler for WinitState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
        surface: &WlSurface,
    ) {
        // A hover whose leave never came ends here.
        self.dnd_leave(data_device);

        if self.dnd.is_none() {
            return;
        }

        // A drag that started in this client carries no offer.
        let offer = match data_device.data::<DataDeviceData>().and_then(|data| data.drag_offer()) {
            Some(offer) => offer,
            None => return,
        };

        // The window the surface belongs to; a decoration is a subsurface of it.
        let parent_surface = surface
            .data::<SurfaceData>()
            .and_then(|data| data.parent_surface())
            .unwrap_or(surface);
        let window_id = wayland::make_wid(parent_surface);
        let on_window_surface = parent_surface == surface;

        let mime = if self.windows.get_mut().contains_key(&window_id) {
            offer.with_mime_types(logic::choose_mime)
        } else {
            None
        };

        let mime = match mime {
            Some(mime) => mime,
            None => {
                // Nothing here takes this drag: say so, the source shows "no drop".
                offer.accept_mime_type(offer.serial, None);
                log::info!("wayland_dnd hop=enter accepted=false");
                return;
            },
        };

        offer.accept_mime_type(offer.serial, Some(mime.clone()));
        // Copy only: the path is inserted as text and the file is not taken, so a move (after
        // which a source may delete its file) must never be agreed to.
        offer.set_actions(DndAction::Copy, DndAction::Copy);

        let id = match self.dnd.as_mut() {
            Some(dnd) => {
                dnd.next_drag_id += 1;
                dnd.next_drag_id
            },
            None => return,
        };

        // The paths are only known once the data is read: ask for it now, `HoveredFile` follows
        // when the pipe ends.
        let reading = self.dnd_start_read(id, &offer, &mime);
        log::info!("wayland_dnd hop=enter accepted=true reading={reading}");

        if let Some(dnd) = self.dnd.as_mut() {
            dnd.drags.push(ActiveDrag {
                id,
                device: data_device.id(),
                offer,
                mime,
                window_id,
                on_window_surface,
                position: (x, y),
                action: DndAction::empty(),
                core: Drag::new(reading),
                buf: Vec::new(),
                unusable: false,
            });
        }
    }

    fn leave(&mut self, _: &Connection, _: &QueueHandle<Self>, data_device: &WlDataDevice) {
        self.dnd_leave(data_device);
    }

    fn motion(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &WlDataDevice,
        x: f64,
        y: f64,
    ) {
        let dnd = match self.dnd.as_mut() {
            Some(dnd) => dnd,
            None => return,
        };

        if let Some(id) = dnd.hovering(data_device) {
            if let Some(drag) = dnd.drag_mut(id) {
                drag.position = (x, y);
            }
        }
    }

    fn selection(&mut self, _: &Connection, _: &QueueHandle<Self>, _: &WlDataDevice) {
        // The clipboard is not winit's business; SCTK keeps (and replaces) the offer object.
    }

    fn drop_performed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        data_device: &WlDataDevice,
    ) {
        let dnd = match self.dnd.as_mut() {
            Some(dnd) => dnd,
            None => return,
        };

        let id = match dnd.hovering(data_device) {
            Some(id) => id,
            // A drop of a drag that was refused on enter: nothing to take.
            None => return,
        };

        let effects = match dnd.drag_mut(id) {
            Some(drag) => drag.core.on_drop(),
            None => return,
        };
        log::info!("wayland_dnd hop=drop waiting_for_data={}", effects.is_empty());
        self.dnd_apply(id, effects);
    }
}

impl DataOfferHandler for WinitState {
    // NOTE: SCTK calls both of these with the offer's own data locked. Nothing here may ask SCTK
    // about the offer (`with_mime_types`, `DataDeviceData::drag_offer`): that would deadlock.

    fn source_actions(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &mut DragOffer,
        _: DndAction,
    ) {
        // The actions were set on enter and do not depend on what the source allows: if it does
        // not allow a copy, the compositor selects no action and the drop does not happen.
    }

    fn selected_action(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        offer: &mut DragOffer,
        action: DndAction,
    ) {
        let dnd = match self.dnd.as_mut() {
            Some(dnd) => dnd,
            None => return,
        };

        let offer: &WlDataOffer = offer.inner();
        if let Some(drag) = dnd.drags.iter_mut().find(|drag| drag.offer.inner() == offer) {
            drag.action = action;
        }
    }
}

impl WinitState {
    /// The drag that hovers through `data_device` (if any) is over without a drop.
    pub(super) fn dnd_leave(&mut self, data_device: &WlDataDevice) {
        let dnd = match self.dnd.as_mut() {
            Some(dnd) => dnd,
            None => return,
        };

        // After a drop the leave means nothing (the drag is no longer "hovering"); SCTK keeps a
        // dropped offer alive, and destroys one that left without a drop.
        let id = match dnd.hovering(data_device) {
            Some(id) => id,
            None => return,
        };

        let effects = match dnd.drag_mut(id) {
            Some(drag) => drag.core.on_leave(),
            None => return,
        };
        log::info!("wayland_dnd hop=leave");
        self.dnd_apply(id, effects);
    }

    /// Ask the source for the data of `offer` and read it on the event loop. `false`: the data
    /// could not be asked for (no pipe, or the loop refused the source); nothing is being read.
    fn dnd_start_read(&mut self, id: u64, offer: &DragOffer, mime: &str) -> bool {
        let read_pipe = match offer.receive(mime.to_owned()) {
            Ok(read_pipe) => read_pipe,
            Err(error) => {
                log::warn!("wayland_dnd hop=receive_failed error={error}");
                return false;
            },
        };

        // The request (with the pipe's write end) is flushed by the event loop before it waits
        // again; the pipe's read end is level-triggered, so every callback has something to read
        // or the end of the data.
        let inserted = self.loop_handle.insert_source(
            read_pipe,
            move |_, file, state: &mut WinitState| state.dnd_read_ready(id, &**file),
        );

        match inserted {
            Ok(_) => true,
            Err(error) => {
                log::warn!("wayland_dnd hop=receive_failed error={}", error.error);
                false
            },
        }
    }

    /// The pipe of drag `id` is readable or at its end. One `read` per call: the descriptor is a
    /// blocking one, and a second read could wait for the source.
    fn dnd_read_ready(&mut self, id: u64, mut file: &File) -> PostAction {
        let drag = match self.dnd.as_mut().and_then(|dnd| dnd.drag_mut(id)) {
            Some(drag) => drag,
            // The drag is gone; dropping the source closes the pipe.
            None => return PostAction::Remove,
        };

        let mut chunk = [0u8; 8192];
        match file.read(&mut chunk) {
            Ok(0) => (),
            Ok(read) => {
                if drag.unusable {
                    // Read on to the end, keep nothing.
                } else if drag.buf.len() + read > logic::MAX_URI_LIST_BYTES {
                    log::warn!("wayland_dnd hop=data_too_large");
                    drag.unusable = true;
                    drag.buf = Vec::new();
                } else {
                    drag.buf.extend_from_slice(&chunk[..read]);
                }
                return PostAction::Continue;
            },
            Err(error) if matches!(error.kind(), ErrorKind::Interrupted | ErrorKind::WouldBlock) => {
                return PostAction::Continue;
            },
            Err(error) => {
                log::warn!("wayland_dnd hop=read_failed error={error}");
                drag.unusable = true;
            },
        }

        // The end of the data.
        let data = std::mem::take(&mut drag.buf);
        let unusable = std::mem::take(&mut drag.unusable);
        let usable: &[u8] = if unusable { &[] } else { &data };
        let effects = drag.core.on_data(usable);
        log::info!("wayland_dnd hop=data bytes={} usable={}", data.len(), !unusable);
        self.dnd_apply(id, effects);

        PostAction::Remove
    }

    /// Carry out what `logic::Drag` decided for drag `id`, in order, then forget the drag if it
    /// is over.
    fn dnd_apply(&mut self, id: u64, effects: Vec<Effect>) {
        let mut effects = VecDeque::from(effects);

        while let Some(effect) = effects.pop_front() {
            let drag = match self.dnd.as_mut().and_then(|dnd| dnd.drag_mut(id)) {
                Some(drag) => drag,
                None => return,
            };
            let window_id = drag.window_id;

            match effect {
                Effect::HoveredFile(path) => {
                    self.events_sink.push_window_event(WindowEvent::HoveredFile(path), window_id);
                },
                Effect::HoveredFileCancelled => {
                    self.events_sink.push_window_event(WindowEvent::HoveredFileCancelled, window_id);
                },
                Effect::DropPosition => {
                    // No pointer event reaches a surface during a drag, so the user of the
                    // window still has the position from before it. Report where the drop is.
                    let (x, y) = drag.position;
                    if !drag.on_window_surface {
                        continue;
                    }
                    let scale_factor = match self.windows.get_mut().get(&window_id) {
                        Some(window) => window.lock().unwrap().scale_factor(),
                        None => continue,
                    };
                    let position: PhysicalPosition<f64> =
                        LogicalPosition::new(x, y).to_physical(scale_factor);
                    let device_id =
                        crate::event::DeviceId(crate::platform_impl::DeviceId::Wayland(DeviceId));
                    self.events_sink.push_window_event(
                        WindowEvent::CursorMoved { device_id, position },
                        window_id,
                    );
                },
                Effect::DroppedFile(path) => {
                    self.events_sink.push_window_event(WindowEvent::DroppedFile(path), window_id);
                },
                Effect::RequestData => {
                    let offer = drag.offer.clone();
                    let mime = drag.mime.clone();
                    if !self.dnd_start_read(id, &offer, &mime) {
                        // Nothing is being read: the drag gets its (empty) answer now.
                        if let Some(drag) = self.dnd.as_mut().and_then(|dnd| dnd.drag_mut(id)) {
                            effects.extend(drag.core.on_data(&[]));
                        }
                    }
                },
                Effect::Finish { accepted } => {
                    // `finish` is a protocol error unless a mime type was accepted (it was, on
                    // enter) and an action is selected; without it the destroy alone tells the
                    // source that the drop was not used. Before version 3 there is neither.
                    let finished = accepted && drag.action.contains(DndAction::Copy);
                    if finished {
                        drag.offer.finish();
                    }
                    drag.offer.destroy();
                    log::info!("wayland_dnd hop=finish accepted={accepted} finished={finished}");
                },
            }
        }

        if let Some(dnd) = self.dnd.as_mut() {
            dnd.drags.retain(|drag| !drag.core.is_done());
        }

        // The events above did not come through the Wayland source when a pipe delivered them:
        // make sure the loop hands them to the user now.
        self.dispatched_events = true;
    }
}

delegate_dispatch!(WinitState: [WlDataDeviceManager: GlobalData] => DataDeviceManagerState);
delegate_dispatch!(WinitState: [WlDataOffer: DataOfferData] => DataDeviceManagerState);
delegate_dispatch!(WinitState: [WlDataDevice: DataDeviceData] => DataDeviceManagerState);
