use super::*;
impl Machine {
    pub(super) fn memory(&mut self, x: &Instruction, m: &mut Memory) -> Result<()> {
        let store = matches!(
            x.opcode,
            Opcode::ST | Opcode::STG | Opcode::STS | Opcode::STL | Opcode::AST
        );
        let Some(Operand::Memory {
            space,
            base,
            offset,
        }) = x.operands.get(usize::from(!store))
        else {
            return Err(x.error("missing memory operand"));
        };
        let value = if store { self.int(x, 1, m)? } else { 0 };
        let addr = if x.modified("e", None) {
            if *base >= 254 {
                return Err(x.error("invalid extended address"));
            }
            ((u64::from(self.register(base + 1)) << 32) | u64::from(self.register(*base)))
                .wrapping_add_signed(i64::from(*offset))
        } else {
            u64::from(self.register(*base).wrapping_add_signed(*offset))
        };
        let address = match space {
            Space::Global => addr.wrapping_sub(m.global_base),
            Space::Local => u64::from(
                self.register(*base)
                    .wrapping_add_signed(*offset)
                    .wrapping_add(m.local_negative),
            ),
            _ => addr,
        };
        if *space == Space::Attribute {
            let count = if x.opcode == Opcode::IPA {
                1
            } else {
                bits(x.raw, 47, 2) + 1
            };
            let Operand::Register(first) = x.operands[usize::from(store)] else {
                return Err(x.error("attribute register"));
            };
            for c in 0..count {
                let address = (address + c as u64 * 4) as u16;
                let r = u32::from(first) + c;
                if r >= 256 {
                    return Err(x.error("attribute register range"));
                }
                if store {
                    self.attributes.insert(address, self.register(r as u8));
                } else if r != 255 {
                    self.registers[r as usize] = *self
                        .attributes
                        .get(&address)
                        .ok_or_else(|| x.error("missing input attribute"))?;
                }
            }
            return Ok(());
        }
        let data = match space {
            Space::Global => &mut m.global,
            Space::Shared => &mut m.shared,
            Space::Local => &mut self.local,
            Space::Attribute => unreachable!(),
        };
        let cell = if address % 4 == 0 {
            usize::try_from(address / 4)
                .ok()
                .and_then(|i| data.get_mut(i))
        } else {
            None
        };
        if store {
            if let Some(cell) = cell {
                *cell = value;
            }
            Ok(())
        } else {
            let v = cell.copied().unwrap_or(0);
            self.put(x, 0, v)
        }
    }
}
