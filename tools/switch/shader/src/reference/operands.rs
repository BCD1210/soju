use super::*;
impl Machine {
    pub(super) fn value(&self, x: &Instruction, i: usize, memory: &Memory) -> Result<u32> {
        match x.operands.get(i) {
            Some(Operand::Register(r)) => Ok(self.register(*r)),
            Some(Operand::Immediate(v) | Operand::FloatBits(v)) => Ok(*v),
            Some(Operand::SystemRegister(id)) => match id {
                0x21..=0x23 => Ok(self.local_id[usize::from(id - 0x21)]),
                0x25..=0x27 => Ok(self.group_id[usize::from(id - 0x25)]),
                _ => Err(x.error("unsupported system register")),
            },
            Some(Operand::Constant { slot, base, offset })
                if *base == 255 && *offset >= 0 && offset % 4 == 0 =>
            {
                memory
                    .constants
                    .get(slot)
                    .and_then(|c| c.get((*offset / 4) as usize))
                    .copied()
                    .ok_or_else(|| x.error("constant buffer out of bounds"))
            }
            _ => Err(x.error("unsupported source operand")),
        }
    }
    pub(super) fn int(&self, x: &Instruction, i: usize, memory: &Memory) -> Result<u32> {
        let mut v = self.value(x, i, memory)?;
        if x.modified("abs", Some(i)) {
            v = (v as i32).unsigned_abs();
        }
        if x.modified("neg", Some(i)) {
            v = v.wrapping_neg();
        }
        if x.modified("inv", Some(i)) {
            v = !v;
        }
        Ok(v)
    }
    pub(super) fn float(&self, x: &Instruction, i: usize, memory: &Memory) -> Result<f32> {
        let mut v = self.value(x, i, memory)?;
        if x.modified("abs", Some(i)) {
            v &= 0x7fffffff;
        }
        if x.modified("neg", Some(i)) {
            v ^= 0x80000000;
        }
        Ok(f32::from_bits(v))
    }
}
pub(super) fn ftz(v: f32) -> f32 {
    let b = v.to_bits();
    if b & 0x7f800000 == 0 {
        f32::from_bits(b & 0x80000000)
    } else {
        v
    }
}
pub(super) fn rounded(v: f32, mode: u32) -> f32 {
    match mode {
        0 => v.round_ties_even(),
        1 => v.floor(),
        2 => v.ceil(),
        _ => v.trunc(),
    }
}
