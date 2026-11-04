use super::*;
fn combine(a: bool, b: bool, mode: u32) -> Result<bool> {
    match mode {
        0 => Ok(a && b),
        1 => Ok(a || b),
        2 => Ok(a ^ b),
        _ => Err(Error("reserved predicate combine".into())),
    }
}
fn compare<T: PartialOrd>(a: T, b: T, mode: u32) -> bool {
    match mode & 7 {
        0 => false,
        1 => a < b,
        2 => a == b,
        3 => a <= b,
        4 => a > b,
        5 => a != b,
        6 => a >= b,
        _ => true,
    }
}
impl Machine {
    pub(super) fn compare(&mut self, x: &Instruction, m: &Memory) -> Result<()> {
        use Opcode::*;
        let setp = matches!(x.opcode, FSETP | ISETP | PSETP);
        let float = matches!(x.opcode, FSET | FSETP);
        let i = if setp { 2 } else { 1 };
        let test = if x.opcode == PSETP {
            combine(self.p(x, 2)?, self.p(x, 3)?, bits(x.raw, 24, 2))?
        } else if float {
            let (mut a, mut b) = (self.float(x, i, m)?, self.float(x, i + 1, m)?);
            if x.modified("ftz", None) {
                if a.is_subnormal() {
                    a = 0.0f32.copysign(a);
                }
                if b.is_subnormal() {
                    b = 0.0f32.copysign(b);
                }
            }
            let mode = bits(x.raw, 48, 4);
            if a.is_nan() || b.is_nan() {
                mode & 8 != 0
            } else {
                compare(a, b, mode)
            }
        } else {
            let (a, b) = (self.int(x, i, m)?, self.int(x, i + 1, m)?);
            let mode = bits(x.raw, 49, 3);
            if bits(x.raw, 48, 1) != 0 {
                compare(a as i32, b as i32, mode)
            } else {
                compare(a, b, mode)
            }
        };
        let c = self.p(x, i + 2)?;
        let mode = bits(x.raw, 45, 2);
        let yes = combine(test, c, mode)?;
        let no = combine(!test, c, mode)?;
        if setp {
            for (i, v) in [(0, yes), (1, no)] {
                if let Operand::Predicate(p) = x.operands[i] {
                    if p.index != 7 {
                        self.predicates[usize::from(p.index)] = v;
                    }
                }
            }
            Ok(())
        } else {
            self.put(
                x,
                0,
                if yes {
                    if bits(x.raw, if float { 52 } else { 44 }, 1) != 0 {
                        1f32.to_bits()
                    } else {
                        u32::MAX
                    }
                } else {
                    0
                },
            )
        }
    }
    pub(super) fn transfer(&mut self, x: &Instruction, m: &Memory) -> Result<()> {
        let shift = bits(x.raw, 41, 2) * 8;
        if x.opcode == Opcode::P2R {
            let mask = (self.int(x, 2, m)? & 127) << shift;
            let packed = self.predicates[..7]
                .iter()
                .enumerate()
                .fold(0u32, |v, (i, p)| v | u32::from(*p) << (i + shift as usize));
            self.put(x, 0, (self.int(x, 1, m)? & !mask) | (packed & mask))
        } else {
            let value = self.int(x, 0, m)? >> shift;
            let mask = self.int(x, 1, m)?;
            for i in 0..7 {
                if mask & (1 << i) != 0 {
                    self.predicates[i] = value & (1 << i) != 0;
                }
            }
            Ok(())
        }
    }
}
