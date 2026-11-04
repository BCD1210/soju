//! Nintendo DSP-ADPCM: 8-byte frames of one header byte (high nibble: coefficient pair
//! index 0..7, low nibble: scale exponent) followed by 14 signed 4-bit samples.
//! sample = ((nibble * 2^scale) << 11 + 1024 + c1 * hist1 + c2 * hist2) >> 11, clamped.

use crate::clamp16;

pub const FRAME_BYTES: usize = 8;
pub const SAMPLES_PER_FRAME: usize = 14;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct State {
    pub hist1: i16,
    pub hist2: i16,
}

/// Decode up to `samples` samples starting at frame-aligned `data`.
/// `coefs` are the 8 coefficient pairs from the voice's ADPCM parameter block.
pub fn decode(data: &[u8], coefs: &[[i16; 2]; 8], state: &mut State, samples: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(samples);
    for frame in data.chunks(FRAME_BYTES) {
        if out.len() >= samples || frame.len() < 2 {
            break;
        }
        let header = frame[0];
        let scale = 1i32 << (header & 0xF);
        let [c1, c2] = coefs[((header >> 4) & 7) as usize];
        for &byte in &frame[1..] {
            for nib in [byte >> 4, byte & 0xF] {
                if out.len() >= samples {
                    return out;
                }
                let n = ((nib as i8) << 4 >> 4) as i32; // sign-extend 4 bits
                let v = ((n * scale) << 11) + 1024 + c1 as i32 * state.hist1 as i32 + c2 as i32 * state.hist2 as i32;
                let s = clamp16(v >> 11);
                state.hist2 = state.hist1;
                state.hist1 = s;
                out.push(s);
            }
        }
    }
    out
}

/// Encode with coefficient pair 0, choosing the best scale per frame (test fixtures only).
pub fn encode_simple(pcm: &[i16], coefs: &[[i16; 2]; 8]) -> Vec<u8> {
    let [c1, c2] = coefs[0];
    let mut out = Vec::new();
    let mut st = State::default();
    for chunk in pcm.chunks(SAMPLES_PER_FRAME) {
        let mut best: Option<(i64, u8, Vec<u8>, State)> = None;
        for exp in 0..12u8 {
            let scale = 1i32 << exp;
            let mut s = st;
            let mut nibbles = Vec::with_capacity(SAMPLES_PER_FRAME);
            let mut err = 0i64;
            for &x in chunk {
                let history = c1 as i32 * s.hist1 as i32 + c2 as i32 * s.hist2 as i32;
                let pred = (1024 + history) >> 11;
                let n = (((x as i32 - pred) as f32) / scale as f32).round().clamp(-8.0, 7.0) as i32;
                let v = clamp16((((n * scale) << 11) + 1024 + history) >> 11);
                err += (v as i64 - x as i64).pow(2);
                s.hist2 = s.hist1;
                s.hist1 = v;
                nibbles.push((n & 0xF) as u8);
            }
            if best.as_ref().is_none_or(|b| err < b.0) {
                best = Some((err, exp, nibbles, s));
            }
        }
        let (_, exp, mut nib, s) = best.expect("at least one scale tried");
        nib.resize(SAMPLES_PER_FRAME, 0);
        out.push(exp);
        for k in 0..7 {
            out.push((nib[2 * k] << 4) | nib[2 * k + 1]);
        }
        st = s;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_frame_with_zero_coefficients() {
        // header 0x02: pair 0, scale 4. Nibbles 1, -1 (0xF), 7, -8 (0x8).
        let coefs = [[0i16; 2]; 8];
        let mut st = State::default();
        let out = decode(&[0x02, 0x1F, 0x78, 0, 0, 0, 0, 0], &coefs, &mut st, 4);
        // (n*4 << 11) + 1024 >> 11 == n*4 (the +1024 rounds, never changes whole values)
        assert_eq!(out, [4, -4, 28, -32]);
        assert_eq!(st, State { hist1: -32, hist2: 28 });
    }

    #[test]
    fn prediction_uses_history() {
        // c1 = 2048 (1.0 in 5.11 fixed point): each sample repeats the last plus the delta.
        let mut coefs = [[0i16; 2]; 8];
        coefs[1] = [2048, 0];
        let mut st = State { hist1: 100, hist2: 0 };
        let out = decode(&[0x10, 0x11, 0x00, 0, 0, 0, 0, 0], &coefs, &mut st, 4);
        assert_eq!(out, [101, 102, 102, 102]);
    }

    #[test]
    fn clamps_and_stops_at_requested_length() {
        let mut coefs = [[0i16; 2]; 8];
        coefs[0] = [4096, 0];
        let mut st = State { hist1: 30000, hist2: 0 };
        let out = decode(&[0x0B, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77, 0x77], &coefs, &mut st, 3);
        assert_eq!(out, [32767, 32767, 32767]);
        assert_eq!(decode(&[0x00; 16], &coefs, &mut State::default(), 20).len(), 20);
        assert_eq!(decode(&[0x00; 16], &coefs, &mut State::default(), 100).len(), 28);
    }

    #[test]
    fn encoder_round_trip_is_close() {
        let mut coefs = [[0i16; 2]; 8];
        coefs[0] = [0, 0];
        let pcm: Vec<i16> = (0..280).map(|i| ((i as f32 * 0.07).sin() * 8000.0) as i16).collect();
        let enc = encode_simple(&pcm, &coefs);
        assert_eq!(enc.len(), 20 * FRAME_BYTES);
        let dec = decode(&enc, &coefs, &mut State::default(), pcm.len());
        let max_err = pcm.iter().zip(&dec).map(|(a, b)| (*a as i32 - *b as i32).abs()).max().unwrap();
        assert!(max_err <= 1100, "max error {max_err}");
    }
}
