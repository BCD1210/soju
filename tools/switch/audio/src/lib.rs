//! Voice processing for Soju's Switch audio renderer, from public descriptions of the
//! DSP-ADPCM format and standard DSP (no emulator code).
//!
//! The Switch mixes at 48 kHz in 5 ms frames of 240 samples. A voice decodes its source
//! (PCM8, PCM16, PCM float or DSP-ADPCM), resamples from its own rate with a pitch
//! factor, runs up to two biquad filters and is mixed into output channels with volume.

pub mod adpcm;
pub mod biquad;
pub mod resample;

pub const SAMPLE_RATE: u32 = 48_000;
pub const FRAME: usize = 240;

#[inline]
pub fn clamp16(v: i32) -> i16 {
    v.clamp(i16::MIN as i32, i16::MAX as i32) as i16
}

pub fn pcm8_to_16(src: &[u8]) -> Vec<i16> {
    src.iter().map(|&b| ((b as i8) as i16) << 8).collect()
}

pub fn pcm16le(src: &[u8]) -> Vec<i16> {
    src.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect()
}

pub fn pcm_f32le(src: &[u8]) -> Vec<i16> {
    src.chunks_exact(4)
        .map(|c| {
            let f = f32::from_le_bytes(c.try_into().unwrap());
            clamp16((f.clamp(-1.0, 1.0) * 32767.0).round() as i32)
        })
        .collect()
}

/// Mix `src` (mono) into interleaved `dst` channel `ch` of `channels` with a volume.
pub fn mix_into(dst: &mut [i32], channels: usize, ch: usize, src: &[i16], volume: f32) {
    for (i, s) in src.iter().enumerate() {
        if let Some(d) = dst.get_mut(i * channels + ch) {
            *d += (*s as f32 * volume) as i32;
        }
    }
}

/// Final mix buffer (i32 accumulators) to clamped 16-bit samples.
pub fn finish(mix: &[i32]) -> Vec<i16> {
    mix.iter().map(|&v| clamp16(v)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pcm_conversions() {
        assert_eq!(pcm8_to_16(&[0x7f, 0x80, 0]), [0x7f00, -0x8000, 0]);
        assert_eq!(pcm16le(&[0x34, 0x12, 0xff, 0xff]), [0x1234, -1]);
        let f: Vec<u8> = [1.0f32, -1.0, 0.5, 2.0].iter().flat_map(|f| f.to_le_bytes()).collect();
        assert_eq!(pcm_f32le(&f), [32767, -32767, 16384, 32767]);
    }

    #[test]
    fn mixing_saturates() {
        let mut mix = vec![0i32; 4];
        mix_into(&mut mix, 2, 0, &[30000, 30000], 1.0);
        mix_into(&mut mix, 2, 0, &[30000, -100], 1.0);
        mix_into(&mut mix, 2, 1, &[100, 200], 0.5);
        assert_eq!(finish(&mix), [32767, 50, 29900, 100]);
    }
}
