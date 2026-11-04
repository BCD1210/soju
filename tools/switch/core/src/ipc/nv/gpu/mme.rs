use super::{Error, invalid};
use std::collections::BTreeMap;
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct MacroUpload {
    instruction_pointer: u32,
    entry_pointer: u32,
    instructions: BTreeMap<u32, u32>,
    entries: BTreeMap<u32, u32>,
}
impl MacroUpload {
    pub fn write(&mut self, method: u32, value: u32) -> Result<(), Error> {
        match method {
            0x114 if value < 16384 => self.instruction_pointer = value,
            0x118 if self.instruction_pointer < 16384 => {
                self.instructions.insert(self.instruction_pointer, value);
                self.instruction_pointer += 1;
            }
            0x11c if value < 128 => self.entry_pointer = value,
            0x120 if self.entry_pointer < 128 => {
                self.entries.insert(self.entry_pointer, value);
                self.entry_pointer += 1;
            }
            _ => return Err(invalid("invalid MME upload method or bounds")),
        }
        Ok(())
    }
    pub fn run(
        &self,
        entry: u32,
        argument: u32,
        parameters: &[u32],
        read: impl Fn(u32) -> Result<u32, Error>,
    ) -> Result<Vec<(u32, u32)>, Error> {
        if parameters.len() > 256 {
            return Err(invalid("MME parameter limit"));
        }
        let mut pc = i64::from(
            *self
                .entries
                .get(&entry)
                .ok_or(invalid("unknown MME entry"))?,
        );
        let mut registers = [0_u32; 8];
        registers[1] = argument;
        let mut method = None;
        let mut increment = 0;
        let mut emitted = Vec::new();
        let mut parameter_index = 0;
        let mut branch = None;
        let mut finish = false;
        for _ in 0..1_048_576 {
            let word = *self
                .instructions
                .get(&u32::try_from(pc).map_err(|_| invalid("MME PC bounds"))?)
                .ok_or(invalid("MME PC not initialized"))?;
            let op = word & 15;
            let assignment = (word >> 4) & 7;
            let end = word & 0x80 != 0;
            let dst = usize::try_from((word >> 8) & 7).map_err(|_| invalid("MME register"))?;
            let a = usize::try_from((word >> 11) & 7).map_err(|_| invalid("MME register"))?;
            let b = usize::try_from((word >> 14) & 7).map_err(|_| invalid("MME register"))?;
            let raw = word >> 14;
            let imm = i64::from(raw) - if raw & (1 << 17) != 0 { 1 << 18 } else { 0 };
            let mut next = pc + 1;
            let mut new_branch = None;
            let new_exit;
            if op == 7 {
                if branch.is_some() || finish {
                    return Err(invalid("nested MME branch delay"));
                }
                let taken = (registers[a] != 0) == (word & 0x10 != 0);
                new_exit = end && !taken;
                let target = if taken { pc + imm } else { pc + 2 };
                if target < 0 {
                    return Err(invalid("MME branch target bounds"));
                }
                if word & 0x20 != 0 {
                    if taken {
                        next = target;
                    }
                } else {
                    new_branch = Some((target, taken));
                }
            } else {
                let mut result = match op {
                    1 => u32::try_from((i64::from(registers[a]) + imm) & 0xffffffff)
                        .map_err(|_| invalid("MME add"))?,
                    5 => {
                        let index = i64::from(registers[a]) + imm;
                        if !(0..0x1000).contains(&index) {
                            return Err(invalid("MME state bounds"));
                        }
                        let address =
                            u32::try_from(index * 4).map_err(|_| invalid("MME state address"))?;
                        match emitted.iter().rev().find(|&&(m, _)| m == address) {
                            Some(&(_, value)) if (0x3400..0x3800).contains(&address) => value,
                            Some(_) => {
                                return Err(Error::Message(format!(
                                    "MME state read after side-effect emission {address:#x}"
                                )));
                            }
                            None => read(address)?,
                        }
                    }
                    2 => {
                        let source = (word >> 17) & 31;
                        let count = (word >> 22) & 31;
                        let destination = (word >> 27) & 31;
                        if source + count > 32 || destination + count > 32 {
                            return Err(invalid("MME bitfield bounds"));
                        }
                        let mask = ((1_u32 << count) - 1) << destination;
                        (registers[a] & !mask) | (((registers[b] >> source) << destination) & mask)
                    }
                    3 | 4 => {
                        let source = if op == 3 {
                            registers[a]
                        } else {
                            (word >> 17) & 31
                        };
                        let count = (word >> 22) & 31;
                        let destination = if op == 4 {
                            registers[a]
                        } else {
                            (word >> 27) & 31
                        };
                        if source >= 32
                            || destination >= 32
                            || source + count > 32
                            || destination + count > 32
                        {
                            return Err(invalid("MME shifted bitfield bounds"));
                        }
                        ((registers[b] >> source) & ((1u32 << count) - 1)) << destination
                    }
                    0 => match (word >> 17) & 31 {
                        0 => registers[a].wrapping_add(registers[b]),
                        2 => registers[a].wrapping_sub(registers[b]),
                        8 => registers[a] ^ registers[b],
                        11 => registers[a] & !registers[b],
                        12 => !(registers[a] & registers[b]),
                        9 => registers[a] | registers[b],
                        10 => registers[a] & registers[b],
                        _ => {
                            return Err(Error::Message(format!(
                                "unsupported MME ALU word={word:#x} pc={pc}"
                            )));
                        }
                    },
                    _ => {
                        return Err(Error::Message(format!(
                            "unsupported MME opcode={op} pc={pc} word={word:#x}"
                        )));
                    }
                };
                if matches!(assignment, 2 | 5 | 6 | 7) {
                    method = Some((result & 0xfff) * 4);
                    increment = (result >> 12) & 63;
                }
                let send = match assignment {
                    3 | 4 => Some(result),
                    6 => {
                        let parameter = *parameters
                            .get(parameter_index)
                            .ok_or(invalid("MME parameter underflow"))?;
                        parameter_index += 1;
                        Some(parameter)
                    }
                    7 => Some((result >> 12) & 63),
                    _ => None,
                };
                if let Some(value) = send {
                    let selected = method.ok_or(invalid("MME missing output selector"))?;
                    if selected > 0x3ffc || emitted.len() >= 262_144 {
                        return Err(invalid("MME output limit"));
                    }
                    emitted.push((selected, value));
                    if selected == 0x2310 {
                        let mut argument = |address| -> Result<u32, Error> {
                            match emitted.iter().rev().find(|&&(m, _)| m == address) {
                                Some(&(_, value)) => Ok(value),
                                None => read(address),
                            }
                        };
                        complete_context_write(value, &mut argument)?;
                        emitted.push((0x3400, 1));
                    }
                    method = Some(selected + increment * 4);
                }
                if matches!(assignment, 0 | 3 | 5) {
                    result = *parameters
                        .get(parameter_index)
                        .ok_or(invalid("MME parameter underflow"))?;
                    parameter_index += 1;
                }
                if dst != 0 {
                    registers[dst] = result;
                }
                new_exit = end && !branch.is_some_and(|(_, taken)| taken);
            }
            if finish {
                if new_exit || parameter_index != parameters.len() {
                    return Err(invalid("MME exit delay or unused parameters"));
                }
                return Ok(emitted);
            }
            if let Some((target, _)) = branch {
                next = target;
            }
            branch = new_branch;
            finish = new_exit;
            pc = next;
        }
        Err(invalid("MME instruction budget exceeded"))
    }
}

// The public driver polls scratch word zero after these context-configuration
// requests. Host caches/raster hardware own the corresponding physical state;
// only the previously accepted setup profile is acknowledged here.
fn complete_context_write(
    address: u32,
    argument: &mut impl FnMut(u32) -> Result<u32, Error>,
) -> Result<(), Error> {
    let value = argument(0x3404)?;
    let mask = argument(0x3408)?;
    let allowed = match address {
        0x418800 => matches!((value, mask), (1, 1) | (0, 0x1800000)),
        0x419a08 => (value, mask) == (0, 0x10),
        0x419f78 => (value, mask) == (0, 8),
        0x404468 => (value, mask) == (0x7ffffff, 0x3fffffff),
        0x419a04 => matches!((value, mask), (1, 1) | (2, 2)),
        0x418e40 => (0..8).any(|i| value == 7 << (i * 4) && mask == 15 << (i * 4)),
        0x418e44 => (0..3).any(|i| value == 7 << (i * 4) && mask == 15 << (i * 4)),
        0x418e58 => matches!((value, mask), (0x842, 0xffff) | (0x4f10000, 0xffff0000)),
        0x418e5c => matches!((value, mask), (0x53, 0xffff) | (0xe90000, 0xffff0000)),
        0x418e60 => matches!((value, mask), (0xea, 0xffff) | (0xeb0000, 0xffff0000)),
        0x418e64 => matches!((value, mask), (0x208, 0xffff) | (0x2090000, 0xffff0000)),
        0x418e68 => matches!((value, mask), (0x20a, 0xffff) | (0x20b0000, 0xffff0000)),
        0x418e6c => (value, mask) == (0x644, 0xffff),
        _ => false,
    };
    if !allowed || argument(0x3400)? != 0 {
        return Err(Error::Message(format!(
            "unsupported context request {address:#x} value={value:#x} mask={mask:#x}"
        )));
    }
    Ok(())
}
