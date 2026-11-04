//! B197 setup fields from the pinned clb197.h; shader/raster controls remain visible.
use super::*;

pub(super) fn retain(method: u32, value: u32) -> Result<bool, Error> {
    let valid = match method {
        0x2ec => value & !0xff1 == 0,
        0x110 | 0x1144 => value == 0,
        0x124 => matches!(value, 0 | 2),
        0x37c | 0xfac => value <= 1,
        0xdcc => (1..=32).contains(&value),
        0x1424 | 0x1428 | 0x1288 => value == 0,
        0xda4 => value & !0x1011 == 0,

        0x3400..=0x37fc if method.is_multiple_of(4) => true,
        0x135c => value == 0,
        0x12d4 => matches!(value, 1 | 2 | 0x1d00 | 0x1d01),
        0x308 => matches!(value, 1 | 3),
        0x10cc | 0x10e0 | 0x10e4 | 0x10ec | 0x10f0 => value & !0xff00ff == 0,
        0x2d0 => value <= 0x3fffff,
        0x7a0 => value <= 0x3ffffff,
        0xdf8 | 0xdfc | 0x197c => value == 0,

        0x19e0..=0x19fc if method.is_multiple_of(4) => value <= 1,
        0x1684 | 0x19cc | 0x151c | 0x20c | 0x15e4 | 0x1520 => value <= 1,
        0x1528 | 0x30c | 0xf8c | 0xfb4 | 0xfd8 | 0x1148 | 0x11f8 => value == 0,
        0x1000 => value & !0x31 == 0 && (value >> 4) < 3,
        0x218 | 0x10fc | 0x1290 | 0x12d8 | 0x12dc => matches!(value, 0 | 0x10 | 0x20),
        0xd64 => matches!(value, 0..=8 | 15),
        0x1610 => value & !0x3f == 0,
        0x164c => value & !0x1000 == 0,
        0x300 => value <= 3,
        0x310 => value <= 0xffff,
        0x77c | 0x794 | 0x79c | 0x1450 | 0x150 | 0x154 | 0xf88 => true,
        0x790 | 0xf84 => value <= 255,
        0x798 | 0x1590 => value <= 63,
        0x1130 => value & !0x13 == 0,
        0x374 | 0x1968 | 0x196c => value & !0x11 == 0,
        0x15cc => value & !0x1ffff == 0,
        0x1220 => value <= 12,
        0x13ac => value == 0,
        0x1604 => value & !0x1fff == 0 && value & 3 <= 2,
        0x16b4 => value <= 3,
        0x1134 => value <= 1,
        0x1a2c => true,
        _ => return Ok(false),
    };
    if !valid {
        return Err(Error::Message(format!(
            "Metal setup field {method:#x}={value:#x}"
        )));
    }
    Ok(true)
}

pub(super) fn compute_field(method: u32, value: u32) -> Result<bool, Error> {
    let mask = match method {
        0x2608 => 7,
        0x790 | 0x2e4 | 0x2f0 | 0x1608 | 0x1574 | 0x155c => 0xff,
        0x794 | 0x2e8 | 0x2f4 | 0x77c | 0x214 | 0x160c | 0x1578 | 0x1560 => u32::MAX,
        0x2ec | 0x2f8 => 0x1ff,
        0x310 => 0xffff,
        0x157c => 0x3fffff,
        0x1564 => 0xfffff,
        0x1698 => 0x1011,
        0x110 => 0,
        _ => return Ok(false),
    };
    if value & !mask != 0 {
        return Err(Error::Malformed("Metal compute setup reserved bits"));
    }
    Ok(true)
}
