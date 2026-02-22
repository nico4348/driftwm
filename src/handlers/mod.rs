pub mod compositor;
pub mod xdg_shell;

use crate::state::DriftWm;
use smithay::{
    delegate_data_device, delegate_output,
    delegate_seat,
    input::{Seat, SeatHandler, SeatState, pointer::CursorImageStatus},
    reexports::wayland_server::protocol::wl_surface::WlSurface,
    wayland::{
        output::OutputHandler,
        selection::{
            data_device::{
                ClientDndGrabHandler, DataDeviceHandler, DataDeviceState, ServerDndGrabHandler,
            },
            SelectionHandler,
        },
    },
};

// --- SeatHandler ---

impl SeatHandler for DriftWm {
    type KeyboardFocus = WlSurface;
    type PointerFocus = WlSurface;
    type TouchFocus = WlSurface;

    fn seat_state(&mut self) -> &mut SeatState<Self> {
        &mut self.seat_state
    }

    fn cursor_image(&mut self, _seat: &Seat<Self>, _image: CursorImageStatus) {}

    fn focus_changed(&mut self, _seat: &Seat<Self>, _focused: Option<&Self::KeyboardFocus>) {}
}

delegate_seat!(DriftWm);

// --- SelectionHandler ---

impl SelectionHandler for DriftWm {
    type SelectionUserData = ();
}

// --- DataDeviceHandler ---

impl DataDeviceHandler for DriftWm {
    fn data_device_state(&self) -> &DataDeviceState {
        &self.data_device_state
    }
}

impl ClientDndGrabHandler for DriftWm {}
impl ServerDndGrabHandler for DriftWm {}

delegate_data_device!(DriftWm);

// --- OutputHandler ---

impl OutputHandler for DriftWm {}

delegate_output!(DriftWm);
