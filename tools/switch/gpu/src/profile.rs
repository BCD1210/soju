//! Opt-in aggregate host timings and Instruments intervals; no guest data is recorded.
use std::sync::{
    atomic::{AtomicU64, Ordering},
    OnceLock,
};
use std::time::Instant;

#[derive(Clone, Copy)]
#[repr(usize)]
pub enum Stage {
    Guest,
    Rendezvous,
    Svc,
    Ipc,
    Decode,
    Snapshot,
    Macro,
    ShaderDiscovery,
    Prepare,
    Submit,
    Readback,
    Commit,
    Present,
    ShaderTranslation,
    DrawReference,
    NvIoctl,
    NativeStartup,
}
const NAMES: [&std::ffi::CStr; 17] = [
    c"guest",
    c"rendezvous",
    c"svc",
    c"ipc",
    c"decode",
    c"snapshot",
    c"macro",
    c"shader_discovery",
    c"prepare",
    c"submit",
    c"readback",
    c"commit",
    c"present",
    c"shader_translation",
    c"draw_reference",
    c"nv_ioctl",
    c"native_startup",
];
static NS: [AtomicU64; 17] = [const { AtomicU64::new(0) }; 17];
static CALLS: [AtomicU64; 17] = [const { AtomicU64::new(0) }; 17];
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("SOJU_PROFILE").is_some())
}
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn soju_profile_begin(stage: *const std::ffi::c_char) -> u64;
    fn soju_profile_end(id: u64);
}
pub struct Span(Option<(Stage, Instant, u64)>);
pub fn span(stage: Stage) -> Span {
    if !enabled() {
        return Span(None);
    }
    #[cfg(target_os = "macos")]
    // SAFETY: names are static NUL-terminated strings.
    let id = unsafe { soju_profile_begin(NAMES[stage as usize].as_ptr()) };
    #[cfg(not(target_os = "macos"))]
    let id = 0;
    Span(Some((stage, Instant::now(), id)))
}
impl Drop for Span {
    fn drop(&mut self) {
        if let Some((stage, start, id)) = self.0 {
            NS[stage as usize].fetch_add(start.elapsed().as_nanos() as u64, Ordering::Relaxed);
            CALLS[stage as usize].fetch_add(1, Ordering::Relaxed);
            #[cfg(target_os = "macos")]
            // SAFETY: id was returned by the matching begin call.
            unsafe {
                soju_profile_end(id);
            }
            #[cfg(not(target_os = "macos"))]
            let _ = id;
        }
    }
}
/// Inclusive cumulative durations; nested stages and concurrent threads overlap.
pub fn report() {
    if !enabled() {
        return;
    }
    for (i, name) in NAMES.iter().enumerate() {
        eprintln!(
            "profile|{}|calls={}|ms={:.6}",
            name.to_str().unwrap(),
            CALLS[i].load(Ordering::Relaxed),
            NS[i].load(Ordering::Relaxed) as f64 / 1e6
        );
    }
}
