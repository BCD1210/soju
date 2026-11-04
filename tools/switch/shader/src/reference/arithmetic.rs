use super::operands::{ftz, rounded};
use super::*;
impl Machine {
    pub(super) fn arithmetic(&mut self, x: &Instruction, m: &Memory) -> Result<()> {
        use Opcode::*;
        let a = |i| self.int(x, i, m);
        let f = |i| self.float(x, i, m);
        let mut dest = 0;
        let integer = match x.opcode {
            MOV | LDC | I2I => a(1)?,
            IADD => a(1)?.wrapping_add(a(2)?),
            IADD3 => a(1)?.wrapping_add(a(2)?).wrapping_add(a(3)?),
            ISCADD => a(1)?.wrapping_shl(a(3)?).wrapping_add(a(2)?),
            IMAD => a(1)?.wrapping_mul(a(2)?).wrapping_add(a(3)?),
            XMAD => (a(1)? & 65535)
                .wrapping_mul(a(2)? & 65535)
                .wrapping_add(a(3)?),
            IMNMX => {
                let (a, b) = (a(1)?, a(2)?);
                let less = if bits(x.raw, 48, 1) != 0 {
                    (a as i32) < (b as i32)
                } else {
                    a < b
                };
                if less == self.p(x, 3)? {
                    a
                } else {
                    b
                }
            }
            SEL => {
                if self.p(x, 3)? {
                    a(1)?
                } else {
                    a(2)?
                }
            }
            LOP => {
                dest = 1;
                let (a, b) = (a(2)?, a(3)?);
                match bits(x.raw, 41, 2) {
                    0 => a & b,
                    1 => a | b,
                    2 => a ^ b,
                    _ => b,
                }
            }
            LOP3 => {
                dest = 1;
                let (a, b, c, lut) = (a(2)?, a(3)?, a(4)?, a(5)?);
                let mut out = 0;
                for bit in 0..32 {
                    let index = ((a >> bit) & 1) * 4 + ((b >> bit) & 1) * 2 + ((c >> bit) & 1);
                    out |= ((lut >> index) & 1) << bit;
                }
                out
            }
            SHL | SHR => {
                let v = a(1)?;
                let count = if x.modified("w", None) {
                    a(2)? & 31
                } else {
                    a(2)?
                };
                let signed = x.opcode == SHR && bits(x.raw, 48, 1) != 0;
                if count >= 32 {
                    if signed && v >> 31 != 0 {
                        u32::MAX
                    } else {
                        0
                    }
                } else if signed {
                    ((v as i32) >> count) as u32
                } else if x.opcode == SHR {
                    v >> count
                } else {
                    v << count
                }
            }
            POPC => a(1)?.count_ones(),
            FLO => {
                let mut v = a(1)?;
                if bits(x.raw, 48, 1) != 0 && v >> 31 != 0 {
                    v = !v;
                }
                if v == 0 {
                    u32::MAX
                } else {
                    31 - v.leading_zeros()
                }
            }
            BFE => {
                let value = a(1)?;
                let control = a(2)?;
                let start = control & 255;
                let width = (control >> 8) & 255;
                let signed = bits(x.raw, 48, 1) != 0;
                if width == 0 {
                    0
                } else if start >= 32 {
                    if signed && value >> 31 != 0 {
                        u32::MAX
                    } else {
                        0
                    }
                } else {
                    let width = width.min(32 - start);
                    let v = value >> start;
                    if width == 32 {
                        v
                    } else {
                        let mask = (1 << width) - 1;
                        let v = v & mask;
                        if signed && v & (1 << (width - 1)) != 0 {
                            v | !mask
                        } else {
                            v
                        }
                    }
                }
            }
            BFI => {
                let insert = a(1)?;
                let control = a(2)?;
                let base = a(3)?;
                let start = control & 255;
                let width = (control >> 8) & 255;
                if start >= 32 || width == 0 {
                    base
                } else {
                    let width = width.min(32 - start);
                    let mask = if width == 32 {
                        u32::MAX
                    } else {
                        ((1 << width) - 1) << start
                    };
                    (base & !mask) | ((insert << start) & mask)
                }
            }
            F2I => {
                let v = rounded(f(1)?, bits(x.raw, 39, 2));
                if bits(x.raw, 12, 1) != 0 {
                    (v as i32) as u32
                } else {
                    v as u32
                }
            }
            FADD | FMUL | FFMA | FMNMX | ReducedSin | ReducedCos | ReducedExp2 | MUFU | F2F
            | I2F => {
                let flush = x.modified("ftz", None)
                    || (x.opcode == FMUL && bits(x.raw, 44, 2) == 1)
                    || (x.opcode == FFMA && bits(x.raw, 53, 2) == 1);
                let f = |i| -> Result<f32> {
                    let v = f(i)?;
                    Ok(if flush { ftz(v) } else { v })
                };
                let mut v = match x.opcode {
                    FADD => f(1)? + f(2)?,
                    FMUL => f(1)? * f(2)?,
                    FFMA => f(1)?.mul_add(f(2)?, f(3)?),
                    FMNMX => {
                        if self.p(x, 3)? {
                            minmax(f(1)?, f(2)?, true)
                        } else {
                            minmax(f(1)?, f(2)?, false)
                        }
                    }
                    ReducedSin => f(1)?.sin(),
                    ReducedCos => f(1)?.cos(),
                    ReducedExp2 => f(1)?.exp2(),
                    MUFU => match bits(x.raw, 20, 4) {
                        2 => f(1)?.exp2(),
                        3 => f(1)?.log2(),
                        4 => 1.0 / f(1)?,
                        5 => 1.0 / f(1)?.sqrt(),
                        8 => f(1)?.sqrt(),
                        _ => return Err(x.error("unsupported MUFU function")),
                    },
                    F2F => {
                        let mode = bits(x.raw, 39, 4) & 11;
                        if mode >= 8 {
                            rounded(f(1)?, mode - 8)
                        } else {
                            f(1)?
                        }
                    }
                    I2F => {
                        if bits(x.raw, 13, 1) != 0 {
                            (a(1)? as i32) as f32
                        } else {
                            a(1)? as f32
                        }
                    }
                    _ => unreachable!(),
                };
                if flush {
                    v = ftz(v);
                }
                if x.modified("sat", None) {
                    v = if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
                }
                v.to_bits()
            }
            _ => return Err(x.error("reference arithmetic unsupported")),
        };
        self.put(x, dest, integer)
    }
}

fn minmax(a: f32, b: f32, min: bool) -> f32 {
    if a == 0.0 && b == 0.0 {
        f32::from_bits(if min {
            a.to_bits() | b.to_bits()
        } else {
            a.to_bits() & b.to_bits()
        })
    } else if min {
        a.min(b)
    } else {
        a.max(b)
    }
}
