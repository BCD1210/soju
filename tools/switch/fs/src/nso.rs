//! NSO executables (the `main`, `rtld`, `sdk`, `subsdk*` files in an ExeFS).
//!
//! Header (0x100): "NSO0", version, reserved, flags (bits 0-2: text/ro/data compressed,
//! bits 3-5: check hash), then for text/ro/data: file offset, memory offset, size,
//! the BSS size at 0x3C, module id at 0x40, compressed sizes at 0x60/0x64/0x68 and
//! SHA-256 of each decompressed segment at 0xA0/0xC0/0xE0.

use crate::{lz4, u32le, Error, Result};
use sha2::{Digest, Sha256};

pub const MAX_SEGMENT: u32 = 256 << 20;

#[derive(Debug, Clone)]
pub struct Segment {
    pub memory_offset: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct Nso {
    pub text: Segment,
    pub ro: Segment,
    pub data: Segment,
    pub bss_size: u32,
    pub module_id: [u8; 32],
}

impl Nso {
    /// Total memory image size (segments + BSS), page aligned.
    pub fn image_size(&self) -> u64 {
        let end = (self.data.memory_offset as u64 + self.data.data.len() as u64 + self.bss_size as u64)
            .max(self.ro.memory_offset as u64 + self.ro.data.len() as u64)
            .max(self.text.memory_offset as u64 + self.text.data.len() as u64);
        (end + 0xFFF) & !0xFFF
    }
}

pub fn parse(file: &[u8]) -> Result<Nso> {
    let h = file.get(..0x100).ok_or(Error::Truncated)?;
    if &h[..4] != b"NSO0" {
        return Err(Error::BadMagic("NSO"));
    }
    let flags = u32le(h, 0xC)?;
    let mut segs = Vec::new();
    for i in 0..3 {
        let base = 0x10 + i * 0x10;
        let file_off = u32le(h, base)? as usize;
        let mem_off = u32le(h, base + 4)?;
        let size = u32le(h, base + 8)?;
        let csize = u32le(h, 0x60 + i * 4)? as usize;
        if size > MAX_SEGMENT || csize as u32 > MAX_SEGMENT {
            return Err(Error::Format("NSO segment is too large"));
        }
        let compressed = flags & (1 << i) != 0;
        let stored = if compressed { csize } else { size as usize };
        let raw = file.get(file_off..file_off.checked_add(stored).ok_or(Error::Truncated)?).ok_or(Error::Truncated)?;
        let data = if compressed { lz4::decompress(raw, size as usize)? } else { raw.to_vec() };
        if flags & (1 << (i + 3)) != 0 {
            let want = &h[0xA0 + i * 0x20..0xC0 + i * 0x20];
            if Sha256::digest(&data).as_slice() != want {
                return Err(Error::Format("NSO segment hash does not match"));
            }
        }
        segs.push(Segment { memory_offset: mem_off, data });
    }
    let mut it = segs.into_iter();
    let (text, ro, data) = (it.next().unwrap(), it.next().unwrap(), it.next().unwrap());
    if text.memory_offset != 0 || ro.memory_offset < text.data.len() as u32 || data.memory_offset < ro.memory_offset + ro.data.len() as u32 {
        return Err(Error::Format("NSO segments overlap or are out of order"));
    }
    Ok(Nso { text, ro, data, bss_size: u32le(h, 0x3C)?, module_id: h[0x40..0x60].try_into().unwrap() })
}

/// Build an NSO for tests: segments compressed with `lz4::compress`, hashes checked.
pub fn build(text: &[u8], ro: &[u8], data: &[u8], bss: u32) -> Vec<u8> {
    let align = |n: usize| ((n + 0xFFF) & !0xFFF) as u32;
    let mem = [0u32, align(text.len()), align(text.len()) + align(ro.len())];
    let segs = [text, ro, data];
    let mut h = vec![0u8; 0x100];
    h[..4].copy_from_slice(b"NSO0");
    h[0xC..0x10].copy_from_slice(&0x3Fu32.to_le_bytes());
    let mut body = Vec::new();
    for (i, s) in segs.iter().enumerate() {
        let c = lz4::compress(s);
        let base = 0x10 + i * 0x10;
        h[base..base + 4].copy_from_slice(&(0x100 + body.len() as u32).to_le_bytes());
        h[base + 4..base + 8].copy_from_slice(&mem[i].to_le_bytes());
        h[base + 8..base + 12].copy_from_slice(&(s.len() as u32).to_le_bytes());
        h[0x60 + i * 4..0x64 + i * 4].copy_from_slice(&(c.len() as u32).to_le_bytes());
        h[0xA0 + i * 0x20..0xC0 + i * 0x20].copy_from_slice(&Sha256::digest(s));
        body.extend_from_slice(&c);
    }
    h[0x3C..0x40].copy_from_slice(&bss.to_le_bytes());
    h[0x40] = 0xAB;
    h.extend_from_slice(&body);
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let text: Vec<u8> = (0..0x3000u32).map(|i| (i % 7) as u8).collect();
        let ro = b"read only strings".repeat(50);
        let data = vec![0x11u8; 0x500];
        let img = build(&text, &ro, &data, 0x2000);
        let n = parse(&img).unwrap();
        assert_eq!(n.text.data, text);
        assert_eq!(n.ro.data, ro);
        assert_eq!(n.data.data, data);
        assert_eq!(n.ro.memory_offset, 0x3000);
        assert_eq!(n.module_id[0], 0xAB);
        assert_eq!(n.image_size(), 0x3000 + 0x1000 + 0x1000 + 0x2000);
    }

    #[test]
    fn hash_and_magic_checks() {
        let mut img = build(b"text text text text", b"ro", b"data", 0);
        img[0xA0] ^= 1;
        assert!(parse(&img).unwrap_err().to_string().contains("hash"));
        img[0] = b'X';
        assert!(matches!(parse(&img), Err(Error::BadMagic(_))));
    }
}
