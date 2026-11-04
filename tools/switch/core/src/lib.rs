//! Soju's experimental, clean-room Switch homebrew core.
pub mod nro;

#[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
pub mod hv;
