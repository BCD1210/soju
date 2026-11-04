//! Texture formats Switch games use, with their block size and the Metal pixel format
//! Apple GPUs can sample directly (Apple silicon supports BCn and ASTC natively).
//!
//! This is Soju's own format list. Mapping the GPU's texture-header format codes onto
//! it belongs to the GPU front end and is tested there against homebrew test programs.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Format {
    R8Unorm,
    Rg8Unorm,
    Rgba8Unorm,
    Rgba8Srgb,
    Bgra8Unorm,
    Bgra8Srgb,
    Rgb10A2Unorm,
    Rg11B10Float,
    R16Float,
    Rg16Float,
    Rgba16Float,
    R32Float,
    Rg32Float,
    Rgba32Float,
    R32Uint,
    B5G6R5Unorm,
    A1Bgr5Unorm,
    Depth16Unorm,
    Depth32Float,
    Depth24UnormStencil8,
    Depth32FloatStencil8,
    Bc1RgbaUnorm,
    Bc1RgbaSrgb,
    Bc2RgbaUnorm,
    Bc2RgbaSrgb,
    Bc3RgbaUnorm,
    Bc3RgbaSrgb,
    Bc4RUnorm,
    Bc4RSnorm,
    Bc5RgUnorm,
    Bc5RgSnorm,
    Bc6hRgbUfloat,
    Bc6hRgbFloat,
    Bc7RgbaUnorm,
    Bc7RgbaSrgb,
    Astc4x4Unorm,
    Astc4x4Srgb,
    Astc5x5Unorm,
    Astc5x5Srgb,
    Astc6x6Unorm,
    Astc6x6Srgb,
    Astc8x8Unorm,
    Astc8x8Srgb,
    Astc10x10Unorm,
    Astc10x10Srgb,
    Astc12x12Unorm,
    Astc12x12Srgb,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Info {
    pub bytes: usize,
    pub block_w: usize,
    pub block_h: usize,
    /// MTLPixelFormat raw value (from Apple's public Metal headers).
    pub metal: u32,
    pub name: &'static str,
}

impl Format {
    pub const ALL: [Format; 47] = [
        Format::R8Unorm, Format::Rg8Unorm, Format::Rgba8Unorm, Format::Rgba8Srgb, Format::Bgra8Unorm, Format::Bgra8Srgb,
        Format::Rgb10A2Unorm, Format::Rg11B10Float, Format::R16Float, Format::Rg16Float, Format::Rgba16Float, Format::R32Float,
        Format::Rg32Float, Format::Rgba32Float, Format::R32Uint, Format::B5G6R5Unorm, Format::A1Bgr5Unorm, Format::Depth16Unorm,
        Format::Depth32Float, Format::Depth24UnormStencil8, Format::Depth32FloatStencil8, Format::Bc1RgbaUnorm, Format::Bc1RgbaSrgb,
        Format::Bc2RgbaUnorm, Format::Bc2RgbaSrgb, Format::Bc3RgbaUnorm, Format::Bc3RgbaSrgb, Format::Bc4RUnorm, Format::Bc4RSnorm,
        Format::Bc5RgUnorm, Format::Bc5RgSnorm, Format::Bc6hRgbUfloat, Format::Bc6hRgbFloat, Format::Bc7RgbaUnorm, Format::Bc7RgbaSrgb,
        Format::Astc4x4Unorm, Format::Astc4x4Srgb, Format::Astc5x5Unorm, Format::Astc5x5Srgb, Format::Astc6x6Unorm, Format::Astc6x6Srgb, Format::Astc8x8Unorm,
        Format::Astc8x8Srgb, Format::Astc10x10Unorm, Format::Astc10x10Srgb, Format::Astc12x12Unorm, Format::Astc12x12Srgb,
    ];

    pub fn info(&self) -> Info {
        use Format::*;
        let (bytes, bw, bh, metal, name) = match self {
            R8Unorm => (1, 1, 1, 10, "r8unorm"),
            Rg8Unorm => (2, 1, 1, 30, "rg8unorm"),
            Rgba8Unorm => (4, 1, 1, 70, "rgba8unorm"),
            Rgba8Srgb => (4, 1, 1, 71, "rgba8unorm_srgb"),
            Bgra8Unorm => (4, 1, 1, 80, "bgra8unorm"),
            Bgra8Srgb => (4, 1, 1, 81, "bgra8unorm_srgb"),
            Rgb10A2Unorm => (4, 1, 1, 90, "rgb10a2unorm"),
            Rg11B10Float => (4, 1, 1, 92, "rg11b10float"),
            R16Float => (2, 1, 1, 25, "r16float"),
            Rg16Float => (4, 1, 1, 65, "rg16float"),
            Rgba16Float => (8, 1, 1, 115, "rgba16float"),
            R32Float => (4, 1, 1, 55, "r32float"),
            Rg32Float => (8, 1, 1, 105, "rg32float"),
            Rgba32Float => (16, 1, 1, 125, "rgba32float"),
            R32Uint => (4, 1, 1, 53, "r32uint"),
            B5G6R5Unorm => (2, 1, 1, 40, "b5g6r5unorm"),
            A1Bgr5Unorm => (2, 1, 1, 41, "a1bgr5unorm"),
            Depth16Unorm => (2, 1, 1, 250, "depth16unorm"),
            Depth32Float => (4, 1, 1, 252, "depth32float"),
            // Apple silicon has no packed 24-bit depth: stored as depth32float_stencil8 in Metal.
            Depth24UnormStencil8 => (4, 1, 1, 260, "depth32float_stencil8 (from d24s8)"),
            Depth32FloatStencil8 => (8, 1, 1, 260, "depth32float_stencil8"),
            Bc1RgbaUnorm => (8, 4, 4, 130, "bc1_rgba"),
            Bc1RgbaSrgb => (8, 4, 4, 131, "bc1_rgba_srgb"),
            Bc2RgbaUnorm => (16, 4, 4, 132, "bc2_rgba"),
            Bc2RgbaSrgb => (16, 4, 4, 133, "bc2_rgba_srgb"),
            Bc3RgbaUnorm => (16, 4, 4, 134, "bc3_rgba"),
            Bc3RgbaSrgb => (16, 4, 4, 135, "bc3_rgba_srgb"),
            Bc4RUnorm => (8, 4, 4, 140, "bc4_runorm"),
            Bc4RSnorm => (8, 4, 4, 141, "bc4_rsnorm"),
            Bc5RgUnorm => (16, 4, 4, 142, "bc5_rgunorm"),
            Bc5RgSnorm => (16, 4, 4, 143, "bc5_rgsnorm"),
            Bc6hRgbFloat => (16, 4, 4, 150, "bc6H_rgbFloat"),
            Bc6hRgbUfloat => (16, 4, 4, 151, "bc6H_rgbuFloat"),
            Bc7RgbaUnorm => (16, 4, 4, 152, "bc7_rgbaUnorm"),
            Bc7RgbaSrgb => (16, 4, 4, 153, "bc7_rgbaUnorm_srgb"),
            Astc4x4Srgb => (16, 4, 4, 186, "astc_4x4_srgb"),
            Astc5x5Srgb => (16, 5, 5, 188, "astc_5x5_srgb"),
            Astc5x5Unorm => (16, 5, 5, 206, "astc_5x5_ldr"),
            Astc6x6Srgb => (16, 6, 6, 190, "astc_6x6_srgb"),
            Astc6x6Unorm => (16, 6, 6, 208, "astc_6x6_ldr"),
            Astc8x8Srgb => (16, 8, 8, 194, "astc_8x8_srgb"),
            Astc4x4Unorm => (16, 4, 4, 204, "astc_4x4_ldr"),
            Astc8x8Unorm => (16, 8, 8, 212, "astc_8x8_ldr"),
            Astc10x10Srgb => (16, 10, 10, 198, "astc_10x10_srgb"),
            Astc10x10Unorm => (16, 10, 10, 216, "astc_10x10_ldr"),
            Astc12x12Srgb => (16, 12, 12, 200, "astc_12x12_srgb"),
            Astc12x12Unorm => (16, 12, 12, 218, "astc_12x12_ldr"),
        };
        Info { bytes, block_w: bw, block_h: bh, metal, name }
    }

    pub fn is_compressed(&self) -> bool {
        self.info().block_w > 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_is_consistent() {
        let mut metal_seen = std::collections::HashMap::new();
        for f in Format::ALL {
            let i = f.info();
            assert!(matches!(i.bytes, 1 | 2 | 4 | 8 | 16), "{f:?}");
            assert_eq!(f.is_compressed(), i.block_w > 1);
            if f.is_compressed() {
                assert!(i.bytes == 8 || i.bytes == 16, "{f:?}");
            }
            // Distinct Metal formats, except the deliberate d24s8 -> d32s8 fallback.
            if let Some(prev) = metal_seen.insert(i.metal, f) {
                assert!(matches!((prev, f), (Format::Depth24UnormStencil8, Format::Depth32FloatStencil8)), "{prev:?} {f:?}");
            }
        }
        assert_eq!(Format::Astc12x12Unorm.info().block_w, 12);
    }

    /// Check every Metal value against the installed SDK's MTLPixelFormat.h (macOS only).
    #[test]
    fn metal_values_match_the_sdk_header() {
        let Ok(out) = std::process::Command::new("xcrun").arg("--show-sdk-path").output() else { return };
        let sdk = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let path = format!("{sdk}/System/Library/Frameworks/Metal.framework/Headers/MTLPixelFormat.h");
        let Ok(header) = std::fs::read_to_string(&path) else { return };
        let mut values = std::collections::HashMap::new();
        for line in header.lines() {
            let l = line.trim();
            if let (Some(rest), Some(eq)) = (l.strip_prefix("MTLPixelFormat"), l.rfind('=')) {
                let name: String = rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
                let num: String = l[eq + 1..].chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = num.parse::<u32>() {
                    values.insert(name.replace('_', "").to_ascii_lowercase(), n);
                }
            }
        }
        assert!(values.len() > 100, "could not read {path}");
        for f in Format::ALL {
            let i = f.info();
            let key = i.name.split(' ').next().unwrap().replace('_', "").to_ascii_lowercase();
            assert_eq!(values.get(&key), Some(&i.metal), "{f:?} ({})", i.name);
        }
    }
}
