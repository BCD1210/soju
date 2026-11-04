use super::{Access, Action, Error, ExecutionState, Method, invalid, surface};
impl ExecutionState {
    pub(super) fn effect(&mut self, command: Method, access: &dyn Access) -> Result<bool, Error> {
        let Method {
            subchannel: s,
            method: m,
            value: v,
            ..
        } = command;
        let class = self.bindings.get(&s).copied();
        match (class, m) {
            (Some(0xb197), 0x114 | 0x118 | 0x11c | 0x120) => self.mme.write(m, v)?,
            (Some(0xb1c0 | 0xa140), 0x1b0) => {
                if !matches!(v, 0x41 | 0x1001) || self.inline_uploads.contains_key(&s) {
                    return Err(self.unsupported(command));
                }
                let target =
                    (u64::from(self.reg(s, 0x188)?) << 32) | u64::from(self.reg(s, 0x18c)?);
                let length = self.reg(s, 0x180)?;
                let lines = self.reg(s, 0x184)?;
                if lines != 1
                    || length == 0
                    || !length.is_multiple_of(4)
                    || length > 8 * 1024 * 1024
                    || !target.is_multiple_of(4)
                    || target + u64::from(length) > 1 << 40
                    || !access.check(
                        target,
                        usize::try_from(length).map_err(|_| invalid("inline length"))?,
                        0,
                        2,
                    )
                {
                    return Err(self.unsupported(command));
                }
                self.inline_uploads.insert(s, (target, length, v == 0x1001));
            }
            (Some(0xb1c0 | 0xa140), 0x1b4) => {
                let (address, remaining, barrier) = *self
                    .inline_uploads
                    .get(&s)
                    .ok_or(invalid("inline data without launch"))?;
                self.pending_actions.push(Action::Write {
                    address,
                    bytes: v.to_le_bytes().to_vec(),
                    kind: 0,
                });
                if remaining == 4 {
                    self.inline_uploads.remove(&s);
                    if barrier {
                        self.pending_actions.push(Action::Barrier);
                    }
                } else {
                    self.inline_uploads
                        .insert(s, (address + 4, remaining - 4, barrier));
                }
            }
            (Some(0xb1c0), 0x1698) => {
                if v != 0x1000 {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Barrier);
            }
            (Some(0xb1c0), 0x110) => {
                if v != 0 || self.inflight_work {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Barrier);
            }
            (Some(0xb1c0), 0x248) => {
                if v & !0xffff3f != 0 {
                    return Err(self.unsupported(command));
                }
                self.compute_ref_counters
                    .insert((s, v & 63), (v >> 8) & 0xffff);
            }
            (Some(0xb197), 0x21c) => {
                if v != 0x1011 || self.inflight_work {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Barrier);
            }
            (Some(0xb197), 0x1144 | 0xf80) => {
                if v != 0 || self.inflight_work {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Barrier);
            }
            (Some(0xb197), 0x2c8) => {
                if v & !0x110fff != 0 {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Increment {
                    syncpoint: v & 0xfff,
                    increment: (v >> 20) & 1,
                    wait: v & 0x10000 != 0,
                });
            }
            (Some(0xb197), 0x2410..=0x2490) if (m - 0x2410) % 32 == 0 => {
                if v & !0x1f1 != 0 {
                    return Err(self.unsupported(command));
                }
                let key = (s, (m - 0x2410) / 32, (v >> 4) & 31);
                if v & 1 != 0 {
                    let size = self.reg(s, 0x2380)?;
                    let address =
                        (u64::from(self.reg(s, 0x2384)?) << 32) | u64::from(self.reg(s, 0x2388)?);
                    if size == 0 || address + u64::from(size) > 1 << 40 {
                        return Err(self.unsupported(command));
                    }
                    self.constant_bindings.insert(key, (address, size));
                } else {
                    self.constant_bindings.remove(&key);
                }
            }
            (Some(0xb197), 0x2390) => {
                let size = self.reg(s, 0x2380)?;
                let offset = self.reg(s, 0x238c)?;
                let address =
                    (u64::from(self.reg(s, 0x2384)?) << 32) | u64::from(self.reg(s, 0x2388)?);
                if !offset.is_multiple_of(4)
                    || u64::from(offset) + 4 > u64::from(size)
                    || address + u64::from(offset) + 4 > 1 << 40
                    || !access.check(address + u64::from(offset), 4, 0, 2)
                {
                    return Err(self.unsupported(command));
                }
                self.pending_actions.push(Action::Write {
                    address: address + u64::from(offset),
                    bytes: v.to_le_bytes().to_vec(),
                    kind: 0,
                });
                self.registers.insert((s, 0x238c), offset + 4);
            }
            (Some(0xb197), 0x19d0) => self.clear(command, access)?,
            (Some(0xb0b5), 0x300) => self.copy(command, access)?,
            _ => return Ok(false),
        }
        Ok(true)
    }
    fn clear(&mut self, command: Method, access: &dyn Access) -> Result<(), Error> {
        let s = command.subchannel;
        let expected = [
            (0x808, 1280),
            (0x80c, 720),
            (0x810, 0xd5),
            (0x814, 0x40),
            (0x818, 1),
            (0x820, 0),
            (0xff4, 0x05000000),
            (0xff8, 0x02d00000),
            (0x10f8, 0),
            (0x15d0, 0),
            (0x1558, 1),
            (0x19e0, 0),
            (0xd80, 0),
            (0xd84, 0),
            (0xd88, 0),
            (0xd8c, 0x3f800000),
        ];
        if command.value != 0x3c
            || expected
                .iter()
                .any(|&(m, v)| self.registers.get(&(s, m)) != Some(&v))
            || self.reg(s, 0x121c)? & 0x7f != 1
        {
            return Err(self.unsupported(command));
        }
        let address = (u64::from(self.reg(s, 0x800)?) << 32) | u64::from(self.reg(s, 0x804)?);
        if !address.is_multiple_of(0x10000) {
            return Err(self.unsupported(command));
        }
        let writes = surface::clear(address, 1280, 720, 4, [0, 0, 0, 255])?;
        if writes
            .iter()
            .any(|(a, b)| !access.check(*a, b.len(), 0xfe, 2))
        {
            return Err(invalid("color clear target unavailable"));
        }
        self.pending_actions
            .extend(writes.into_iter().map(|(address, bytes)| Action::Write {
                address,
                bytes,
                kind: 0xfe,
            }));
        Ok(())
    }
    fn copy(&mut self, command: Method, access: &dyn Access) -> Result<(), Error> {
        let s = command.subchannel;
        let expected = [
            (0x708, 0x03303210),
            (0x70c, 0x1040),
            (0x710, 301),
            (0x714, 76),
            (0x718, 1),
            (0x71c, 0),
            (0x720, 0),
            (0x410, 1204),
            (0x414, 1216),
            (0x418, 301),
            (0x41c, 76),
        ];
        if command.value != 0x686
            || expected
                .iter()
                .any(|&(m, v)| self.registers.get(&(s, m)) != Some(&v))
        {
            return Err(self.unsupported(command));
        }
        let source = (u64::from(self.reg(s, 0x400)?) << 32) | u64::from(self.reg(s, 0x404)?);
        let target = (u64::from(self.reg(s, 0x408)?) << 32) | u64::from(self.reg(s, 0x40c)?);
        if !source.is_multiple_of(4) || !target.is_multiple_of(0x10000) {
            return Err(self.unsupported(command));
        }
        let chunks = surface::copy(source, target, 301, 76, 1204, 4)?;
        if (0..76).any(|row| !access.check(source + row * 1204, 1204, 0, 1))
            || chunks
                .iter()
                .any(|&(_, dst, size)| !access.check(dst, size, 0xfe, 2))
        {
            return Err(invalid("GPU copy source or target unavailable"));
        }
        self.pending_actions.push(Action::Copy { chunks });
        Ok(())
    }
}
