use winit::event_loop::{EventLoop, EventLoopProxy};

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Debug)]
pub enum DomControlsUserEvent {
    AButtonPressed,
    AButtonReleased,
    BButtonPressed,
    BButtonReleased,
    PitchYawJoystickMoved { vector: (f64, f64) },
    PitchYawJoystickReleased,
    TranslationJoystickMoved { vector: (f64, f64) },
    TranslationJoystickReleased,
    WindowResized { size: winit::dpi::LogicalSize<u32> },
}

struct EventLoopGlobalState {
    event_loop_proxy: Option<EventLoopProxy<DomControlsUserEvent>>,
}
static mut EVENT_LOOP_GLOBAL_STATE: EventLoopGlobalState = EventLoopGlobalState {
    event_loop_proxy: None,
};

pub unsafe fn set_global_event_loop_proxy(event_loop: &EventLoop<DomControlsUserEvent>) {
    EVENT_LOOP_GLOBAL_STATE.event_loop_proxy = Some(event_loop.create_proxy());
}

fn send_dom_controls_user_event(event: DomControlsUserEvent) {
    let event_loop_proxy = unsafe {
        match EVENT_LOOP_GLOBAL_STATE.event_loop_proxy {
            None => return,
            _ => EVENT_LOOP_GLOBAL_STATE.event_loop_proxy.as_ref().unwrap(),
        }
    };

    event_loop_proxy
        .send_event(event)
        .expect("Failed to queue DOM button event");
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn a_button_pressed() {
    send_dom_controls_user_event(DomControlsUserEvent::AButtonPressed);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn a_button_released() {
    send_dom_controls_user_event(DomControlsUserEvent::AButtonReleased);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn b_button_pressed() {
    send_dom_controls_user_event(DomControlsUserEvent::BButtonPressed);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn b_button_released() {
    send_dom_controls_user_event(DomControlsUserEvent::BButtonReleased);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn pitch_yaw_joystick_moved(x: f64, y: f64) {
    send_dom_controls_user_event(DomControlsUserEvent::PitchYawJoystickMoved { vector: (x, y) });
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn pitch_yaw_joystick_released() {
    send_dom_controls_user_event(DomControlsUserEvent::PitchYawJoystickReleased);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn translation_joystick_moved(x: f64, y: f64) {
    send_dom_controls_user_event(DomControlsUserEvent::TranslationJoystickMoved { vector: (x, y) });
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn translation_joystick_released() {
    send_dom_controls_user_event(DomControlsUserEvent::TranslationJoystickReleased);
}
#[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
pub fn web_window_resized(width: u32, height: u32) {
    send_dom_controls_user_event(DomControlsUserEvent::WindowResized {
        size: winit::dpi::LogicalSize { width, height },
    });
}
