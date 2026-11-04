//! Checked guest-address space. All writers advance range generations.
use crate::{Error, Result};
use std::{
    collections::BTreeMap,
    ops::Range,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT_SPACE: AtomicU64 = AtomicU64::new(1);
pub struct Memory {
    pub(crate) identity: u64,
    regions: Vec<(u64, Vec<u8>)>,
    writes: BTreeMap<u64, u64>,
    generation: u64,
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            identity: NEXT_SPACE.fetch_add(1, Ordering::Relaxed),
            regions: Vec::new(),
            writes: BTreeMap::new(),
            generation: 0,
        }
    }
}
impl Memory {
    pub(crate) fn epoch(&self) -> (u64, u64) {
        (self.identity, self.generation)
    }
    pub fn map(&mut self, address: u64, bytes: Vec<u8>) -> Result<()> {
        let end = address
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| Error("address overflow".into()))?;
        if bytes.is_empty()
            || self
                .regions
                .iter()
                .any(|(a, b)| address < *a + b.len() as u64 && *a < end)
        {
            return Err(Error("empty or overlapping mapping".into()));
        }
        self.regions.push((address, bytes));
        self.mark(address..end);
        Ok(())
    }
    fn mark(&mut self, range: Range<u64>) {
        if range.is_empty() {
            return;
        }
        self.generation += 1;
        let after = self
            .writes
            .range(..=range.end)
            .next_back()
            .map_or(0, |(_, g)| *g);
        let covered: Vec<_> = self.writes.range(range.clone()).map(|(a, _)| *a).collect();
        for address in covered {
            self.writes.remove(&address);
        }
        self.writes.insert(range.start, self.generation);
        self.writes.insert(range.end, after);
    }

    pub fn read(&self, address: u64, size: usize) -> Result<&[u8]> {
        let end = address
            .checked_add(size as u64)
            .ok_or_else(|| Error("address overflow".into()))?;
        for (a, b) in &self.regions {
            if address >= *a && end <= *a + b.len() as u64 {
                return Ok(&b[(address - *a) as usize..(end - *a) as usize]);
            }
        }
        Err(Error(format!("unmapped GPU range {address:#x}+{size:#x}")))
    }
    pub fn write(&mut self, address: u64, data: &[u8]) -> Result<()> {
        self.read(address, data.len())?;
        let (a, b) = self
            .regions
            .iter_mut()
            .find(|(a, b)| address >= *a && address + data.len() as u64 <= *a + b.len() as u64)
            .unwrap();
        let offset = (address - *a) as usize;
        b[offset..offset + data.len()].copy_from_slice(data);
        self.mark(address..address + data.len() as u64);
        Ok(())
    }
    pub fn generation(&self, address: u64, size: usize) -> Result<u64> {
        self.read(address, size)?;
        if size == 0 {
            return Ok(0);
        }
        let first = self
            .writes
            .range(..=address)
            .next_back()
            .map_or(0, |(_, g)| *g);
        Ok(self
            .writes
            .range(address..address + size as u64)
            .map(|(_, g)| *g)
            .fold(first, u64::max))
    }
}
