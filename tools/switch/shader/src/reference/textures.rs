use super::*;
use crate::texture::{Binding, Handle};
#[derive(Debug, Clone)]
pub struct Texture {
    pub width: u32,
    pub height: u32,
    pub texels: Vec<[f32; 4]>,
}
#[derive(Debug, Clone, Copy)]
pub enum Sampler {
    ClampNearest,
    RepeatNearest,
}
impl Machine {
    pub(super) fn texture(&mut self, x: &Instruction, m: &Memory) -> Result<()> {
        use Opcode::*;
        let scalar = matches!(x.opcode, TEXS | TLDS);
        let bindless = x.options.iter().any(|o| o.name == "b");
        let hi = match x.opcode {
            TEXS | TLDS => 4,
            TXQ => 2,
            _ => {
                if bindless {
                    2
                } else {
                    3
                }
            }
        };
        let raw = self.value(x, hi, m)?;
        let handle = if bindless {
            Handle::Bindless(raw)
        } else {
            Handle::Bound(raw)
        };
        let binding = m
            .bindings
            .iter()
            .find(|b| b.handle == handle)
            .copied()
            .unwrap_or(Binding {
                handle,
                texture_index: raw,
                sampler_index: raw,
                texture_type: crate::texture::TextureType::D2,
            });
        let texture = m
            .textures
            .get(binding.texture_index as usize)
            .ok_or_else(|| x.error("missing reference texture"))?;
        let sampler = m
            .samplers
            .get(binding.sampler_index as usize)
            .ok_or_else(|| x.error("missing reference sampler"))?;
        let Operand::Register(coord) = x.operands[if scalar { 2 } else { 1 }] else {
            return Err(x.error("coordinate register"));
        };
        let y = if scalar {
            self.value(x, 3, m)?
        } else {
            self.register(coord + 1)
        };
        let index = |value: f32, size: u32| -> u32 {
            match sampler {
                Sampler::ClampNearest => {
                    (value * size as f32).floor().clamp(0.0, (size - 1) as f32) as u32
                }
                Sampler::RepeatNearest => ((value - value.floor()) * size as f32).floor() as u32,
            }
        };
        let texel = |x: u32, y: u32| -> [f32; 4] {
            if x >= texture.width || y >= texture.height {
                [0.0; 4]
            } else {
                texture.texels[(y * texture.width + x) as usize]
            }
        };
        let value = match x.opcode {
            TEX | TEXS => texel(
                index(f32::from_bits(self.register(coord)), texture.width),
                index(f32::from_bits(y), texture.height),
            )
            .map(f32::to_bits),
            TLD | TLDS => texel(self.register(coord), y).map(f32::to_bits),
            TXQ => [texture.width, texture.height, 1, 1],
            TLD4 => {
                let u = f32::from_bits(self.register(coord));
                let v = f32::from_bits(y);
                let cx = (u * texture.width as f32 - 0.5).floor();
                let cy = (v * texture.height as f32 - 0.5).floor();
                let c = bits(x.raw, if bindless { 38 } else { 56 }, 2) as usize;
                [(0.0, 1.0), (1.0, 1.0), (1.0, 0.0), (0.0, 0.0)].map(|(dx, dy)| {
                    texel(
                        index((cx + dx + 0.5) / texture.width as f32, texture.width),
                        index((cy + dy + 0.5) / texture.height as f32, texture.height),
                    )[c]
                        .to_bits()
                })
            }
            _ => return Err(x.error("unsupported reference texture")),
        };
        let mask = if scalar {
            [1, 2, 4, 8, 3, 9, 10, 12][bits(x.raw, 50, 3) as usize]
        } else {
            bits(x.raw, 31, 4)
        };
        let Operand::Register(first) = x.operands[usize::from(scalar)] else {
            return Err(x.error("texture destination"));
        };
        let mut dest = usize::from(first);
        for (c, v) in value.into_iter().enumerate() {
            if mask & (1 << c) != 0 {
                if dest < 255 {
                    self.registers[dest] = v;
                }
                dest += 1;
            }
        }
        Ok(())
    }
}
