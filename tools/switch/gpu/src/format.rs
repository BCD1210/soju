//! NVIDIA clb197.h/clb197tex.h encodings; Metal values come from the texture crate.
use crate::{Error, Result};
use soju_switch_texture::format::Format;
pub fn color(code: u32) -> Result<Format> {
    use Format::*;
    Ok(match code {
        0xc0 => Rgba32Float,
        0xca => Rgba16Float,
        0xcb => Rg32Float,
        0xcf => Bgra8Unorm,
        0xd0 => Bgra8Srgb,
        0xd1 => Rgb10A2Unorm,
        0xd5 => Rgba8Unorm,
        0xd6 => Rgba8Srgb,
        0xde => Rg16Float,
        0xe0 => Rg11B10Float,
        0xe4 => R32Uint,
        0xe5 => R32Float,
        0xe8 => B5G6R5Unorm,
        0xe9 => A1Bgr5Unorm,
        0xea => Rg8Unorm,
        0xf2 => R16Float,
        0xf3 => R8Unorm,
        _ => {
            return Err(Error(format!(
                "render target requires supported format: {code:#x}"
            )))
        }
    })
}
pub fn texture(code: u32, ty: u32, srgb: bool) -> Result<Format> {
    use Format::*;
    let f = match (code, ty) {
        (1, 7) => Rgba32Float,
        (3, 7) => Rgba16Float,
        (4, 7) => Rg32Float,
        (8, 2) => Rgba8Unorm,
        (9, 2) => Rgb10A2Unorm,
        (0xc, 7) => Rg16Float,
        (0xf, 7) => R32Float,
        (0xf, 4) => R32Uint,
        (0x14, 2) => A1Bgr5Unorm,
        (0x15, 2) => B5G6R5Unorm,
        (0x18, 2) => Rg8Unorm,
        (0x1b, 7) => R16Float,
        (0x1d, 2) => R8Unorm,
        (0x21, 7) => Rg11B10Float,
        (0x24, 2) => Bc1RgbaUnorm,
        (0x25, 2) => Bc2RgbaUnorm,
        (0x26, 2) => Bc3RgbaUnorm,
        (0x27, 2) => Bc4RUnorm,
        (0x27, 1) => Bc4RSnorm,
        (0x28, 2) => Bc5RgUnorm,
        (0x28, 1) => Bc5RgSnorm,
        (0x10, 7) => Bc6hRgbFloat,
        (0x11, 7) => Bc6hRgbUfloat,
        (0x17, 2) => Bc7RgbaUnorm,
        (0x40, 2) => Astc4x4Unorm,
        (0x41, 2) => Astc5x5Unorm,
        (0x42, 2) => Astc6x6Unorm,
        (0x44, 2) => Astc8x8Unorm,
        (0x45, 2) => Astc10x10Unorm,
        (0x46, 2) => Astc12x12Unorm,
        (0x3a, 2) => Depth16Unorm,
        (0x2f, 7) => Depth32Float,
        (0x29, 2) => Depth24UnormStencil8,
        (0x30, 7) => Depth32FloatStencil8,
        _ => {
            return Err(Error(format!(
                "TIC format {code:#x}, component type {ty} unsupported"
            )))
        }
    };
    Ok(if srgb {
        match f {
            Rgba8Unorm => Rgba8Srgb,
            Bc1RgbaUnorm => Bc1RgbaSrgb,
            Bc2RgbaUnorm => Bc2RgbaSrgb,
            Bc3RgbaUnorm => Bc3RgbaSrgb,
            Bc7RgbaUnorm => Bc7RgbaSrgb,
            Astc4x4Unorm => Astc4x4Srgb,
            Astc8x8Unorm => Astc8x8Srgb,
            Astc5x5Unorm => Astc5x5Srgb,
            Astc6x6Unorm => Astc6x6Srgb,
            Astc10x10Unorm => Astc10x10Srgb,
            Astc12x12Unorm => Astc12x12Srgb,
            _ => return Err(Error("unsupported sRGB conversion".into())),
        }
    } else {
        f
    })
}
fn half(bits: u16) -> f32 {
    let sign = if bits & 0x8000 != 0 { -1. } else { 1. };
    let exp = (bits >> 10) & 31;
    let frac = (bits & 1023) as f32;
    sign * match exp {
        0 => frac * 2f32.powi(-24),
        31 => {
            if frac == 0. {
                f32::INFINITY
            } else {
                f32::NAN
            }
        }
        _ => (1. + frac / 1024.) * 2f32.powi(exp as i32 - 15),
    }
}
/// Diagnostic RGBA8 preview. Guest writeback always uses the original raw bytes.
pub fn rgba(format: Format, bytes: &[u8]) -> Vec<u8> {
    use Format::*;
    bytes
        .chunks_exact(format.info().bytes)
        .flat_map(|p| {
            let mut c = [0., 0., 0., 1.];
            let u16at = |i| u16::from_le_bytes([p[i], p[i + 1]]);
            let u32at = |i| u32::from_le_bytes(p[i..i + 4].try_into().unwrap());
            match format {
                R8Unorm | Rg8Unorm | Rgba8Unorm | Rgba8Srgb | Bgra8Unorm | Bgra8Srgb => {
                    for (v, b) in c.iter_mut().zip(p) {
                        *v = *b as f32 / 255.;
                    }
                    if matches!(format, Bgra8Unorm | Bgra8Srgb) {
                        c.swap(0, 2);
                    }
                }
                R16Float | Rg16Float | Rgba16Float => {
                    for (i, v) in c.iter_mut().take(p.len() / 2).enumerate() {
                        *v = half(u16at(i * 2));
                    }
                }
                R32Float | Rg32Float | Rgba32Float => {
                    for (i, v) in c.iter_mut().take(p.len() / 4).enumerate() {
                        *v = f32::from_bits(u32at(i * 4));
                    }
                }
                R32Uint => c[0] = u32at(0) as f32,
                Rgb10A2Unorm => {
                    let v = u32at(0);
                    c = [
                        (v & 1023) as f32 / 1023.,
                        ((v >> 10) & 1023) as f32 / 1023.,
                        ((v >> 20) & 1023) as f32 / 1023.,
                        (v >> 30) as f32 / 3.,
                    ];
                }
                B5G6R5Unorm => {
                    let v = u16at(0);
                    c = [
                        (v >> 11) as f32 / 31.,
                        ((v >> 5) & 63) as f32 / 63.,
                        (v & 31) as f32 / 31.,
                        1.,
                    ];
                }
                A1Bgr5Unorm => {
                    let v = u16at(0);
                    c = [
                        ((v >> 10) & 31) as f32 / 31.,
                        ((v >> 5) & 31) as f32 / 31.,
                        (v & 31) as f32 / 31.,
                        (v >> 15) as f32,
                    ];
                }
                Rg11B10Float => {
                    let v = u32at(0);
                    c = [
                        half(((v & 2047) << 4) as u16),
                        half((((v >> 11) & 2047) << 4) as u16),
                        half(((v >> 22) << 5) as u16),
                        1.,
                    ];
                }
                _ => unreachable!("not a color format"),
            }
            c.map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
        })
        .collect()
}
