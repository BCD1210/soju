//! Mapping Mac input to the Switch layout.
//!
//! Gamepads are described by face-button *position* (south/east/west/north, the way
//! macOS GameController reports buttonA/B/X/Y on Xbox and PlayStation pads). Two styles:
//! - `ByPosition` (default for Xbox/PlayStation): the right face button is Switch A and
//!   the bottom one is B, so muscle memory from the Switch itself works.
//! - `ByLabel`: the button labelled A is Switch A (some players prefer this on Xbox).
//!
//! Nintendo's own Pro Controller already has Switch positions and labels.

use crate::button::*;
use crate::{stick_from_axes, NpadState};

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Gamepad {
    pub south: bool,
    pub east: bool,
    pub west: bool,
    pub north: bool,
    pub left_shoulder: bool,
    pub right_shoulder: bool,
    /// Triggers 0.0..=1.0
    pub left_trigger: f32,
    pub right_trigger: f32,
    pub left_thumb: bool,
    pub right_thumb: bool,
    pub menu: bool,
    pub options: bool,
    pub dpad_up: bool,
    pub dpad_down: bool,
    pub dpad_left: bool,
    pub dpad_right: bool,
    /// Sticks -1.0..=1.0, y up positive.
    pub lx: f32,
    pub ly: f32,
    pub rx: f32,
    pub ry: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FaceLayout {
    #[default]
    ByPosition,
    ByLabel,
    NintendoPro,
}

pub const TRIGGER_PRESS: f32 = 0.35;
pub const DEAD_ZONE: f32 = 0.12;

pub fn from_gamepad(g: &Gamepad, layout: FaceLayout) -> NpadState {
    let mut b = 0u64;
    let mut set = |on: bool, bit: u64| if on { b |= bit };
    match layout {
        // Switch: A right (east), B bottom (south), X top (north), Y left (west).
        FaceLayout::ByPosition | FaceLayout::NintendoPro => {
            set(g.east, A);
            set(g.south, B);
            set(g.north, X);
            set(g.west, Y);
        }
        // Xbox labels: A bottom, B right, X left, Y top.
        FaceLayout::ByLabel => {
            set(g.south, A);
            set(g.east, B);
            set(g.west, X);
            set(g.north, Y);
        }
    }
    set(g.left_shoulder, L);
    set(g.right_shoulder, R);
    set(g.left_trigger >= TRIGGER_PRESS, ZL);
    set(g.right_trigger >= TRIGGER_PRESS, ZR);
    set(g.left_thumb, STICK_L);
    set(g.right_thumb, STICK_R);
    set(g.menu, PLUS);
    set(g.options, MINUS);
    set(g.dpad_up, UP);
    set(g.dpad_down, DOWN);
    set(g.dpad_left, LEFT);
    set(g.dpad_right, RIGHT);
    NpadState { buttons: b, left: stick_from_axes(g.lx, g.ly, DEAD_ZONE), right: stick_from_axes(g.rx, g.ry, DEAD_ZONE) }.with_stick_buttons()
}

/// Default keyboard layout (macOS virtual key codes from Carbon's Events.h):
/// arrows = D-pad, WASD = left stick, IJKL = right stick, X/Z/C/V = A/B/X/Y,
/// Q/E = L/R, 1/3 = ZL/ZR, Return = +, Delete = -, Tab/Space = stick clicks.
pub mod keys {
    pub const A: u16 = 0x00;
    pub const S: u16 = 0x01;
    pub const D: u16 = 0x02;
    pub const Q: u16 = 0x0C;
    pub const W: u16 = 0x0D;
    pub const E: u16 = 0x0E;
    pub const R: u16 = 0x0F;
    pub const Z: u16 = 0x06;
    pub const X: u16 = 0x07;
    pub const C: u16 = 0x08;
    pub const V: u16 = 0x09;
    pub const I: u16 = 0x22;
    pub const J: u16 = 0x26;
    pub const K: u16 = 0x28;
    pub const L: u16 = 0x25;
    pub const RETURN: u16 = 0x24;
    pub const TAB: u16 = 0x30;
    pub const SPACE: u16 = 0x31;
    pub const DELETE: u16 = 0x33;
    pub const ONE: u16 = 0x12;
    pub const THREE: u16 = 0x14;
    pub const LEFT: u16 = 0x7B;
    pub const RIGHT: u16 = 0x7C;
    pub const DOWN: u16 = 0x7D;
    pub const UP: u16 = 0x7E;
}

/// Keyboard -> Npad. `pressed` holds macOS key codes currently down.
pub fn from_keyboard(pressed: &[u16]) -> NpadState {
    use keys as k;
    let down = |c: u16| pressed.contains(&c);
    let mut b = 0u64;
    for (code, bit) in [
        (k::X, A), (k::Z, B), (k::C, X), (k::V, Y),
        (k::Q, L), (k::E, R), (k::ONE, ZL), (k::THREE, ZR),
        (k::RETURN, PLUS), (k::DELETE, MINUS), (k::TAB, STICK_L), (k::SPACE, STICK_R),
        (k::UP, UP), (k::DOWN, DOWN), (k::LEFT, LEFT), (k::RIGHT, RIGHT),
    ] {
        if down(code) {
            b |= bit;
        }
    }
    let axis = |neg: u16, pos: u16| (down(pos) as i32 - down(neg) as i32) as f32;
    let (lx, ly) = (axis(k::A, k::D), axis(k::S, k::W));
    let (rx, ry) = (axis(k::J, k::L), axis(k::K, k::I));
    NpadState { buttons: b, left: stick_from_axes(lx, ly, 0.0), right: stick_from_axes(rx, ry, 0.0) }.with_stick_buttons()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xbox_by_position_puts_switch_a_on_the_right() {
        let g = Gamepad { east: true, ..Default::default() };
        assert_eq!(from_gamepad(&g, FaceLayout::ByPosition).buttons, A);
        assert_eq!(from_gamepad(&g, FaceLayout::ByLabel).buttons, B);
        let g = Gamepad { south: true, ..Default::default() };
        assert_eq!(from_gamepad(&g, FaceLayout::ByPosition).buttons, B);
        assert_eq!(from_gamepad(&g, FaceLayout::ByLabel).buttons, A);
    }

    #[test]
    fn triggers_menu_and_sticks() {
        let g = Gamepad { left_trigger: 0.2, right_trigger: 0.9, menu: true, options: true, lx: 1.0, ry: -1.0, ..Default::default() };
        let s = from_gamepad(&g, FaceLayout::ByPosition);
        assert_eq!(s.buttons, ZR | PLUS | MINUS | STICK_L_RIGHT | STICK_R_DOWN);
        assert_eq!(s.left.x, crate::STICK_MAX);
        assert_eq!(s.right.y, -crate::STICK_MAX);
    }

    #[test]
    fn small_stick_noise_is_ignored() {
        let g = Gamepad { lx: 0.08, ly: -0.05, ..Default::default() };
        assert_eq!(from_gamepad(&g, FaceLayout::ByPosition), NpadState::default());
    }

    #[test]
    fn keyboard_layout() {
        let s = from_keyboard(&[keys::X, keys::UP, keys::W, keys::D, keys::RETURN]);
        assert_eq!(s.buttons & (A | UP | PLUS), A | UP | PLUS);
        assert!(s.left.x > 20000 && s.left.y > 20000, "{:?}", s.left);
        assert_ne!(s.buttons & STICK_L_UP, 0);
        assert_eq!(from_keyboard(&[keys::A, keys::D]).left.x, 0);
    }
}
