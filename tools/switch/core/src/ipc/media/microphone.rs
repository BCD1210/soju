//! Native capture is optional; unavailable or unapproved devices supply silence.
#[derive(Default)]
pub(super) struct Microphone {
    #[cfg(feature = "desktop")]
    raw: Option<std::ptr::NonNull<std::ffi::c_void>>,
}
#[cfg(feature = "desktop")]
unsafe extern "C" {
    fn soju_mic_open() -> *mut std::ffi::c_void;
    fn soju_mic_read(handle: *mut std::ffi::c_void, out: *mut i16, capacity: usize) -> usize;
    fn soju_mic_close(handle: *mut std::ffi::c_void);
}
impl Microphone {
    pub(super) fn start() -> Self {
        #[cfg(feature = "desktop")]
        {
            Self {
                raw: std::ptr::NonNull::new(unsafe { soju_mic_open() }),
            }
        }
        #[cfg(not(feature = "desktop"))]
        {
            Self::default()
        }
    }
    pub(super) fn read(&mut self, samples: usize) -> Vec<i16> {
        let mut output = vec![0; samples];
        #[cfg(feature = "desktop")]
        if let Some(raw) = self.raw {
            unsafe {
                soju_mic_read(raw.as_ptr(), output.as_mut_ptr(), samples);
            }
        }
        #[cfg(not(feature = "desktop"))]
        let _ = &mut output;
        output
    }
}
impl Drop for Microphone {
    fn drop(&mut self) {
        #[cfg(feature = "desktop")]
        if let Some(raw) = self.raw {
            unsafe {
                soju_mic_close(raw.as_ptr());
            }
        }
    }
}
