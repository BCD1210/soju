use super::{Access, Action, Error, Method, invalid, masks, mme::MacroUpload};
use std::collections::BTreeMap;
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(crate) struct ExecutionState {
    pub(super) bindings: BTreeMap<u32, u32>,
    pub(super) registers: BTreeMap<(u32, u32), u32>,
    pub(super) compute_ref_counters: BTreeMap<(u32, u32), u32>,
    pub(super) constant_bindings: BTreeMap<(u32, u32, u32), (u64, u32)>,
    pub pending_actions: Vec<Action>,
    pub inflight_work: bool,
    pub(super) inline_uploads: BTreeMap<u32, (u64, u32, bool)>,
    pub(super) mme: MacroUpload,
}
impl ExecutionState {
    pub fn stage(&self, methods: &[Method], access: &dyn Access) -> Result<Self, Error> {
        let mut staged = self.clone();
        let mut index = 0;
        while index < methods.len() {
            let command = methods[index];
            index += 1;
            let Method {
                subchannel,
                method,
                value,
                ..
            } = command;
            if subchannel > 7 {
                return Err(invalid("GPU subchannel bounds"));
            }
            let class = staged.bindings.get(&subchannel).copied();
            if method == 0 {
                if !matches!(value, 0xa140 | 0xb0b5 | 0x902d | 0xb197 | 0xb1c0)
                    || class.is_some_and(|c| c != value)
                {
                    return Err(staged.unsupported(command));
                }
                staged.bindings.insert(subchannel, value);
                if value == 0xb197 {
                    for (reg, initial) in
                        [(0xdac, 0x1b02), (0xdb0, 0x1b02), (0x20c0, 0), (0x2100, 0)]
                    {
                        staged.registers.entry((subchannel, reg)).or_insert(initial);
                    }
                }
                continue;
            }
            if class == Some(0xb197)
                && matches!(method, 0x3808 | 0x3810 | 0x3818 | 0x3820 | 0x3828 | 0x3830)
            {
                let mut parameters = Vec::new();
                if method == 0x3808 {
                    let continuation = methods
                        .get(index..index + 4)
                        .ok_or(invalid("vertex macro needs four parameters"))?;
                    if continuation
                        .iter()
                        .any(|m| m.subchannel != subchannel || m.method != 0x380c)
                    {
                        return Err(invalid("vertex macro continuation"));
                    }
                    parameters.extend(continuation.iter().map(|m| m.value));
                    index += 4;
                }
                let outputs = staged
                    .mme
                    .run((method - 0x3800) / 8, value, &parameters, |reg| {
                        staged.reg(subchannel, reg)
                    })?;
                if outputs
                    .iter()
                    .any(|&(reg, _)| (0x3800..=0x3ffc).contains(&reg))
                {
                    return Err(invalid("recursive MME call"));
                }
                let generated: Vec<_> = outputs
                    .into_iter()
                    .map(|(method, value)| Method {
                        method,
                        value,
                        ..command
                    })
                    .collect();
                staged = staged.stage(&generated, access)?;
            } else if staged.effect(command, access)? {
            } else if let Some(class) = class {
                if valid_register(class, method, value) {
                    staged.registers.insert((subchannel, method), value);
                } else {
                    return Err(staged.unsupported(command));
                }
            } else {
                return Err(staged.unsupported(command));
            }
        }
        if !staged.inline_uploads.is_empty() {
            return Err(invalid("incomplete compute inline upload"));
        }
        Ok(staged)
    }
    pub(super) fn reg(&self, sub: u32, reg: u32) -> Result<u32, Error> {
        self.registers
            .get(&(sub, reg))
            .copied()
            .ok_or(invalid("GPU state uninitialized"))
    }
    pub(super) fn unsupported(&self, command: Method) -> Error {
        Error::Message(format!(
            "unsupported GPU method {:#x} subchannel={} class={:?} value={:#x} at {:#x}",
            command.method,
            command.subchannel,
            self.bindings.get(&command.subchannel),
            command.value,
            command.address
        ))
    }
}
fn valid_register(class: u32, m: u32, v: u32) -> bool {
    if let Some(mask) = masks::mask(class, m) {
        return v & !mask == 0;
    }
    match class {
        0x902d => match m {
            0x260 | 0x290 | 0x29c | 0x888 => v <= 1,
            0x104 => v <= 0x1ffffff,
            0x108 => true,
            0x26c => v <= 4,
            0x2ac => v == 3,
            0x884 => v <= 0x3ff,
            _ => false,
        },
        0xb0b5 => match m {
            0x708 => v == 0x03303210,
            0x70c => v == 0x1040,
            0x400 | 0x408 => v <= 255,
            0x718 => v == 1,
            0x71c | 0x720 => v == 0,
            0x710 | 0x714 | 0x404 | 0x40c | 0x410 | 0x414 | 0x418 | 0x41c => true,
            _ => false,
        },
        0xa140 => match m {
            0x188 => v <= 0x1ffffff,
            0x180 | 0x184 | 0x18c | 0x190 => true,
            _ => false,
        },
        0xb197 => valid_3d(m, v),
        _ => false,
    }
}
fn valid_3d(m: u32, v: u32) -> bool {
    match m {
        0x1c04..=0x1cf8 if (m - 0x1c04) % 16 <= 4 && m.is_multiple_of(4) => {
            !(m - 0x1c04).is_multiple_of(16) || v <= 255
        }
        0x1f00..=0x1f7c if m.is_multiple_of(4) => !(m - 0x1f00).is_multiple_of(8) || v <= 255,
        0x1360..=0x137c if m.is_multiple_of(4) => v <= 1,
        0xdac | 0xdb0 => matches!(v, 0x1b00..=0x1b02),
        0x2000 | 0x2040 | 0x2080 | 0x20c0 | 0x2100 | 0x2140 => {
            v & !0xf1 == 0 && v >> 4 == (m - 0x2000) / 64
        }
        0x2004..=0x2150 if matches!((m - 0x2000) % 64, 4 | 12 | 16) => {
            v & !match (m - 0x2000) % 64 {
                4 => u32::MAX,
                12 => 255,
                _ => 7,
            } == 0
        }
        0x2ec => v & !0xff1 == 0,
        0x1234 | 0x15b4 => v == 0,
        0x2380 => v <= 0x1ffff,
        0x2384 => v <= 255,
        0x2388 => true,
        0x238c => v <= 0xffff,
        0x19e0..=0x19fc if m.is_multiple_of(4) => v <= 1,
        0x1604 => v & !0x1fff == 0 && v & 3 <= 2,
        0x165c => v <= 1,
        0x15d0 => matches!(v,0..=6|8..=11),
        0x12d4 => matches!(v, 1 | 2 | 0x1d00 | 0x1d01),
        0xd64 => matches!(v, 0..=8 | 15),
        0x308 => matches!(v, 1 | 3),
        0x218 | 0x10fc | 0x1290 | 0x12d8 | 0x12dc => matches!(v, 0 | 0x10 | 0x20),
        0x16a8 => v == 0x30003,
        0x1794 => v == 0x20002,
        0x1950 | 0x113c => v <= 2,
        0x193c => v == 2,
        0x810 => v == 0xd5,
        0x814 | 0xfec => v == 0x40,
        0xfe8 => v == 0x16,
        0x1340 | 0x134c => v == 0x8006,
        0x1344 => v == 0x4302,
        0x1348 | 0x1358 => v == 0x4303,
        0x1350 => v == 0x4001,
        0x191c => matches!(v, 0x900 | 0x901),
        0x1920 => matches!(v, 0x404 | 0x405 | 0x408),
        0x121c => v >> 28 == 0 && v & 15 <= 8,
        0x1700..=0x177c if m.is_multiple_of(4) => true,
        0x1558 | 0xde8 | 0x19cc | 0x194c | 0x197c | 0x19c0 | 0x192c => v <= 1,
        0xdcc => v <= 255,
        0xde4 => v <= 0x3f,
        0xdf8 => v <= 0x1ffff,
        0xdfc => v <= 0x3ffff,
        0xa00..=0xbf4 if matches!((m - 0xa00) % 32, 0 | 4 | 8 | 12 | 16 | 20) => true,
        0xc00..=0xcfc if m.is_multiple_of(4) => true,
        0xd00..=0xd3c if m.is_multiple_of(4) => true,
        0xe00..=0xef8 if matches!((m - 0xe00) % 16, 0 | 4 | 8) => {
            !(m - 0xe00).is_multiple_of(16) || v <= 1
        }
        _ => false,
    }
}
