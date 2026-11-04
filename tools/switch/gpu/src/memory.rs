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

    pub fn available(&self, address: u64) -> Result<usize> {
        self.regions
            .iter()
            .find_map(|(a, b)| {
                (address >= *a && address - *a < b.len() as u64)
                    .then(|| b.len() - (address - *a) as usize)
            })
            .ok_or_else(|| Error("unmapped GPU address".into()))
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
    /// Update only differing pages, preserving shader/resource generations for unchanged bytes.
    pub fn synchronize(&mut self, address: u64, data: &[u8]) -> Result<()> {
        self.read(address, data.len())?;
        for (i, page) in data.chunks(4096).enumerate() {
            let a = address + (i * 4096) as u64;
            if self.read(a, page.len())? != page {
                self.write(a, page)?;
            }
        }
        Ok(())
    }
    pub fn revision(&self) -> u64 {
        self.generation
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn synchronization_preserves_unchanged_ranges_and_marks_changed_pages() {
        let mut memory = Memory::default();
        let mut data = vec![7; 8193];
        memory.map(0x1000, data.clone()).unwrap();
        let original = memory.revision();
        memory.synchronize(0x1000, &data).unwrap();
        assert_eq!(memory.revision(), original);
        data[8192] = 9;
        memory.synchronize(0x1000, &data).unwrap();
        assert_eq!(memory.generation(0x1000, 8192).unwrap(), original);
        assert!(memory.generation(0x3000, 1).unwrap() > original);
        assert_eq!(memory.read(0x1000, data.len()).unwrap(), data);
        assert!(memory.synchronize(0x3000, &[0, 1]).is_err());
    }
}
