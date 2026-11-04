//! Ordered method writes. Offsets use NVIDIA byte units; FIFO decoding is caller-owned.
use crate::{memory::Memory, registers, require, Error, Result};
use std::collections::BTreeMap;
pub const B197: u32 = 0xb197;
pub const A140: u32 = 0xa140;
pub const TWOD: u32 = 0x902d;
#[derive(Clone, Debug, Default)]
pub struct Registers(pub BTreeMap<u32, u32>);
impl Registers {
    pub fn get(&self, offset: u32) -> u32 {
        self.0.get(&offset).copied().unwrap_or(0)
    }
    pub fn address(&self, offset: u32) -> u64 {
        (u64::from(self.get(offset)) << 32) | u64::from(self.get(offset + 4))
    }
    pub fn float(&self, offset: u32) -> f32 {
        f32::from_bits(self.get(offset))
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConstantBuffer {
    pub address: u64,
    pub size: u32,
}
#[derive(Clone, Debug)]
pub enum Action {
    Clear(u32),
    Draw {
        topology: u32,
        first: u32,
        count: u32,
        indexed: bool,
        instance: u32,
    },
}
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub registers: Registers,
    pub constants: [[Option<ConstantBuffer>; 32]; 5],
    pub action: Action,
}
#[derive(Default)]
pub struct Engine {
    pub registers: Registers,
    constants: [[Option<ConstantBuffer>; 32]; 5],
    begin: Option<u32>,
    instance: u32,
    inline: Registers,
    copy: Registers,
    upload: Option<(u64, usize, usize)>,
}
impl Engine {
    /// Returns an action at the exact write that launches work. The caller must resolve
    /// its resources before applying later writes to guest memory.
    pub fn method(
        &mut self,
        class: u32,
        offset: u32,
        value: u32,
        memory: &mut Memory,
    ) -> Result<Option<Snapshot>> {
        require(
            registers::lookup(class, offset).is_some(),
            &format!("unknown method {class:04x}:{offset:04x}"),
        )?;
        if class == A140 {
            self.inline(offset, value, memory)?;
            return Ok(None);
        }
        if class == TWOD {
            self.copy(offset, value, memory)?;
            return Ok(None);
        }
        require(class == B197, "unsupported engine class")?;
        let action = match offset {
            0x1618 => {
                require(self.begin.is_none(), "nested BEGIN")?;
                require(value & !0x0c00ffff == 0, "split primitive mode unsupported")?;
                require(
                    matches!(value & 0xffff, 0 | 1 | 3 | 4 | 5),
                    "unsupported primitive topology",
                )?;
                self.instance = match (value >> 26) & 3 {
                    0 => 0,
                    1 => self
                        .instance
                        .checked_add(1)
                        .ok_or_else(|| Error("instance overflow".into()))?,
                    2 => self.instance,
                    _ => return Err(Error("reserved instance mode".into())),
                };
                self.begin = Some(value & 0xffff);
                None
            }
            0x1614 => {
                require(
                    value == 0 && self.begin.take().is_some(),
                    "END without BEGIN or nonzero END",
                )?;
                None
            }
            0x0d78 | 0x17e0 => {
                let topology = self
                    .begin
                    .ok_or_else(|| Error("draw without BEGIN".into()))?;
                Some(Action::Draw {
                    topology,
                    first: self
                        .registers
                        .get(if offset == 0x17e0 { 0x17dc } else { 0x0d74 }),
                    count: value,
                    indexed: offset == 0x17e0,
                    instance: self.instance,
                })
            }
            0x19d0 => Some(Action::Clear(value)),
            0x2410 | 0x2430 | 0x2450 | 0x2470 | 0x2490 => {
                require(value & !0x1f1 == 0, "reserved constant binding bits")?;
                let cb = ConstantBuffer {
                    address: self.registers.address(0x2384),
                    size: self.registers.get(0x2380),
                };
                require(cb.size <= 65536, "constant buffer exceeds 64 KiB")?;
                self.constants[((offset - 0x2410) / 32) as usize][((value >> 4) & 31) as usize] =
                    if value & 1 != 0 { Some(cb) } else { None };
                None
            }
            0x2390..=0x23cc => {
                let pos = self.registers.get(0x238c);
                require(
                    pos.checked_add(4)
                        .is_some_and(|v| v <= self.registers.get(0x2380)),
                    "constant upload exceeds selector",
                )?;
                let addr = self
                    .registers
                    .address(0x2384)
                    .checked_add(u64::from(pos))
                    .ok_or_else(|| Error("constant address overflow".into()))?;
                memory.write(addr, &value.to_le_bytes())?;
                self.registers.0.insert(0x238c, pos + 4);
                None
            }
            _ => None,
        };
        self.registers.0.insert(offset, value);
        Ok(action.map(|action| Snapshot {
            registers: self.registers.clone(),
            constants: self.constants,
            action,
        }))
    }
    fn inline(&mut self, offset: u32, value: u32, memory: &mut Memory) -> Result<()> {
        match offset {
            0x180..=0x190 => {
                require(
                    self.upload.is_none(),
                    "upload configuration during transfer",
                )?;
                self.inline.0.insert(offset, value);
            }
            0x1b0 => {
                require(value == 1, "inline upload supports pitch/no semaphore only")?;
                require(self.upload.is_none(), "unfinished inline upload")?;
                let width = self.inline.get(0x180) as usize;
                let height = self.inline.get(0x184) as usize;
                require(
                    width > 0 && height == 1,
                    "inline upload supports one nonempty line",
                )?;
                let address = self.inline.address(0x188);
                memory.read(address, width)?;
                self.upload = Some((address, width, 0));
            }
            0x1b4 => {
                let (address, size, pos) = self
                    .upload
                    .ok_or_else(|| Error("inline data without launch".into()))?;
                let n = (size - pos).min(4);
                memory.write(address + pos as u64, &value.to_le_bytes()[..n])?;
                self.upload = if pos + n == size {
                    None
                } else {
                    Some((address, size, pos + n))
                };
            }
            _ => return Err(Error(format!("unsupported A140 method {offset:#x}"))),
        }
        Ok(())
    }
    fn copy(&mut self, offset: u32, value: u32, memory: &mut Memory) -> Result<()> {
        // Pitch RGBA8 SRCCOPY, integer source origin and unity derivatives.
        require(
            matches!(offset,0x200..=0x224|0x230..=0x254|0x2ac|0x8b0..=0x8dc),
            "unsupported 902D method",
        )?;
        self.copy.0.insert(offset, value);
        if offset != 0x8dc {
            return Ok(());
        }
        let r = &self.copy;
        require(
            r.get(0x200) == 0xd5
                && r.get(0x230) == 0xd5
                && r.get(0x204) == 1
                && r.get(0x234) == 1
                && r.get(0x2ac) == 3,
            "902D requires pitch RGBA8 SRCCOPY",
        )?;
        require(
            r.get(0x8c0) == 0
                && r.get(0x8c4) == 1
                && r.get(0x8c8) == 0
                && r.get(0x8cc) == 1
                && r.get(0x8d0) == 0
                && r.get(0x8d8) == 0,
            "902D scaling/fractional origins unsupported",
        )?;
        require(
            r.get(0x208) == 0
                && r.get(0x238) == 0
                && r.get(0x20c) == 1
                && r.get(0x23c) == 1
                && r.get(0x210) == 0,
            "902D requires nonlayered 2D surfaces",
        )?;
        let (dx, dy, w, h, sx, sy) = (
            r.get(0x8b0),
            r.get(0x8b4),
            r.get(0x8b8),
            r.get(0x8bc),
            r.get(0x8d4),
            value,
        );
        require(
            w > 0
                && h > 0
                && dx.checked_add(w).is_some_and(|v| v <= r.get(0x218))
                && dy.checked_add(h).is_some_and(|v| v <= r.get(0x21c))
                && sx.checked_add(w).is_some_and(|v| v <= r.get(0x248))
                && sy.checked_add(h).is_some_and(|v| v <= r.get(0x24c)),
            "902D rectangle bounds",
        )?;
        require(
            u64::from(r.get(0x214)) >= u64::from(r.get(0x218)) * 4
                && u64::from(r.get(0x244)) >= u64::from(r.get(0x248)) * 4,
            "902D pitch too small",
        )?;
        let mut rows = Vec::new();
        for y in 0..h {
            let src = r
                .address(0x250)
                .checked_add(u64::from(sy + y) * u64::from(r.get(0x244)) + u64::from(sx) * 4)
                .ok_or_else(|| Error("copy source overflow".into()))?;
            let dst = r
                .address(0x220)
                .checked_add(u64::from(dy + y) * u64::from(r.get(0x214)) + u64::from(dx) * 4)
                .ok_or_else(|| Error("copy dest overflow".into()))?;
            memory.read(dst, w as usize * 4)?;
            rows.push((dst, memory.read(src, w as usize * 4)?.to_vec()));
        }
        for (dst, data) in rows {
            memory.write(dst, &data)?;
        }
        Ok(())
    }
}
