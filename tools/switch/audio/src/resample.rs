//! Resampling a voice from its source rate (times a pitch factor) to 48 kHz using a
//! 4-tap Catmull-Rom interpolator with a 32.32 fixed-point position that carries over
//! between frames, so long sounds do not drift or click at frame edges.

#[derive(Debug, Clone, Copy, Default)]
pub struct Resampler {
    /// Fractional position (32 bits) into the source.
    frac: u64,
    /// Last 3 source samples from the previous call (for interpolation across calls).
    hist: [i16; 3],
}

impl Resampler {
    /// Step per output sample in 32.32 fixed point.
    pub fn step(source_rate: u32, pitch: f32) -> u64 {
        let ratio = source_rate as f64 * pitch.max(0.0) as f64 / crate::SAMPLE_RATE as f64;
        (ratio * (1u64 << 32) as f64).round() as u64
    }

    /// Produce `out_len` samples from `src`; returns how many source samples were consumed.
    pub fn process(&mut self, src: &[i16], step: u64, out_len: usize, out: &mut Vec<i16>) -> usize {
        let get = |i: isize, hist: &[i16; 3]| -> f32 {
            if i < 0 {
                hist[(3 + i).max(0) as usize] as f32
            } else {
                src.get(i as usize).copied().unwrap_or(*src.last().unwrap_or(&hist[2])) as f32
            }
        };
        let mut pos = self.frac;
        for _ in 0..out_len {
            let i = (pos >> 32) as isize;
            let t = (pos & 0xFFFF_FFFF) as f32 / 4294967296.0;
            let (p0, p1, p2, p3) = (get(i - 1, &self.hist), get(i, &self.hist), get(i + 1, &self.hist), get(i + 2, &self.hist));
            let v = p1 + 0.5 * t * (p2 - p0 + t * (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3 + t * (3.0 * (p1 - p2) + p3 - p0)));
            out.push(crate::clamp16(v.round() as i32));
            pos += step;
        }
        let consumed = ((pos >> 32) as usize).min(src.len());
        for k in 0..3 {
            let idx = consumed as isize - 3 + k as isize;
            self.hist[k] = if idx >= 0 { src[idx as usize] } else { self.hist[(3 + idx) as usize] };
        }
        self.frac = pos - ((consumed as u64) << 32);
        consumed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_rate_is_identity() {
        let mut r = Resampler::default();
        let src: Vec<i16> = (0..240).map(|i| (i * 37 % 1000) as i16).collect();
        let mut out = Vec::new();
        let used = r.process(&src, Resampler::step(48_000, 1.0), 240, &mut out);
        assert_eq!(used, 240);
        assert_eq!(out, src);
    }

    #[test]
    fn half_rate_doubles_and_keeps_shape() {
        let mut r = Resampler::default();
        let src: Vec<i16> = (0..120).map(|i| (i * 100) as i16).collect();
        let mut out = Vec::new();
        let used = r.process(&src, Resampler::step(24_000, 1.0), 240, &mut out);
        assert_eq!(used, 120);
        // A straight ramp stays a ramp at half steps.
        for (i, v) in out.iter().enumerate().take(230).skip(2) {
            assert!((*v as i32 - (i as i32 * 50)).abs() <= 1, "{i}: {v}");
        }
    }

    #[test]
    fn continues_across_frames_without_jumps() {
        let src: Vec<i16> = (0..2000).map(|i| ((i as f32 * 0.05).sin() * 10000.0) as i16).collect();
        let step = Resampler::step(32_000, 1.0);
        let mut one = Resampler::default();
        let mut all = Vec::new();
        one.process(&src, step, 960, &mut all);
        let mut split = Resampler::default();
        let mut parts = Vec::new();
        let mut at = 0;
        for _ in 0..4 {
            at += split.process(&src[at..], step, 240, &mut parts);
        }
        let worst = all.iter().zip(&parts).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(worst <= 2, "frame-split output differs by {worst}");
    }
}
