//! Update patches (BKTR). An update's Program NCA has a RomFS section whose data is a
//! *patch*: an "indirect" table says, for each range of the new RomFS, whether the bytes
//! come from the base game's RomFS or from the update itself, and an "AES-CTR-EX" table
//! gives the counter generation used for each range of the update's own data.
//!
//! Both tables are bucket trees: a 0x10-byte header ("BKTR", version, entry count), then
//! 0x4000-byte nodes. Node 0 lists the start offsets of the entry sets; each entry set
//! node starts with (index u32, count u32, end offset u64) followed by fixed-size entries.
//! Indirect entries are 0x14 bytes (virtual offset u64, physical offset u64, storage u32);
//! AES-CTR-EX entries are 0x10 bytes (offset u64, reserved u32, generation u32).
//!
//! Written from public format descriptions; verified here only with synthetic files.

use crate::pfs0::Storage;
use crate::{u32le, u64le, Error, Result};

pub const NODE: usize = 0x4000;
pub const MAX_ENTRIES: u32 = 1 << 22;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndirectEntry {
    pub virtual_offset: u64,
    pub physical_offset: u64,
    /// 0 = base game RomFS, 1 = the update's own data.
    pub storage: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CtrExEntry {
    pub offset: u64,
    pub generation: u32,
}

/// Read a bucket tree's entries (`entry_size` bytes each) from `table`.
fn bucket_entries(table: &[u8], header: &[u8], entry_size: usize) -> Result<(Vec<Vec<u8>>, u64)> {
    if header.len() < 0x10 || &header[..4] != b"BKTR" {
        return Err(Error::BadMagic("BKTR"));
    }
    let count = u32le(header, 8)?;
    if count > MAX_ENTRIES {
        return Err(Error::Format("patch table is too large"));
    }
    let l1 = table.get(..NODE).ok_or(Error::Truncated)?;
    let sets = u32le(l1, 4)? as usize;
    let end = u64le(l1, 8)?;
    let per_set = (NODE - 0x10) / entry_size;
    if sets == 0 || sets > (NODE - 0x10) / 8 || sets * per_set < count as usize {
        return Err(Error::Format("patch table node count does not fit its entries"));
    }
    let mut out = Vec::with_capacity(count as usize);
    for set in 0..sets {
        let node = table.get(NODE * (1 + set)..NODE * (2 + set)).ok_or(Error::Truncated)?;
        let n = u32le(node, 4)? as usize;
        if n > per_set {
            return Err(Error::Format("patch table entry set overflows its node"));
        }
        for i in 0..n {
            let e = 0x10 + i * entry_size;
            out.push(node[e..e + entry_size].to_vec());
        }
    }
    if out.len() != count as usize {
        return Err(Error::Format("patch table entry count mismatch"));
    }
    Ok((out, end))
}

pub fn indirect_table(table: &[u8], header: &[u8]) -> Result<(Vec<IndirectEntry>, u64)> {
    let (raw, end) = bucket_entries(table, header, 0x14)?;
    let mut v: Vec<IndirectEntry> = raw
        .iter()
        .map(|e| Ok(IndirectEntry { virtual_offset: u64le(e, 0)?, physical_offset: u64le(e, 8)?, storage: u32le(e, 16)? }))
        .collect::<Result<_>>()?;
    if v.windows(2).any(|w| w[0].virtual_offset >= w[1].virtual_offset) || v.first().is_some_and(|f| f.virtual_offset != 0) {
        return Err(Error::Format("patch ranges are out of order"));
    }
    if v.iter().any(|e| e.storage > 1) {
        return Err(Error::Format("unknown patch storage index"));
    }
    v.shrink_to_fit();
    Ok((v, end))
}

pub fn ctr_ex_table(table: &[u8], header: &[u8]) -> Result<(Vec<CtrExEntry>, u64)> {
    let (raw, end) = bucket_entries(table, header, 0x10)?;
    let v: Vec<CtrExEntry> = raw.iter().map(|e| Ok(CtrExEntry { offset: u64le(e, 0)?, generation: u32le(e, 12)? })).collect::<Result<_>>()?;
    if v.windows(2).any(|w| w[0].offset >= w[1].offset) {
        return Err(Error::Format("counter ranges are out of order"));
    }
    Ok((v, end))
}

/// The patched RomFS: reads go to the base RomFS or to the update's decrypted data.
pub struct Patched<'a> {
    pub base: &'a dyn Storage,
    pub patch: &'a dyn Storage,
    pub entries: Vec<IndirectEntry>,
    pub size: u64,
}

impl Storage for Patched<'_> {
    fn len(&self) -> u64 {
        self.size
    }

    fn read_at(&self, mut offset: u64, mut buf: &mut [u8]) -> Result<()> {
        if offset.checked_add(buf.len() as u64).ok_or(Error::Truncated)? > self.size {
            return Err(Error::Truncated);
        }
        while !buf.is_empty() {
            let i = self.entries.partition_point(|e| e.virtual_offset <= offset) - 1;
            let e = self.entries[i];
            let range_end = self.entries.get(i + 1).map_or(self.size, |n| n.virtual_offset);
            let n = ((range_end - offset) as usize).min(buf.len());
            let phys = e.physical_offset + (offset - e.virtual_offset);
            let (head, rest) = buf.split_at_mut(n);
            if e.storage == 0 { self.base.read_at(phys, head)? } else { self.patch.read_at(phys, head)? }
            buf = rest;
            offset += n as u64;
        }
        Ok(())
    }
}

/// Build a bucket tree table + header for tests.
pub fn build_table(entries: &[Vec<u8>], end: u64) -> (Vec<u8>, [u8; 0x10]) {
    let entry_size = entries.first().map_or(0x10, |e| e.len());
    let per_set = (NODE - 0x10) / entry_size;
    let sets: Vec<&[Vec<u8>]> = entries.chunks(per_set.max(1)).collect();
    let sets_n = sets.len().max(1);
    let mut t = vec![0u8; NODE * (1 + sets_n)];
    t[4..8].copy_from_slice(&(sets_n as u32).to_le_bytes());
    t[8..16].copy_from_slice(&end.to_le_bytes());
    for (s, chunk) in sets.iter().enumerate() {
        let node = NODE * (1 + s);
        t[node..node + 4].copy_from_slice(&(s as u32).to_le_bytes());
        t[node + 4..node + 8].copy_from_slice(&(chunk.len() as u32).to_le_bytes());
        for (i, e) in chunk.iter().enumerate() {
            let at = node + 0x10 + i * entry_size;
            t[at..at + entry_size].copy_from_slice(e);
        }
    }
    let mut h = [0u8; 0x10];
    h[..4].copy_from_slice(b"BKTR");
    h[4..8].copy_from_slice(&1u32.to_le_bytes());
    h[8..12].copy_from_slice(&(entries.len() as u32).to_le_bytes());
    (t, h)
}

pub fn indirect_bytes(e: IndirectEntry) -> Vec<u8> {
    let mut b = Vec::with_capacity(0x14);
    b.extend_from_slice(&e.virtual_offset.to_le_bytes());
    b.extend_from_slice(&e.physical_offset.to_le_bytes());
    b.extend_from_slice(&e.storage.to_le_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    fn patched_fixture() -> (Vec<u8>, Vec<u8>, Vec<IndirectEntry>, u64) {
        let base: Vec<u8> = (0..4096u32).map(|i| (i % 251) as u8).collect();
        let patch: Vec<u8> = vec![0xEE; 1024];
        let entries = vec![
            IndirectEntry { virtual_offset: 0, physical_offset: 0, storage: 0 },
            IndirectEntry { virtual_offset: 1000, physical_offset: 100, storage: 1 },
            IndirectEntry { virtual_offset: 1500, physical_offset: 1500, storage: 0 },
            IndirectEntry { virtual_offset: 3000, physical_offset: 600, storage: 1 },
        ];
        (base, patch, entries, 3200)
    }

    #[test]
    fn reads_across_base_and_patch_ranges() {
        let (base, patch, entries, size) = patched_fixture();
        let p = Patched { base: &base, patch: &patch, entries, size };
        let all = p.read_vec(0, size as usize).unwrap();
        assert_eq!(&all[..1000], &base[..1000]);
        assert!(all[1000..1500].iter().all(|&b| b == 0xEE));
        assert_eq!(&all[1500..3000], &base[1500..3000]);
        assert!(all[3000..].iter().all(|&b| b == 0xEE));
        // A read straddling a boundary.
        assert_eq!(p.read_vec(998, 4).unwrap(), [base[998], base[999], 0xEE, 0xEE]);
        assert!(p.read_vec(3199, 2).is_err());
    }

    #[test]
    fn bucket_tree_round_trip_with_many_sets() {
        let entries: Vec<IndirectEntry> = (0..2000u64).map(|i| IndirectEntry { virtual_offset: i * 0x100, physical_offset: i * 7, storage: (i % 2) as u32 }).collect();
        let raw: Vec<Vec<u8>> = entries.iter().map(|e| indirect_bytes(*e)).collect();
        let (table, header) = build_table(&raw, 2000 * 0x100);
        let (back, end) = indirect_table(&table, &header).unwrap();
        assert_eq!(back, entries);
        assert_eq!(end, 2000 * 0x100);
    }

    #[test]
    fn ctr_ex_entries() {
        let raw: Vec<Vec<u8>> = [(0u64, 3u32), (0x4000, 4), (0x9000, 4)]
            .iter()
            .map(|(o, g)| [o.to_le_bytes().as_slice(), &0u32.to_le_bytes(), &g.to_le_bytes()].concat())
            .collect();
        let (table, header) = build_table(&raw, 0x10000);
        let (v, _) = ctr_ex_table(&table, &header).unwrap();
        assert_eq!(v[1], CtrExEntry { offset: 0x4000, generation: 4 });
    }

    #[test]
    fn rejects_corrupt_tables() {
        let bad_order = vec![indirect_bytes(IndirectEntry { virtual_offset: 0, physical_offset: 0, storage: 0 }), indirect_bytes(IndirectEntry { virtual_offset: 0, physical_offset: 0, storage: 1 })];
        let (t, h) = build_table(&bad_order, 10);
        assert!(indirect_table(&t, &h).is_err());
        let ok = vec![indirect_bytes(IndirectEntry { virtual_offset: 0, physical_offset: 0, storage: 5 })];
        let (t, h) = build_table(&ok, 10);
        assert!(indirect_table(&t, &h).is_err());
        let (t, mut h) = build_table(&[indirect_bytes(IndirectEntry { virtual_offset: 0, physical_offset: 0, storage: 0 })], 10);
        h[0] = b'X';
        assert!(matches!(indirect_table(&t, &h), Err(Error::BadMagic(_))));
        h[0] = b'B';
        h[8..12].copy_from_slice(&5000u32.to_le_bytes());
        assert!(indirect_table(&t, &h).is_err());
    }
}
