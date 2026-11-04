use super::*;
impl Emitter<'_> {
    pub(super) fn predicate_instruction(&self, x: &Instruction) -> Result<String> {
        use Opcode::*;
        let dst = |v: String| assign(x, 0, &v, false);
        match x.opcode {
            FSET | FSETP | ISET | ISETP => {
                let float = matches!(x.opcode, FSET | FSETP);
                let setp = matches!(x.opcode, FSETP | ISETP);
                let first = if setp { 2 } else { 1 };
                let mut left = source(x, first, float)?;
                let mut right = source(x, first + 1, float)?;
                if !float && bits(x.raw, 48, 1) != 0 {
                    left = format!("as_type<int>({left})");
                    right = format!("as_type<int>({right})");
                }
                let c = compare(
                    &left,
                    &right,
                    bits(
                        x.raw,
                        if float { 48 } else { 49 },
                        if float { 4 } else { 3 },
                    ),
                    float,
                );
                let p = predicate_operand(x, first + 2)?;
                let op = bits(x.raw, 45, 2);
                let yes = bool_op(x, "test", &p, op)?;
                if setp {
                    let no = bool_op(x, "(!test)", &p, op)?;
                    let mut s = format!("bool test = {c};\nbool yes = {yes};\nbool no = {no};\n");
                    for (i, v) in [(0, "yes"), (1, "no")] {
                        if let Operand::Predicate(p) = x.operands[i] {
                            if p.index != 7 {
                                writeln!(s, "p[{}] = {v};", p.index).unwrap();
                            }
                        }
                    }
                    Ok(s)
                } else {
                    let bf = bits(x.raw, if float { 52 } else { 44 }, 1) != 0;
                    Ok(format!(
                        "bool test = {c};\n{}",
                        dst(format!(
                            "({yes} ? {} : 0u)",
                            if bf { "0x3f800000u" } else { "0xffffffffu" }
                        ))?
                    ))
                }
            }
            PSETP => {
                let first = bool_op(
                    x,
                    &predicate_operand(x, 2)?,
                    &predicate_operand(x, 3)?,
                    bits(x.raw, 24, 2),
                )?;
                let c = predicate_operand(x, 4)?;
                let op = bits(x.raw, 45, 2);
                let mut s = format!(
                    "bool test = {first};\nbool yes = {};\nbool no = {};\n",
                    bool_op(x, "test", &c, op)?,
                    bool_op(x, "(!test)", &c, op)?
                );
                for (i, v) in [(0, "yes"), (1, "no")] {
                    if let Operand::Predicate(p) = x.operands[i] {
                        if p.index != 7 {
                            writeln!(s, "p[{}] = {v};", p.index).unwrap();
                        }
                    }
                }
                Ok(s)
            }
            _ => Err(x.error("expected predicate instruction")),
        }
    }
}
