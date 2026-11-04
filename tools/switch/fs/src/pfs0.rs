//! PFS0 (PartitionFs): the container of an NSP file and of NCA ExeFS sections.
//!
//! Layout: "PFS0", file count u32, string table size u32, reserved u32,
//! then 0x18-byte entries (data offset u64, size u64, name offset u32, reserved u32),
//! the string table, and file data relative to the end of the header.

use crate::{cstr, u32le, u64le, Error, Result};

/// Random-access byte source (file, memory, or a decrypted NCA section).
pub trait Storage {
    fn len(&self) -> u64;
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()>;

    fn read_vec(&self, offset: u64, len: usize) -> Result<Vec<u8>> {
        let end = offset.checked_add(len as u64).ok_or(Error::Truncated)?;
        if end > self.len() {
            return Err(Error::Truncated);
        }
        let mut v = vec![0u8; len];
        self.read_at(offset, &mut v)?;
        Ok(v)
    }
}

impl Storage for Vec<u8> {
    fn len(&self) -> u64 {
        self.as_slice().len() as u64
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let start = usize::try_from(offset).map_err(|_| Error::Truncated)?;
        let src = self.get(start..start + buf.len()).ok_or(Error::Truncated)?;
        buf.copy_from_slice(src);
        Ok(())
    }
}

impl Storage for std::fs::File {
    fn len(&self) -> u64 {
        self.metadata().map(|m| m.len()).unwrap_or(0)
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        use std::os::unix::fs::FileExt;
        self.read_exact_at(buf, offset).map_err(|e| {
            if e.kind() == std::io::ErrorKind::UnexpectedEof { Error::Truncated } else { Error::Io(e) }
        })
    }
}

/// A window into another storage.
pub struct Sub<'a> {
    pub inner: &'a dyn Storage,
    pub offset: u64,
    pub size: u64,
}

impl Storage for Sub<'_> {
    fn len(&self) -> u64 {
        self.size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let end = offset.checked_add(buf.len() as u64).ok_or(Error::Truncated)?;
        if end > self.size {
            return Err(Error::Truncated);
        }
        self.inner.read_at(self.offset + offset, buf)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    /// Absolute offset inside the storage the PFS0 was read from.
    pub offset: u64,
    pub size: u64,
}

pub const MAX_FILES: u32 = 4096;
pub const MAX_STRINGS: u32 = 1 << 20;

pub fn parse(s: &dyn Storage) -> Result<Vec<Entry>> {
    let head = s.read_vec(0, 0x10)?;
    if &head[..4] != b"PFS0" {
        return Err(Error::BadMagic("PFS0"));
    }
    let count = u32le(&head, 4)?;
    let strings = u32le(&head, 8)?;
    if count > MAX_FILES || strings > MAX_STRINGS {
        return Err(Error::Format("PFS0 header is too large"));
    }
    let table_len = 0x10 + count as usize * 0x18 + strings as usize;
    let table = s.read_vec(0, table_len)?;
    let names = &table[0x10 + count as usize * 0x18..];
    let data_start = table_len as u64;
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count as usize {
        let e = 0x10 + i * 0x18;
        let offset = data_start.checked_add(u64le(&table, e)?).ok_or(Error::Truncated)?;
        let size = u64le(&table, e + 8)?;
        if offset.checked_add(size).ok_or(Error::Truncated)? > s.len() {
            return Err(Error::Truncated);
        }
        out.push(Entry { name: cstr(names, u32le(&table, e + 0x10)? as usize)?, offset, size });
    }
    Ok(out)
}

/// Build a PFS0 image (used for synthetic test fixtures).
pub fn build(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut strings = Vec::new();
    let mut offsets = Vec::new();
    for (name, _) in files {
        offsets.push(strings.len() as u32);
        strings.extend_from_slice(name.as_bytes());
        strings.push(0);
    }
    while (0x10 + files.len() * 0x18 + strings.len()) % 0x20 != 0 {
        strings.push(0);
    }
    let mut out = Vec::new();
    out.extend_from_slice(b"PFS0");
    out.extend_from_slice(&(files.len() as u32).to_le_bytes());
    out.extend_from_slice(&(strings.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    let mut data_off = 0u64;
    for (i, (_, data)) in files.iter().enumerate() {
        out.extend_from_slice(&data_off.to_le_bytes());
        out.extend_from_slice(&(data.len() as u64).to_le_bytes());
        out.extend_from_slice(&offsets[i].to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        data_off += data.len() as u64;
    }
    out.extend_from_slice(&strings);
    for (_, data) in files {
        out.extend_from_slice(data);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let img = build(&[("main", b"code"), ("main.npdm", b"meta!")]);
        let entries = parse(&img).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[1].name, "main.npdm");
        assert_eq!(img.read_vec(entries[1].offset, entries[1].size as usize).unwrap(), b"meta!");
    }

    #[test]
    fn rejects_bad_magic_truncation_and_unsafe_names() {
        let mut img = build(&[("a", b"x")]);
        assert!(matches!(parse(&img[..0x18].to_vec()), Err(Error::Truncated)));
        let bad = build(&[("../evil", b"x")]);
        assert!(parse(&bad).is_err());
        img[0] = b'X';
        assert!(matches!(parse(&img), Err(Error::BadMagic(_))));
        let mut huge = build(&[("a", b"x")]);
        huge[4..8].copy_from_slice(&(MAX_FILES + 1).to_le_bytes());
        assert!(parse(&huge).is_err());
    }
}
