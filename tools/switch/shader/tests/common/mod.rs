#![allow(dead_code)]
use std::collections::BTreeMap;

pub const PT: u64 = 7 << 16;
pub const EXIT: u64 = 0xe30000000000000f | PT;
pub const NOP: u64 = 0x50b0000000000f00 | PT;

/// Original assembler for fixtures. All offsets include the scheduling words.
#[derive(Default)]
pub struct Assembler {
    words: Vec<u64>,
    labels: BTreeMap<&'static str, u32>,
    fixups: Vec<(usize, &'static str)>,
}
impl Assembler {
    pub fn address(&self) -> u32 {
        ((self.words.len() + self.words.len() / 3 + 1) * 8) as u32
    }
    pub fn label(&mut self, name: &'static str) {
        assert!(self.labels.insert(name, self.address()).is_none());
    }
    pub fn emit(&mut self, word: u64) {
        self.words.push(word);
    }
    pub fn branch(&mut self, word: u64, label: &'static str) {
        self.fixups.push((self.words.len(), label));
        self.emit(word);
    }
    pub fn finish(mut self, header: [u32; 20]) -> Vec<u8> {
        for (i, label) in self.fixups {
            let address = ((i + i / 3 + 1) * 8) as i64;
            let delta = i64::from(self.labels[label]) - address - 8;
            assert!((-0x800000..0x800000).contains(&delta));
            self.words[i] |= ((delta as u64) & 0xffffff) << 20;
        }
        let mut out: Vec<_> = header.into_iter().flat_map(u32::to_le_bytes).collect();
        for chunk in self.words.chunks(3) {
            // No dependency barriers (7), no stalls, no reuse hints.
            let schedule = 0x7e0u64 | (0x7e0 << 21) | (0x7e0 << 42);
            out.extend(schedule.to_le_bytes());
            for w in chunk {
                out.extend(w.to_le_bytes());
            }
        }
        out
    }
}
pub fn vertex_header() -> [u32; 20] {
    let mut h = [0; 20];
    h[0] = 1 | (1 << 10);
    h[6] = 15;
    h[13] = 15 << 12;
    h
}
pub fn fragment_header() -> [u32; 20] {
    let mut h = [0; 20];
    h[0] = 2 | (5 << 10);
    h[18] = 15;
    h
}
pub fn mov32(dst: u8, value: u32) -> u64 {
    0x010000000000f000 | PT | u64::from(dst) | (u64::from(value) << 20)
}
pub fn alu(op: u64, dst: u8, a: u8, b: u8) -> u64 {
    op | PT | u64::from(dst) | (u64::from(a) << 8) | (u64::from(b) << 20)
}
pub fn attr(store: bool, address: u16, first: u8, count: u64) -> u64 {
    (if store {
        0xeff0000000000000
    } else {
        0xefd8000000000000
    }) | PT
        | u64::from(first)
        | 255 << 8
        | u64::from(address) << 20
        | 255 << 39
        | (count - 1) << 47
}
pub fn ipa(dst: u8, address: u16) -> u64 {
    0xe00000000000ff00
        | PT
        | u64::from(dst)
        | 255 << 20
        | u64::from(address) << 28
        | 255 << 39
        | 7 << 47
}
