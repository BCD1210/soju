//! Main-thread host window; guest execution yields in bounded slices for event polling.
use crate::{
    ipc::{router::Hub, types::Frame},
    nro::Nro,
    runtime::{Runtime, Stop},
};
use soju_switch_input::{
    NpadState,
    mapping::{self, FaceLayout, Gamepad},
};
use std::{
    ffi::CStr,
    path::Path,
    time::{Duration, Instant},
};
unsafe extern "C" {
    fn soju_host_open() -> i32;
    fn soju_host_close();
    fn soju_host_error() -> *const std::ffi::c_char;
    fn soju_host_present(pixels: *const u8, width: i32, height: i32) -> i32;
    fn soju_host_poll(keys: *mut u8, pad: *mut f32) -> i32;
}
struct Window;
impl Window {
    fn new() -> Result<Self, String> {
        let window = Self;
        check(unsafe { soju_host_open() })?;
        Ok(window)
    }
    fn poll(&self) -> (bool, NpadState) {
        let mut keys = [0; 128];
        let mut pad = [0.; 22];
        let quit = unsafe { soju_host_poll(keys.as_mut_ptr(), pad.as_mut_ptr()) } != 0;
        let pressed: Vec<_> = keys
            .iter()
            .enumerate()
            .filter(|(_, v)| **v != 0)
            .map(|(i, _)| i as u16)
            .collect();
        (quit, combine(&pressed, &pad))
    }
    fn present(&self, frame: &Frame) -> Result<(), String> {
        if frame.width == 0
            || frame.height == 0
            || frame.width > 4096
            || frame.height > 2160
            || frame.pixels.len() != frame.width as usize * frame.height as usize * 4
        {
            return Err("invalid host frame".into());
        }
        check(unsafe {
            soju_host_present(
                frame.pixels.as_ptr(),
                frame.width as i32,
                frame.height as i32,
            )
        })
    }
}
impl Drop for Window {
    fn drop(&mut self) {
        unsafe { soju_host_close() };
    }
}
fn check(code: i32) -> Result<(), String> {
    if code == 0 {
        Ok(())
    } else {
        Err(unsafe { CStr::from_ptr(soju_host_error()) }
            .to_string_lossy()
            .into_owned())
    }
}
fn combine(keys: &[u16], p: &[f32; 22]) -> NpadState {
    let g = Gamepad {
        south: p[0] > 0.5,
        east: p[1] > 0.5,
        west: p[2] > 0.5,
        north: p[3] > 0.5,
        left_shoulder: p[4] > 0.5,
        right_shoulder: p[5] > 0.5,
        left_trigger: p[6],
        right_trigger: p[7],
        left_thumb: p[8] > 0.5,
        right_thumb: p[9] > 0.5,
        menu: p[10] > 0.5,
        options: p[11] > 0.5,
        dpad_up: p[12] > 0.5,
        dpad_down: p[13] > 0.5,
        dpad_left: p[14] > 0.5,
        dpad_right: p[15] > 0.5,
        lx: p[16],
        ly: p[17],
        rx: p[18],
        ry: p[19],
    };
    let keyboard = mapping::from_keyboard(keys);
    let mut state = mapping::from_gamepad(&g, FaceLayout::ByPosition);
    state.buttons |= keyboard.buttons;
    if keyboard.left != Default::default() {
        state.left = keyboard.left;
    }
    if keyboard.right != Default::default() {
        state.right = keyboard.right;
    }
    state
}
pub fn play(path: &Path, home: &Path) -> Result<(), String> {
    let nro = Nro::read(&mut std::fs::File::open(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let boot = nro.protected_homebrew().map_err(|e| e.to_string())?;
    let mut runtime = Runtime::new(&boot).map_err(|e| e.to_string())?;
    let mut hub = Hub::new(path, home).map_err(|e| e.to_string())?;
    runtime.kernel_mut().program_id = hub.program_id();
    let window = Window::new()?;
    let mut closing = None;
    loop {
        let (quit, input) = window.poll();
        hub.set_input(input);
        if quit && closing.is_none() {
            let handled = hub
                .request_exit(runtime.kernel_mut())
                .map_err(|e| e.to_string())?;
            if !handled {
                break;
            }
            closing = Some(Instant::now());
        }
        match runtime
            .run(&mut hub, Duration::from_millis(8), 10_000)
            .map_err(|e| e.to_string())?
        {
            Stop::BudgetExpired => {}
            Stop::ProcessExited(0) | Stop::AllThreadsExited => break,
            stop => return Err(format!("guest stopped: {stop:?}")),
        }
        if let Some(frame) = hub.take_frame() {
            window.present(&frame)?;
        }
        if closing.is_some_and(|start| start.elapsed() > Duration::from_secs(5)) {
            return Err("guest did not respond to exit request within five seconds".into());
        }
    }
    hub.finish().map_err(|e| e.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn keyboard_and_gamecontroller_share_npad_mapping() {
        let mut pad = [0.; 22];
        pad[1] = 1.;
        pad[16] = 1.;
        let state = combine(&[mapping::keys::UP], &pad);
        assert_ne!(state.buttons & soju_switch_input::button::A, 0);
        assert_ne!(state.buttons & soju_switch_input::button::UP, 0);
        assert_eq!(state.left.x, 32767);
        assert_eq!(combine(&[], &[0.; 22]), NpadState::default());
    }
}
