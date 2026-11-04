use super::*;
impl Emitter<'_> {
    pub(super) fn attribute(&self, x: &Instruction) -> Result<String> {
        if self.compute.is_some() {
            return Err(x.error("graphics attribute in compute program"));
        }
        let store = x.opcode == Opcode::AST;
        let index = if store { 0 } else { 1 };
        let Some(Operand::Memory {
            space: Space::Attribute,
            base: 255,
            offset,
        }) = x.operands.get(index)
        else {
            return Err(x.error("indexed attribute access is not supported"));
        };
        if offset % 4 != 0 || *offset < 0 {
            return Err(x.error("unaligned attribute access"));
        }
        if x.opcode != Opcode::IPA && register_operand(x, 2)? != 255 {
            return Err(x.error("per-vertex attribute indexing is not supported"));
        }
        if x.opcode == Opcode::IPA {
            if self.header.stage != Stage::Fragment {
                return Err(x.error("IPA requires fragment stage"));
            }
            if register_operand(x, 2)? != 255
                || register_operand(x, 3)? != 255
                || predicate_operand(x, 4)? != "true"
            {
                return Err(x.error("IPA arithmetic or sample predicate is not supported"));
            }
        } else if self.header.stage != Stage::Vertex {
            return Err(x.error("ALD/AST backend requires vertex stage"));
        }
        let count = if x.opcode == Opcode::IPA {
            1
        } else {
            bits(x.raw, 47, 2) + 1
        };
        let first = register_operand(x, if store { 1 } else { 0 })?;
        if u32::from(first) + count > 256 {
            return Err(x.error("attribute register range overflow"));
        }
        let mut s = String::new();
        for c in 0..count {
            let address = (*offset as u32 + c * 4) as u16;
            let map = if store {
                &self.header.outputs
            } else {
                &self.header.inputs
            };
            let attr = map
                .iter()
                .find(|a| a.address == address)
                .ok_or_else(|| x.error(format!("attribute 0x{address:x} absent from SPH")))?;
            let r = (u32::from(first) + c) as u8;
            if store {
                let field = if address == 0x6c {
                    "point_size".into()
                } else if (0x2c0..0x2e0).contains(&address) {
                    format!("clip_distance[{}]", (address - 0x2c0) / 4)
                } else if (0x70..0x80).contains(&address) {
                    format!("position[{}]", (address - 0x70) / 4)
                } else {
                    format!("v{}", address / 4)
                };
                writeln!(s, "out.{field} = as_type<float>({});", reg(r)).unwrap();
            } else {
                let field = if self.header.stage == Stage::Vertex {
                    format!("a{}[{}]", (address - 0x80) / 16, (address % 16) / 4)
                } else if (0x70..0x80).contains(&address) {
                    format!("position[{}]", (address - 0x70) / 4)
                } else {
                    format!("v{}", address / 4)
                };
                if x.opcode == Opcode::IPA
                    && bits(x.raw, 54, 2) == 2
                    && attr.interpolation != Some(Interpolation::Flat)
                {
                    return Err(x.error("IPA.CONSTANT requires flat SPH interpolation"));
                }
                if r != 255 {
                    let value = if x.modified("sat", None) {
                        format!("soju_sat(in.{field})")
                    } else {
                        format!("in.{field}")
                    };
                    writeln!(s, "r[{r}] = as_type<uint>({value});").unwrap();
                }
            }
        }
        Ok(s)
    }
    pub(super) fn local(&self, x: &Instruction) -> Result<String> {
        let store = x.opcode == Opcode::STL;
        let index = if store { 0 } else { 1 };
        let Some(Operand::Memory {
            space: Space::Local,
            base: 255,
            offset,
        }) = x.operands.get(index)
        else {
            return Err(x.error("only static local addresses are supported"));
        };
        let adjusted = i64::from(*offset) + i64::from(self.header.local_negative);
        if adjusted < 0 || adjusted % 4 != 0 || adjusted + 4 > i64::from(self.local_bytes) {
            return Err(x.error("local address outside SPH allocation"));
        }
        if store {
            Ok(format!(
                "local_mem[{}] = {};\n",
                adjusted / 4,
                source(x, 1, false)?
            ))
        } else {
            assign(x, 0, &format!("local_mem[{}]", adjusted / 4), false)
        }
    }
}
