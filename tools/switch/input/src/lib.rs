//! Switch controller (Npad) state built from Mac input, using the button bit layout
//! documented publicly for homebrew (libnx `HidNpadButton`). No emulator code.
//!
//! Games read a 64-bit button mask plus two sticks in -32767..=32767. Stick directions
//! are also reported as buttons (bits 16-23) past a threshold, as the console does.

pub mod mapping;

pub mod button {
    pub const A: u64 = 1 << 0;
    pub const B: u64 = 1 << 1;
    pub const X: u64 = 1 << 2;
    pub const Y: u64 = 1 << 3;
    pub const STICK_L: u64 = 1 << 4;
    pub const STICK_R: u64 = 1 << 5;
    pub const L: u64 = 1 << 6;
    pub const R: u64 = 1 << 7;
    pub const ZL: u64 = 1 << 8;
    pub const ZR: u64 = 1 << 9;
    pub const PLUS: u64 = 1 << 10;
    pub const MINUS: u64 = 1 << 11;
    pub const LEFT: u64 = 1 << 12;
    pub const UP: u64 = 1 << 13;
    pub const RIGHT: u64 = 1 << 14;
    pub const DOWN: u64 = 1 << 15;
    pub const STICK_L_LEFT: u64 = 1 << 16;
    pub const STICK_L_UP: u64 = 1 << 17;
    pub const STICK_L_RIGHT: u64 = 1 << 18;
    pub const STICK_L_DOWN: u64 = 1 << 19;
    pub const STICK_R_LEFT: u64 = 1 << 20;
    pub const STICK_R_UP: u64 = 1 << 21;
    pub const STICK_R_RIGHT: u64 = 1 << 22;
    pub const STICK_R_DOWN: u64 = 1 << 23;
}

pub const STICK_MAX: i32 = 0x7FFF;
/// Fraction of full deflection at which a stick also counts as a direction button.
pub const STICK_BUTTON_THRESHOLD: f32 = 0.5;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stick {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NpadState {
    pub buttons: u64,
    pub left: Stick,
    pub right: Stick,
}

/// Radial dead zone with rescaling: inside `dead` (0..1) reads as centered; outside,
/// the remaining range is stretched back to full scale. Input is -1.0..=1.0 per axis,
/// y up positive (the Switch convention).
pub fn stick_from_axes(x: f32, y: f32, dead: f32) -> Stick {
    let (x, y) = (x.clamp(-1.0, 1.0), y.clamp(-1.0, 1.0));
    let mag = (x * x + y * y).sqrt();
    if mag <= dead || mag == 0.0 {
        return Stick::default();
    }
    let scaled = ((mag - dead) / (1.0 - dead)).min(1.0);
    let (nx, ny) = (x / mag * scaled, y / mag * scaled);
    Stick { x: (nx * STICK_MAX as f32).round() as i32, y: (ny * STICK_MAX as f32).round() as i32 }
}

fn stick_buttons(s: Stick, left: bool) -> u64 {
    use button::*;
    let t = (STICK_MAX as f32 * STICK_BUTTON_THRESHOLD) as i32;
    let (l, u, r, d) = if left { (STICK_L_LEFT, STICK_L_UP, STICK_L_RIGHT, STICK_L_DOWN) } else { (STICK_R_LEFT, STICK_R_UP, STICK_R_RIGHT, STICK_R_DOWN) };
    let mut b = 0;
    if s.x <= -t { b |= l; }
    if s.x >= t { b |= r; }
    if s.y >= t { b |= u; }
    if s.y <= -t { b |= d; }
    b
}

impl NpadState {
    /// Finalise: add stick-direction bits, as games expect them alongside the sticks.
    pub fn with_stick_buttons(mut self) -> Self {
        self.buttons |= stick_buttons(self.left, true) | stick_buttons(self.right, false);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bit_layout_is_unique_and_dense() {
        use button::*;
        let all = [A, B, X, Y, STICK_L, STICK_R, L, R, ZL, ZR, PLUS, MINUS, LEFT, UP, RIGHT, DOWN,
                   STICK_L_LEFT, STICK_L_UP, STICK_L_RIGHT, STICK_L_DOWN, STICK_R_LEFT, STICK_R_UP, STICK_R_RIGHT, STICK_R_DOWN];
        assert_eq!(all.iter().fold(0, |a, b| a | b), (1 << 24) - 1);
    }

    #[test]
    fn dead_zone_and_scaling() {
        assert_eq!(stick_from_axes(0.1, 0.05, 0.15), Stick::default());
        let full = stick_from_axes(1.0, 0.0, 0.15);
        assert_eq!(full, Stick { x: STICK_MAX, y: 0 });
        let half = stick_from_axes(0.575, 0.0, 0.15);
        assert!((half.x - STICK_MAX / 2).abs() < 40, "{half:?}");
        let diag = stick_from_axes(1.0, 1.0, 0.0);
        assert!(diag.x <= STICK_MAX && (diag.x - diag.y).abs() <= 1);
    }

    #[test]
    fn stick_direction_buttons() {
        let s = NpadState { buttons: 0, left: Stick { x: -30000, y: 20000 }, right: Stick { x: 0, y: -20000 } }.with_stick_buttons();
        assert_eq!(s.buttons, button::STICK_L_LEFT | button::STICK_L_UP | button::STICK_R_DOWN);
    }
}
