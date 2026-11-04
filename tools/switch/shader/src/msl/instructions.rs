use super::*;
impl Emitter<'_> {
    pub(super) fn instruction(&self, x: &Instruction) -> Result<String> {
        use Opcode::*;
        self.validate(x)?;
        let a = |i| source(x, i, false);
        let f = |i| source(x, i, true);
        let dst = |v: String| assign(x, 0, &v, false);
        let fdst = |v: String| assign(x, 0, &v, true);
        match x.opcode {
            NOP | DEPBAR => Ok(String::new()),
            EXIT => Ok(self.finish()),
            KIL => {
                if self.header.stage == Stage::Fragment {
                    Ok(format!("discard_fragment();\n{}", self.finish()))
                } else {
                    Err(x.error("KIL requires fragment stage"))
                }
            }
            MOV => dst(a(1)?),
            FADD | FMUL | FFMA => {
                let ftz = x.modified("ftz", None)
                    || x.options.iter().any(|o| {
                        (o.name == "5c68_0" && o.bits == 1 << 44)
                            || (o.name == "5980_0" && o.bits == 1 << 53)
                    });
                let value = |i| -> Result<String> {
                    let v = f(i)?;
                    Ok(if ftz { format!("soju_ftz({v})") } else { v })
                };
                let v = match x.opcode {
                    FADD => format!("({} + {})", value(1)?, value(2)?),
                    FMUL => format!("({} * {})", value(1)?, value(2)?),
                    _ => format!("fma({}, {}, {})", value(1)?, value(2)?, value(3)?),
                };
                fdst(if ftz { format!("soju_ftz({v})") } else { v })
            }
            FMNMX => fdst(format!(
                "({} ? fmin({}, {}) : fmax({}, {}))",
                predicate_operand(x, 3)?,
                f(1)?,
                f(2)?,
                f(1)?,
                f(2)?
            )),
            IADD => dst(format!("({} + {})", a(1)?, a(2)?)),
            IADD3 => dst(format!("({} + {} + {})", a(1)?, a(2)?, a(3)?)),
            ISCADD => dst(format!("(({} << {}) + {})", a(1)?, a(3)?, a(2)?)),
            IMAD => dst(format!("({} * {} + {})", a(1)?, a(2)?, a(3)?)),
            XMAD => dst(format!(
                "(({} & 0xffffu) * ({} & 0xffffu) + {})",
                a(1)?,
                a(2)?,
                a(3)?
            )),
            SEL => dst(format!(
                "({} ? {} : {})",
                predicate_operand(x, 3)?,
                a(1)?,
                a(2)?
            )),
            IMNMX => {
                let signed = bits(x.raw, 48, 1) != 0;
                let cast = if signed { "as_type<int>" } else { "uint" };
                dst(format!(
                    "uint({} ? min({cast}({}), {cast}({})) : max({cast}({}), {cast}({})))",
                    predicate_operand(x, 3)?,
                    a(1)?,
                    a(2)?,
                    a(1)?,
                    a(2)?
                ))
            }
            FSET | FSETP | ISET | ISETP | PSETP => self.predicate_instruction(x),
            LOP => {
                let op = bits(x.raw, 41, 2);
                let b = a(3)?;
                let v = match op {
                    0 => format!("({} & {b})", a(2)?),
                    1 => format!("({} | {b})", a(2)?),
                    2 => format!("({} ^ {b})", a(2)?),
                    _ => b,
                };
                assign(x, 1, &v, false)
            }
            LOP3 => assign(
                x,
                1,
                &format!("soju_lop3({}, {}, {}, {})", a(2)?, a(3)?, a(4)?, a(5)?),
                false,
            ),
            SHL | SHR => {
                let right = x.opcode == SHR;
                let signed = right && bits(x.raw, 48, 1) != 0;
                let value = if signed {
                    format!("as_type<int>({})", a(1)?)
                } else {
                    a(1)?
                };
                let amount = if x.modified("w", None) {
                    format!("({} & 31u)", a(2)?)
                } else {
                    a(2)?
                };
                dst(format!(
                    "({amount} >= 32u ? {} : uint({value} {} {amount}))",
                    if signed {
                        format!("uint({value} >> 31u)")
                    } else {
                        "0u".into()
                    },
                    if right { ">>" } else { "<<" }
                ))
            }
            POPC => dst(format!("popcount({})", a(1)?)),
            FLO => {
                let v = a(1)?;
                let v = if bits(x.raw, 48, 1) != 0 {
                    format!("(as_type<int>({v}) < 0 ? ~{v} : {v})")
                } else {
                    v
                };
                dst(format!("({v} == 0u ? 0xffffffffu : 31u - clz({v}))"))
            }
            BFE => dst(format!(
                "soju_bfe({}, {}, {})",
                a(1)?,
                a(2)?,
                bits(x.raw, 48, 1) != 0
            )),
            BFI => dst(format!("soju_bfi({}, {}, {})", a(1)?, a(2)?, a(3)?)),
            MUFU => {
                let v = f(1)?;
                let expr = match bits(x.raw, 20, 4) {
                    2 => format!("exp2({v})"),
                    3 => format!("log2({v})"),
                    4 => format!("(1.0f / {v})"),
                    5 => format!("rsqrt({v})"),
                    8 => format!("sqrt({v})"),
                    _ => {
                        return Err(
                            x.error("MUFU function requires range reduction or FP64 emulation")
                        )
                    }
                };
                fdst(expr)
            }
            I2F => fdst(format!(
                "float({}({}))",
                if bits(x.raw, 13, 1) != 0 {
                    "as_type<int>"
                } else {
                    "uint"
                },
                a(1)?
            )),
            F2I => dst(format!(
                "soju_f2i({}, {}, {})",
                f(1)?,
                bits(x.raw, 12, 1) != 0,
                bits(x.raw, 39, 2)
            )),
            F2F => {
                let v = f(1)?;
                let mode = bits(x.raw, 39, 4) & 11;
                fdst(match mode {
                    0 | 3 => v,
                    8 => format!("rint({v})"),
                    9 => format!("floor({v})"),
                    10 => format!("ceil({v})"),
                    11 => format!("trunc({v})"),
                    _ => return Err(x.error("F2F rounding mode unsupported")),
                })
            }
            I2I => dst(a(1)?),
            LDC => {
                let Some(Operand::Constant { slot, base, offset }) = x.operands.get(1) else {
                    return Err(x.error("invalid LDC operand"));
                };
                if *base != 255 || offset % 4 != 0 || *offset < 0 {
                    return Err(x.error("indexed/unaligned LDC not supported"));
                }
                dst(format!("cb{slot}[{}]", offset / 4))
            }
            ALD | AST | IPA => self.attribute(x),
            LDL | STL => self.local(x),
            TEX => self.texture(x),
            _ => Err(x.error("opcode decoded but Metal lowering is not implemented")),
        }
    }
}
