//! Bounded audout mixing into a 48 kHz stereo CoreAudio AudioUnit.
use crate::{ipc::router::Hub, kernel::ObjectId};
use soju_switch_audio::{FRAME, finish, mix_into, pcm16le};
use std::{
    collections::{BTreeMap, VecDeque},
    ffi::c_void,
    marker::PhantomData,
    ptr::NonNull,
    rc::Rc,
};
unsafe extern "C" {
    fn soju_audio_open(error: *mut i32) -> *mut c_void;
    fn soju_audio_close(audio: *mut c_void);
    fn soju_audio_queued(audio: *mut c_void) -> u64;
    fn soju_audio_write(audio: *mut c_void, samples: *const i16, count: usize) -> i32;
    fn soju_audio_nonzero(audio: *mut c_void) -> u64;
}
struct Device {
    raw: NonNull<c_void>,
    _main_thread: PhantomData<Rc<()>>,
}
impl Device {
    fn new() -> Result<Self, String> {
        let mut error = 0;
        let raw = NonNull::new(unsafe { soju_audio_open(&mut error) })
            .ok_or_else(|| format!("CoreAudio output failed: OSStatus {error}"))?;
        Ok(Self {
            raw,
            _main_thread: PhantomData,
        })
    }
    fn queued(&self) -> u64 {
        unsafe { soju_audio_queued(self.raw.as_ptr()) }
    }
    fn write(&self, samples: &[i16]) -> Result<(), String> {
        if unsafe { soju_audio_write(self.raw.as_ptr(), samples.as_ptr(), samples.len()) } != 0 {
            return Err("CoreAudio ring full".into());
        }
        Ok(())
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        unsafe { soju_audio_close(self.raw.as_ptr()) };
    }
}
#[derive(Default)]
struct Voice {
    samples: VecDeque<i16>,
    running: bool,
}
#[derive(Default)]
pub struct Audio {
    device: Option<Device>,
    voices: BTreeMap<ObjectId, Voice>,
}
impl Audio {
    pub fn nonzero_samples(&self) -> u64 {
        self.device
            .as_ref()
            .map_or(0, |d| unsafe { soju_audio_nonzero(d.raw.as_ptr()) })
    }
    pub fn pump(&mut self, hub: &mut Hub) -> Result<(), String> {
        self.voices.retain(|id, voice| match hub.audio_state(*id) {
            Some(running) => {
                voice.running = running;
                true
            }
            None => false,
        });
        let mut buffered: usize = self.voices.values().map(|v| v.samples.len()).sum();
        while let Some(packet) = hub.take_audio() {
            let samples = pcm16le(&packet.pcm);
            buffered = buffered
                .checked_add(samples.len())
                .ok_or("host audio length overflow")?;
            if buffered > 4 * 1024 * 1024
                || (self.voices.len() >= 16 && !self.voices.contains_key(&packet.object))
            {
                return Err("host audio queue limit".into());
            }
            let voice = self.voices.entry(packet.object).or_default();
            voice.running = hub.audio_running(packet.object);
            voice.samples.extend(samples);
        }
        if !self
            .voices
            .values()
            .any(|v| v.running && !v.samples.is_empty())
        {
            return Ok(());
        }
        if self.device.is_none() {
            self.device = Some(Device::new()?);
        }
        let device = self.device.as_ref().ok_or("missing audio device")?;
        // At most 50 ms is submitted ahead; pause/close can have that much output latency.
        while device.queued() + (FRAME * 2) as u64 <= 4800
            && self
                .voices
                .values()
                .any(|v| v.running && !v.samples.is_empty())
        {
            device.write(&mix(&mut self.voices))?;
        }
        Ok(())
    }
}
fn mix(voices: &mut BTreeMap<ObjectId, Voice>) -> Vec<i16> {
    let mut output = vec![0i32; FRAME * 2];
    for voice in voices.values_mut().filter(|v| v.running) {
        let mut left = [0i16; FRAME];
        let mut right = [0i16; FRAME];
        for i in 0..FRAME {
            left[i] = voice.samples.pop_front().unwrap_or(0);
            right[i] = voice.samples.pop_front().unwrap_or(0);
        }
        mix_into(&mut output, 2, 0, &left, 1.0);
        mix_into(&mut output, 2, 1, &right, 1.0);
    }
    finish(&output)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mixes_channels_clips_and_preserves_paused_voice() {
        let mut voices = BTreeMap::from([
            (
                1,
                Voice {
                    samples: VecDeque::from([30000, -20000]),
                    running: true,
                },
            ),
            (
                2,
                Voice {
                    samples: VecDeque::from([30000, -20000]),
                    running: true,
                },
            ),
            (
                3,
                Voice {
                    samples: VecDeque::from([100, 200]),
                    running: false,
                },
            ),
        ]);
        let mixed = mix(&mut voices);
        assert_eq!(&mixed[..2], &[32767, -32768]);
        assert!(mixed[2..].iter().all(|s| *s == 0));
        assert_eq!(voices[&3].samples.len(), 2);
        assert!(voices[&1].samples.is_empty());
    }
}
