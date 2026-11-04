//! Soju's experimental, clean-room Switch homebrew core.
pub mod ipc;
pub mod kernel;
pub mod memory;
pub mod nro;

#[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
pub mod hv;

#[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
pub mod runtime;

#[cfg(all(feature = "desktop", target_os = "macos", target_arch = "aarch64"))]
pub mod desktop;

#[cfg(all(feature = "desktop", target_os = "macos", target_arch = "aarch64"))]
pub mod host_audio;

pub mod program;
