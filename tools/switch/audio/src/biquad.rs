//! Biquad filter as games configure it: coefficients b0, b1, b2, a1, a2 in signed
//! Q2.14 fixed point; y = b0*x + b1*x1 + b2*x2 - a1*y1 - a2*y2 (direct form I).

use crate::clamp16;

#[derive(Debug, Clone, Copy, Default)]
pub struct Biquad {
    pub b: [i16; 3],
    pub a: [i16; 2],
    x1: i32,
    x2: i32,
    y1: i32,
    y2: i32,
}

impl Biquad {
    pub fn new(b: [i16; 3], a: [i16; 2]) -> Self {
        Biquad { b, a, ..Default::default() }
    }

    pub fn process(&mut self, samples: &mut [i16]) {
        for s in samples.iter_mut() {
            let x = *s as i32;
            let acc = self.b[0] as i64 * x as i64 + self.b[1] as i64 * self.x1 as i64 + self.b[2] as i64 * self.x2 as i64
                - self.a[0] as i64 * self.y1 as i64
                - self.a[1] as i64 * self.y2 as i64;
            let y = clamp16(((acc + (1 << 13)) >> 14) as i32) as i32;
            self.x2 = self.x1;
            self.x1 = x;
            self.y2 = self.y1;
            self.y1 = y;
            *s = y as i16;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pass_through_and_gain() {
        let mut f = Biquad::new([1 << 14, 0, 0], [0, 0]);
        let mut s = [100i16, -200, 300];
        f.process(&mut s);
        assert_eq!(s, [100, -200, 300]);
        let mut g = Biquad::new([1 << 13, 0, 0], [0, 0]);
        let mut s = [100i16, -200];
        g.process(&mut s);
        assert_eq!(s, [50, -100]);
    }

    #[test]
    fn one_pole_low_pass_settles_to_dc() {
        // y = 0.25 x + 0.75 y1  ->  b0 = 0.25, a1 = -0.75
        let mut f = Biquad::new([4096, 0, 0], [-12288, 0]);
        let mut s = vec![1000i16; 200];
        f.process(&mut s);
        assert!(s[0] == 250 && (s[199] - 1000).abs() <= 2, "{} {}", s[0], s[199]);
    }

    #[test]
    fn saturates() {
        let mut f = Biquad::new([i16::MAX, i16::MAX, i16::MAX], [0, 0]);
        let mut s = [i16::MAX; 3];
        f.process(&mut s);
        assert_eq!(s[2], i16::MAX);
    }
}
