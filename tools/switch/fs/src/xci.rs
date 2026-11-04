//! XCI game card images: "HEAD" at 0x100 (or 0x1100 when a 0x1000-byte key area
//! precedes the header), root HFS0 offset u64 at +0x30 and size at +0x38 from the
//! header base. The root HFS0 holds the "update", "normal", "secure" (and "logo") partitions.

use crate::hfs0::{self, HashedEntry};
use crate::pfs0::{Storage, Sub};
use crate::{u64le, Error, Result};

pub struct Xci<'a> {
    pub storage: &'a dyn Storage,
    /// 0 or 0x1000 when the dump starts with the key area.
    pub base: u64,
    pub root_offset: u64,
    pub partitions: Vec<HashedEntry>,
}

impl<'a> Xci<'a> {
    pub fn open(s: &'a dyn Storage) -> Result<Self> {
        let mut base = None;
        for b in [0u64, 0x1000] {
            if s.len() >= b + 0x200 && s.read_vec(b + 0x100, 4)? == b"HEAD" {
                base = Some(b);
                break;
            }
        }
        let base = base.ok_or(Error::BadMagic("XCI"))?;
        let head = s.read_vec(base + 0x100, 0x100)?;
        let root_offset = base + u64le(&head, 0x30)?;
        let root_size = u64le(&head, 0x38)?;
        if root_offset.checked_add(root_size).ok_or(Error::Truncated)? > s.len() {
            return Err(Error::Truncated);
        }
        let root = Sub { inner: s, offset: root_offset, size: s.len() - root_offset };
        let mut partitions = hfs0::parse(&root)?;
        for p in &mut partitions {
            p.entry.offset += root_offset;
        }
        Ok(Xci { storage: s, base, root_offset, partitions })
    }

    /// Files of a partition (offsets absolute in the XCI).
    pub fn partition(&self, name: &str) -> Result<Vec<HashedEntry>> {
        let p = self.partitions.iter().find(|p| p.entry.name == name).ok_or(Error::Format("partition not found"))?;
        let sub = Sub { inner: self.storage, offset: p.entry.offset, size: p.entry.size };
        let mut files = hfs0::parse(&sub)?;
        for f in &mut files {
            f.entry.offset += p.entry.offset;
        }
        Ok(files)
    }
}

/// Build a minimal XCI image for tests: header + root HFS0 with the given secure files.
pub fn build(secure_files: &[(&str, &[u8])], with_key_area: bool) -> Vec<u8> {
    let secure = hfs0::build(secure_files);
    let root = hfs0::build(&[("update", &[]), ("normal", &[]), ("secure", &secure)]);
    let base = if with_key_area { 0x1000 } else { 0 };
    let root_at = 0xF000u64;
    let mut out = vec![0u8; base + root_at as usize];
    out[base + 0x100..base + 0x104].copy_from_slice(b"HEAD");
    out[base + 0x130..base + 0x138].copy_from_slice(&root_at.to_le_bytes());
    out[base + 0x138..base + 0x140].copy_from_slice(&(0x200u64).to_le_bytes());
    out.extend_from_slice(&root);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_secure_partition_with_and_without_key_area() {
        for key_area in [false, true] {
            let img = build(&[("abcd.nca", b"nca bytes")], key_area);
            let x = Xci::open(&img).unwrap();
            assert_eq!(x.base, if key_area { 0x1000 } else { 0 });
            let names: Vec<_> = x.partitions.iter().map(|p| p.entry.name.as_str()).collect();
            assert_eq!(names, ["update", "normal", "secure"]);
            let files = x.partition("secure").unwrap();
            assert_eq!(files[0].entry.name, "abcd.nca");
            assert_eq!(img.read_vec(files[0].entry.offset, 9).unwrap(), b"nca bytes");
        }
    }

    #[test]
    fn rejects_non_xci() {
        assert!(matches!(Xci::open(&vec![0u8; 0x2000]), Err(Error::BadMagic(_))));
    }
}
