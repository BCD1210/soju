use crate::{Error, Result};
#[path = "encodings.rs"]
mod encodings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Opcode {
    FFMA,
    FADD,
    FMUL,
    FMNMX,
    FSET,
    FSETP,
    MUFU,
    F2F,
    F2I,
    I2F,
    I2I,
    IADD,
    IADD3,
    ISCADD,
    IMAD,
    XMAD,
    ISET,
    ISETP,
    IMNMX,
    LOP,
    LOP3,
    SHL,
    SHR,
    BFE,
    BFI,
    POPC,
    FLO,
    SEL,
    MOV,
    PSETP,
    P2R,
    R2P,
    LD,
    ST,
    LDG,
    STG,
    LDS,
    STS,
    LDL,
    STL,
    LDC,
    IPA,
    ALD,
    AST,
    OUT,
    TEX,
    TEXS,
    TLD,
    TLDS,
    TLD4,
    TXQ,
    BRA,
    SSY,
    SYNC,
    PBK,
    BRK,
    EXIT,
    KIL,
    NOP,
    DEPBAR,
    BAR,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Predicate {
    pub index: u8,
    pub inverted: bool,
}
impl Predicate {
    pub const ALWAYS: Self = Self {
        index: 7,
        inverted: false,
    };
    pub fn is_always(self) -> bool {
        self == Self::ALWAYS
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Space {
    Attribute,
    Local,
    Shared,
    Global,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operand {
    Register(u8),
    Predicate(Predicate),
    Immediate(u32),
    FloatBits(u32),
    Constant { slot: u8, base: u8, offset: i32 },
    Memory { space: Space, base: u8, offset: i32 },
    Target(u32),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Modifier {
    pub name: &'static str,
    /// None means instruction-wide; otherwise indexes `operands` (including destinations).
    pub operand: Option<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionField {
    /// Stable envytools subtable identifier, or a literal mode name (mask == 0).
    pub name: &'static str,
    pub mask: u64,
    pub bits: u64,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instruction {
    /// Byte offset relative to the beginning of code, including scheduling words.
    pub address: u32,
    pub raw: u64,
    pub opcode: Opcode,
    pub predicate: Predicate,
    pub operands: Vec<Operand>,
    pub modifiers: Vec<Modifier>,
    pub options: Vec<OptionField>,
    pub encoding: u64,
}
impl Instruction {
    pub fn error(&self, message: impl std::fmt::Display) -> Error {
        Error(format!(
            "{message} at 0x{:x}: 0x{:016x} ({:?})",
            self.address, self.raw, self.opcode
        ))
    }
    pub fn target(&self) -> Option<u32> {
        self.operands.iter().find_map(|o| {
            if let Operand::Target(t) = o {
                Some(*t)
            } else {
                None
            }
        })
    }
    pub fn modified(&self, name: &str, operand: Option<usize>) -> bool {
        self.modifiers
            .iter()
            .any(|m| m.name == name && m.operand == operand)
    }
}
struct Encoding {
    opcode: Opcode,
    value: u64,
    mask: u64,
    predicated: bool,
    fields: &'static str,
    flags: &'static [(u8, &'static str, bool, i32)],
    options: &'static [(&'static str, u64)],
}
pub(crate) fn bits(raw: u64, start: u32, width: u32) -> u32 {
    ((raw >> start) & ((1u64 << width) - 1)) as u32
}
fn signed(value: u32, width: u32) -> i32 {
    ((value << (32 - width)) as i32) >> (32 - width)
}

/// Decode one instruction. Scheduling words must be removed by the caller or `decode`.
pub fn decode_instruction(address: u32, raw: u64) -> Result<Instruction> {
    let e = encodings::ENCODINGS
        .iter()
        .find(|e| raw & e.mask == e.value)
        .ok_or_else(|| Error(format!("unknown opcode at 0x{address:x}: 0x{raw:016x}")))?;
    let predicate = if e.predicated {
        Predicate {
            index: bits(raw, 16, 3) as u8,
            inverted: bits(raw, 19, 1) != 0,
        }
    } else {
        Predicate::ALWAYS
    };
    let mut operands = Vec::new();
    for field in e.fields.split_whitespace() {
        let reg = |p| bits(raw, p, 8) as u8;
        let operand = if let Some(p) = field.strip_prefix("REG_") {
            Operand::Register(reg(p.parse().unwrap()))
        } else if field.starts_with("PRED") || field.starts_with('p') || field == "INV_PRED41" {
            let (p, invert) = if field == "INV_PRED41" {
                (41, true)
            } else {
                (
                    field
                        .trim_start_matches("PRED")
                        .trim_start_matches('p')
                        .parse()
                        .unwrap(),
                    false,
                )
            };
            Operand::Predicate(Predicate {
                index: bits(raw, p, 3) as u8 ^ if invert { 7 } else { 0 },
                inverted: false,
            })
        } else if field == "BTARG" {
            let target = i64::from(address) + 8 + i64::from(signed(bits(raw, 20, 24), 24));
            Operand::Target(u32::try_from(target).map_err(|_| {
                Error(format!(
                    "branch target overflow at 0x{address:x}: 0x{raw:016x}"
                ))
            })?)
        } else if field.starts_with('C') {
            let short = field.starts_with("C34");
            Operand::Constant {
                slot: bits(raw, if short { 34 } else { 36 }, 5) as u8,
                base: if field.contains("_08_") { reg(8) } else { 255 },
                offset: if short {
                    signed(bits(raw, 20, 14), 14) * 4
                } else {
                    signed(bits(raw, 20, 16), 16)
                },
            }
        } else if field.contains("MEM") {
            let (space, start, width, sign) = match field {
                "AMEM" => (Space::Attribute, 20, 10, false),
                "AMEM28" => (Space::Attribute, 28, 10, false),
                "AMEMIDX" => (Space::Attribute, 20, 0, false),
                "LMEM" => (Space::Local, 20, 24, true),
                "SMEM" => (Space::Shared, 20, 24, true),
                "GMEM" => (Space::Global, 20, 32, true),
                "NCGMEM_20" => (Space::Global, 20, 20, true),
                "NCGMEM" => (Space::Global, 20, 24, true),
                _ => unreachable!("static memory field"),
            };
            let v = bits(raw, start, width);
            Operand::Memory {
                space,
                base: reg(8),
                offset: if sign { signed(v, width) } else { v as i32 },
            }
        } else {
            let (width, start) = field[1..].split_once('_').unwrap();
            let width: u32 = width.parse().unwrap();
            let start = start.parse().unwrap();
            let v = if width == 20 && start == 20 {
                bits(raw, 20, 19) | (bits(raw, 56, 1) << 19)
            } else {
                bits(raw, start, width)
            };
            match field.as_bytes()[0] {
                b'F' => Operand::FloatBits(if width == 20 { v << 12 } else { v }),
                b'S' => Operand::Immediate(signed(v, width) as u32),
                _ => Operand::Immediate(v),
            }
        };
        operands.push(operand);
    }
    let mut modifiers = Vec::new();
    for &(bit, name, zero, index) in e.flags {
        if (bits(raw, u32::from(bit), 1) != 0) != zero {
            let operand = usize::try_from(index).ok();
            if name == "not" {
                if let Some(Operand::Predicate(p)) = operand.and_then(|i| operands.get_mut(i)) {
                    p.inverted = !p.inverted;
                }
            } else {
                modifiers.push(Modifier { name, operand });
            }
        }
    }
    Ok(Instruction {
        address,
        raw,
        opcode: e.opcode,
        predicate,
        operands,
        modifiers,
        options: e
            .options
            .iter()
            .map(|&(name, mask)| OptionField {
                name,
                mask,
                bits: raw & mask,
            })
            .collect(),
        encoding: e.value,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Schedule {
    pub stall: u8,
    pub yield_hint: bool,
    pub write_barrier: u8,
    pub read_barrier: u8,
    pub wait_mask: u8,
    pub reuse_mask: u8,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduledInstruction {
    pub instruction: Instruction,
    pub schedule: Schedule,
}

/// Code begins with a control word followed by three instruction words per bundle.
/// A final partial bundle is accepted, but a dangling control word is not.
pub fn decode(code: &[u8]) -> Result<Vec<ScheduledInstruction>> {
    if code.is_empty() || !code.len().is_multiple_of(8) || code.len() > u32::MAX as usize {
        return Err(Error(
            "code must be a nonempty sequence of 64-bit words".into(),
        ));
    }
    let mut result = Vec::new();
    let mut control = 0;
    for (slot, bytes) in code.chunks_exact(8).enumerate() {
        let raw = u64::from_le_bytes(bytes.try_into().unwrap());
        if slot % 4 == 0 {
            if raw >> 63 != 0 {
                return Err(Error(format!("reserved control bit at 0x{:x}", slot * 8)));
            }
            control = raw;
            continue;
        }
        let s = control >> ((slot % 4 - 1) * 21);
        result.push(ScheduledInstruction {
            instruction: decode_instruction((slot * 8) as u32, raw)?,
            schedule: Schedule {
                stall: bits(s, 0, 4) as u8,
                yield_hint: bits(s, 4, 1) != 0,
                write_barrier: bits(s, 5, 3) as u8,
                read_barrier: bits(s, 8, 3) as u8,
                wait_mask: bits(s, 11, 6) as u8,
                reuse_mask: bits(s, 17, 4) as u8,
            },
        });
    }
    if code.len() / 8 % 4 == 1 {
        return Err(Error("dangling scheduling word".into()));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn forms_and_modifiers() {
        let i = decode_instruction(
            8,
            0x3858_0000_0007_0000 | 2 << 8 | 0x3f800 << 20 | 1 << 48 | 1 << 49 | 1 << 50,
        )
        .unwrap();
        assert_eq!(i.opcode, Opcode::FADD);
        assert_eq!(
            i.operands,
            [
                Operand::Register(0),
                Operand::Register(2),
                Operand::FloatBits(1f32.to_bits())
            ]
        );
        assert!(i.modified("neg", Some(1)));
        assert!(i.modified("abs", Some(2)));
        assert!(i.modified("sat", None));
        let i = decode_instruction(8, 0x4c98_0000_0007_0000 | 3 << 34 | 2 << 20).unwrap();
        assert_eq!(
            i.operands[1],
            Operand::Constant {
                slot: 3,
                base: 255,
                offset: 8
            }
        );
        let i = decode_instruction(8, 0x3898_0000_0007_0000 | 1 << 56 | 0x7ffff << 20).unwrap();
        assert_eq!(i.operands[1], Operand::Immediate(u32::MAX));
    }
    #[test]
    fn schedules_and_errors() {
        let words = [5 | 3 << 17, 0x50b0_0000_0007_0f00, 0xe300_0000_0007_000f];
        let bytes: Vec<_> = words.into_iter().flat_map(u64::to_le_bytes).collect();
        let v = decode(&bytes).unwrap();
        assert_eq!(v.len(), 2);
        assert_eq!(v[0].schedule.stall, 5);
        assert_eq!(v[0].schedule.reuse_mask, 3);
        assert_eq!(v[1].instruction.address, 16);
        assert!(decode(&[0; 8]).is_err());
        assert!(decode(&[0; 9]).is_err());
        let e = decode_instruction(0x88, 0xffffffffffffffff)
            .unwrap_err()
            .to_string();
        assert!(e.contains("0x88") && e.contains("0xffffffffffffffff"));
    }
}
