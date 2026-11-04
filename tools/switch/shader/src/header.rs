use crate::{Error, Result};

pub const HEADER_SIZE: usize = 0x50;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    VertexA,
    Vertex,
    TessControl,
    TessEvaluation,
    Geometry,
    Fragment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interpolation {
    Flat,
    Perspective,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Attribute {
    /// Byte address in the Maxwell attribute space, one entry per 32-bit component.
    pub address: u16,
    pub interpolation: Option<Interpolation>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Header {
    pub stage: Stage,
    pub version: u8,
    pub sass_version: u8,
    pub local_positive: u32,
    pub local_negative: u32,
    pub warp_stack_size: u32,
    /// Raw input and output map words; fragment maps have a distinct layout.
    pub imap: Vec<u32>,
    pub omap: Vec<u32>,
    pub inputs: Vec<Attribute>,
    pub outputs: Vec<Attribute>,
    pub color_masks: [u8; 8],
    pub depth_output: bool,
    pub raw: [u32; 20],
}

impl Header {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() < HEADER_SIZE {
            return Err(Error(format!("SPH needs 80 bytes, got {}", bytes.len())));
        }
        let mut raw = [0; 20];
        for (word, bytes) in raw.iter_mut().zip(bytes.chunks_exact(4)) {
            *word = u32::from_le_bytes(bytes.try_into().unwrap());
        }
        let stage = match (raw[0] >> 10) & 15 {
            0 => Stage::VertexA,
            1 => Stage::Vertex,
            2 => Stage::TessControl,
            3 => Stage::TessEvaluation,
            4 => Stage::Geometry,
            5 => Stage::Fragment,
            n => return Err(Error(format!("unknown SPH stage {n}"))),
        };
        let fragment = stage == Stage::Fragment;
        let expected = if fragment { 2 } else { 1 };
        if raw[0] & 31 != expected {
            return Err(Error("SPH type disagrees with stage".into()));
        }
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        let mut color_masks = [0; 8];
        let (imap, omap) = if fragment {
            for component in 0..4 {
                if raw[5] & (1 << (28 + component)) != 0 {
                    inputs.push(Attribute {
                        address: 0x70 + component * 4,
                        interpolation: None,
                    });
                }
            }
            for (i, word) in raw.iter().enumerate().take(17).skip(6) {
                // Word 14 has special color/point fields; preserve rather than guess.
                if i == 14 {
                    continue;
                }
                let base = if i >= 15 {
                    0x300 + (i - 15) * 64
                } else {
                    0x80 + (i - 6) * 64
                };
                for c in 0..16 {
                    let mode = match (word >> (c * 2)) & 3 {
                        0 => continue,
                        1 => Interpolation::Flat,
                        2 => Interpolation::Perspective,
                        _ => Interpolation::Linear,
                    };
                    inputs.push(Attribute {
                        address: (base + c * 4) as u16,
                        interpolation: Some(mode),
                    });
                }
            }
            for (i, mask) in color_masks.iter_mut().enumerate() {
                *mask = ((raw[18] >> (4 * i)) & 15) as u8;
            }
            (raw[5..17].to_vec(), raw[18..20].to_vec())
        } else {
            for i in 0..192 {
                if raw[5 + i / 32] & (1 << (i % 32)) != 0 {
                    inputs.push(Attribute {
                        address: (i * 4) as u16,
                        interpolation: None,
                    });
                }
                if i < 168 && raw[13 + i / 32] & (1 << (i % 32)) != 0 {
                    outputs.push(Attribute {
                        address: (0x40 + i * 4) as u16,
                        interpolation: None,
                    });
                }
            }
            (raw[5..11].to_vec(), raw[13..19].to_vec())
        };
        Ok(Self {
            stage,
            version: ((raw[0] >> 5) & 31) as u8,
            sass_version: ((raw[0] >> 17) & 31) as u8,
            local_positive: raw[1] & 0xffffff,
            local_negative: raw[2] & 0xffffff,
            warp_stack_size: raw[3] & 0xffffff,
            imap,
            omap,
            inputs,
            outputs,
            color_masks,
            depth_output: fragment && raw[19] & 2 != 0,
            raw,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bytes(words: [u32; 20]) -> Vec<u8> {
        words.into_iter().flat_map(u32::to_le_bytes).collect()
    }
    #[test]
    fn vertex_maps_and_allocations() {
        let mut w = [0; 20];
        w[0] = 1 | (1 << 10);
        w[1] = 0xab000123;
        w[2] = 456;
        w[3] = 512;
        w[6] = 3;
        w[13] = 15 << 12;
        let h = Header::parse(&bytes(w)).unwrap();
        assert_eq!(h.stage, Stage::Vertex);
        assert_eq!(h.local_positive, 0x123);
        assert_eq!(h.local_negative, 456);
        assert_eq!(h.warp_stack_size, 512);
        assert_eq!(
            h.inputs.iter().map(|a| a.address).collect::<Vec<_>>(),
            [0x80, 0x84]
        );
        assert_eq!(
            h.outputs.iter().map(|a| a.address).collect::<Vec<_>>(),
            [0x70, 0x74, 0x78, 0x7c]
        );
    }
    #[test]
    fn fragment_layout_and_errors() {
        let mut w = [0; 20];
        w[0] = 2 | (5 << 10);
        w[6] = 1 | (2 << 2) | (3 << 4);
        w[18] = 0xa5;
        w[19] = 2;
        let h = Header::parse(&bytes(w)).unwrap();
        assert_eq!(h.inputs.len(), 3);
        assert_eq!(h.inputs[1].interpolation, Some(Interpolation::Perspective));
        assert_eq!(&h.color_masks[..2], &[5, 10]);
        assert!(h.depth_output);
        assert!(Header::parse(&[0; 79]).is_err());
        w[0] = 2 | (15 << 10);
        assert!(Header::parse(&bytes(w)).is_err());
        w[0] = 1 | (5 << 10);
        assert!(Header::parse(&bytes(w)).is_err());
    }
}
