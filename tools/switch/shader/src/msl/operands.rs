use super::*;
pub(super) fn reg(r: u8) -> String {
    if r == 255 {
        "0u".into()
    } else {
        format!("r[{r}]")
    }
}
pub(super) fn pred(p: Predicate) -> String {
    let v = if p.index == 7 {
        "true".into()
    } else {
        format!("p[{}]", p.index)
    };
    if p.inverted {
        format!("(!{v})")
    } else {
        v
    }
}
pub(super) fn predicate_operand(x: &Instruction, i: usize) -> Result<String> {
    match x.operands.get(i) {
        Some(Operand::Predicate(p)) => Ok(pred(*p)),
        _ => Err(x.error("expected predicate operand")),
    }
}
pub(super) fn register_operand(x: &Instruction, i: usize) -> Result<u8> {
    match x.operands.get(i) {
        Some(Operand::Register(r)) => Ok(*r),
        _ => Err(x.error("expected register operand")),
    }
}
pub(super) fn source(x: &Instruction, i: usize, float: bool) -> Result<String> {
    let raw = match x.operands.get(i) {
        Some(Operand::Register(r)) => reg(*r),
        Some(Operand::Immediate(v) | Operand::FloatBits(v)) => format!("0x{v:08x}u"),
        Some(Operand::Constant { slot, base, offset }) => {
            if *base != 255 || *offset < 0 || offset % 4 != 0 {
                return Err(x.error("only aligned direct cbuf operands are supported"));
            }
            format!("cb{slot}[{}]", offset / 4)
        }
        _ => return Err(x.error("unsupported source operand")),
    };
    let mut value = if float {
        format!("as_type<float>({raw})")
    } else {
        raw
    };
    if x.modified("abs", Some(i)) {
        value = if float {
            format!("abs({value})")
        } else {
            format!("uint(abs(as_type<int>({value})))")
        };
    }
    if x.modified("neg", Some(i)) {
        value = if float {
            format!("(-{value})")
        } else {
            format!("(0u - {value})")
        };
    }
    if x.modified("inv", Some(i)) {
        value = format!("(~{value})");
    }
    Ok(value)
}
pub(super) fn assign(x: &Instruction, operand: usize, value: &str, float: bool) -> Result<String> {
    let r = register_operand(x, operand)?;
    let mut value = value.to_string();
    if float {
        if x.modified("sat", None) {
            value = format!("soju_sat({value})");
        }
        value = format!("as_type<uint>({value})");
    }
    Ok(if r == 255 {
        format!("(void)({value});\n")
    } else {
        format!("r[{r}] = {value};\n")
    })
}
pub(super) fn bool_op(x: &Instruction, a: &str, b: &str, mode: u32) -> Result<String> {
    let op = match mode {
        0 => "&&",
        1 => "||",
        2 => "!=",
        _ => return Err(x.error("reserved boolean operation")),
    };
    Ok(format!("({a} {op} {b})"))
}
pub(super) fn compare(a: &str, b: &str, mode: u32, float: bool) -> String {
    let unordered = if float {
        format!("(isnan({a}) || isnan({b}))")
    } else {
        "false".into()
    };
    let base = match mode & 7 {
        0 => "false".into(),
        1 => format!("({a} < {b})"),
        2 => format!("({a} == {b})"),
        3 => format!("({a} <= {b})"),
        4 => format!("({a} > {b})"),
        5 => format!("({a} != {b})"),
        6 => format!("({a} >= {b})"),
        _ => "true".into(),
    };
    if !float {
        base
    } else if mode & 8 != 0 {
        format!("({unordered} || {base})")
    } else {
        format!("(!{unordered} && {base})")
    }
}
