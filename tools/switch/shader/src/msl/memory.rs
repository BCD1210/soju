use super::*;

pub(super) fn parameters(
    program: &Program,
    options: &Options,
    cb: &BTreeSet<u8>,
    textures: bool,
    s: &mut String,
) -> Result<()> {
    if !reachability::instructions(program).into_iter().any(|x| {
        matches!(
            x.opcode,
            Opcode::LD | Opcode::ST | Opcode::LDG | Opcode::STG
        )
    }) {
        return Ok(());
    }
    let g = options
        .global_memory
        .ok_or_else(|| Error("global memory requires an address mapping".into()))?;
    if g.buffer > 30
        || cb.contains(&g.buffer)
        || (textures && g.buffer == options.texture_buffer)
        || g.bytes < 4
        || g.bytes % 4 != 0
        || g.base_address % 4 != 0
        || g.base_address.checked_add(u64::from(g.bytes)).is_none()
    {
        return Err(Error("invalid or colliding global memory mapping".into()));
    }
    write!(s, ", device uint* global_mem [[buffer({})]]", g.buffer).unwrap();
    Ok(())
}
impl Emitter<'_> {
    pub(super) fn memory(&self, x: &Instruction) -> Result<String> {
        let store = matches!(x.opcode, Opcode::ST | Opcode::STG | Opcode::STS);
        if matches!(x.opcode, Opcode::LD | Opcode::ST) && predicate_operand(x, 2)? != "true" {
            return Err(x.error("generic LD/ST requires PT address-space predicate"));
        }
        if x.encoding == 0xeec8000000000000 {
            return Err(x.error("LDG predicate result unsupported"));
        }
        let Some(Operand::Memory {
            space,
            base,
            offset,
        }) = x.operands.get(usize::from(!store))
        else {
            return Err(x.error("memory operand missing"));
        };
        let (name, bytes, address) = match space {
            Space::Shared => {
                let launch = self
                    .compute
                    .ok_or_else(|| x.error("shared memory requires compute stage"))?;
                if launch.shared_bytes == 0 {
                    return Err(x.error("shared memory allocation is zero"));
                }
                (
                    "shared_mem",
                    launch.shared_bytes,
                    format!(
                        "ulong({} + {}u)",
                        reg(*base),
                        u32::from_ne_bytes(offset.to_ne_bytes())
                    ),
                )
            }
            Space::Global => {
                let g = self
                    .options
                    .global_memory
                    .ok_or_else(|| x.error("global address mapping missing"))?;
                let addr = if x.modified("e", None) {
                    if *base >= 254 {
                        return Err(x.error("extended address requires two registers"));
                    }
                    format!(
                        "((ulong({}) << 32) | ulong({})) + {}ul",
                        reg(base + 1),
                        reg(*base),
                        u64::from_ne_bytes(i64::from(*offset).to_ne_bytes())
                    )
                } else {
                    format!(
                        "ulong({} + {}u)",
                        reg(*base),
                        u32::from_ne_bytes(offset.to_ne_bytes())
                    )
                };
                (
                    "global_mem",
                    g.bytes,
                    format!("({addr}) - {}ul", g.base_address),
                )
            }
            _ => return Err(x.error("unexpected memory address space")),
        };
        let valid = format!("address < {}ul && (address & 3ul) == 0ul", bytes - 3);
        let line = if store {
            format!(
                "if ({valid}) {name}[address >> 2] = {};\n",
                source(x, 1, false)?
            )
        } else {
            assign(
                x,
                0,
                &format!("(({valid}) ? {name}[address >> 2] : 0u)"),
                false,
            )?
        };
        Ok(format!("ulong address = {address};\n{line}"))
    }
    pub(super) fn predicate_transfer(&self, x: &Instruction) -> Result<String> {
        let shift = bits(x.raw, 41, 2) * 8;
        if x.opcode == Opcode::P2R {
            let mut s = format!(
                "uint mask = ({} & 127u) << {shift};\nuint packed = 0u;\n",
                source(x, 2, false)?
            );
            for i in 0..7 {
                writeln!(s, "packed |= uint(p[{i}]) << {};", i + shift).unwrap();
            }
            s.push_str(&assign(
                x,
                0,
                &format!("(({} & ~mask) | (packed & mask))", source(x, 1, false)?),
                false,
            )?);
            Ok(s)
        } else {
            let mut s = format!(
                "uint value = {} >> {shift};\nuint mask = {};\n",
                source(x, 0, false)?,
                source(x, 1, false)?
            );
            for i in 0..7 {
                writeln!(
                    s,
                    "if (mask & {}u) p[{i}] = (value & {}u) != 0u;",
                    1 << i,
                    1 << i
                )
                .unwrap();
            }
            Ok(s)
        }
    }
}
