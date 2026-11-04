//! Checked 4 KiB mappings backed by a stable, bounded host allocation.
use crate::nro::Homebrew;
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::collections::BTreeMap;
use std::fmt;
use std::ptr::NonNull;

pub const PAGE: usize = 4096;
pub const PHYSICAL_BASE: u64 = 0x10000;
pub const ROOT: u64 = PHYSICAL_BASE + 0x1000;
pub const MAX_ARENA: usize = 512 * 1024 * 1024;
const ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;
const LIMIT: u64 = 1 << 39;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Alignment,
    Address,
    Overlap,
    Permission,
    Capacity,
    Unmapped,
    Access,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "guest memory: {self:?}")
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub physical: u64,
    pub permissions: u8,
    pub attribute: u32,
    pub state: u32,
}

pub struct MemoryImage {
    arena: NonNull<u8>,
    used: usize,
    pages: BTreeMap<u64, Mapping>,
    pending: BTreeMap<usize, u64>,
    references: BTreeMap<u64, usize>,
    executable: BTreeMap<u64, usize>,
    free: Vec<u64>,
    generation: u64,
}
// SAFETY: the allocation is exclusively owned. Every access requires a borrow;
// the HV owner places this object behind its execution mutex, including guest runs.
unsafe impl Send for MemoryImage {}
impl Drop for MemoryImage {
    fn drop(&mut self) {
        // SAFETY: allocation uses this exact layout; HV unmaps before dropping it.
        unsafe { dealloc(self.arena.as_ptr(), Self::layout()) };
    }
}
impl MemoryImage {
    fn layout() -> Layout {
        Layout::from_size_align(MAX_ARENA, 0x4000).expect("constant layout")
    }
    #[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn generation(&self) -> u64 {
        self.generation
    }
    #[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn host_pointer(&self) -> *mut u8 {
        self.arena.as_ptr()
    }
    fn bytes(&self) -> &[u8] {
        // SAFETY: used is bounded by the allocation; callers exclude guest execution.
        unsafe { std::slice::from_raw_parts(self.arena.as_ptr(), self.used) }
    }
    fn bytes_mut(&mut self) -> &mut [u8] {
        // SAFETY: exclusive borrow and execution guard exclude every other accessor.
        unsafe { std::slice::from_raw_parts_mut(self.arena.as_ptr(), self.used) }
    }
    pub fn new(boot: &Homebrew) -> Result<Self, Error> {
        // SAFETY: valid nonzero layout; its lifetime is owned by MemoryImage.
        let arena = NonNull::new(unsafe { alloc_zeroed(Self::layout()) }).ok_or(Error::Capacity)?;
        let mut image = Self {
            arena,
            used: 2 * PAGE,
            pages: BTreeMap::new(),
            pending: BTreeMap::new(),
            references: BTreeMap::new(),
            executable: BTreeMap::new(),
            free: Vec::new(),
            generation: 0,
        };
        for offset in (0..0x800).step_by(0x80) {
            image.bytes_mut()[offset..offset + 4].copy_from_slice(&0xd4000002u32.to_le_bytes());
            image.bytes_mut()[offset + 4..offset + 8].copy_from_slice(&0xd69f03e0u32.to_le_bytes());
        }
        // Private EL1 trampoline: DSB, TLBI, IC IALLU, DSB, ISB, HVC #1.
        for (i, opcode) in [
            0xd5033a9fu32,
            0xd508871f,
            0xd508751f,
            0xd5033b9f,
            0xd5033fdf,
            0xd4000022,
        ]
        .into_iter()
        .enumerate()
        {
            image.bytes_mut()[0x800 + i * 4..0x804 + i * 4].copy_from_slice(&opcode.to_le_bytes());
        }
        let slot = image.slot(PHYSICAL_BASE, true)?.ok_or(Error::Address)?;
        image.store(
            slot,
            PHYSICAL_BASE | 3 | (1 << 7) | (3 << 8) | (1 << 10) | (1 << 54),
        );
        for region in &boot.regions {
            image.map_zeroed(region.address, region.bytes.len(), region.permissions)?;
            let state = if (crate::nro::BASE..crate::nro::STACK).contains(&region.address) {
                4
            } else if (crate::nro::STACK..crate::nro::STACK + crate::nro::STACK_SIZE as u64)
                .contains(&region.address)
            {
                0xb
            } else if region.address == crate::nro::TLS {
                0xc
            } else {
                5
            };
            image.set_state(region.address, region.bytes.len(), state)?;
            for (index, page) in region.bytes.chunks_exact(PAGE).enumerate() {
                let p = image.pages[&(region.address + (index * PAGE) as u64)].physical;
                let at = (p - PHYSICAL_BASE) as usize;
                image.bytes_mut()[at..at + PAGE].copy_from_slice(page);
            }
        }
        image.flush_code(PHYSICAL_BASE);
        image.publish();
        Ok(image)
    }
    fn range(address: u64, size: usize) -> Result<std::ops::Range<u64>, Error> {
        if !address.is_multiple_of(PAGE as u64) || size == 0 || !size.is_multiple_of(PAGE) {
            return Err(Error::Alignment);
        }
        let end = address
            .checked_add(size as u64)
            .filter(|v| *v <= LIMIT)
            .ok_or(Error::Address)?;
        if address <= PHYSICAL_BASE && end > PHYSICAL_BASE {
            return Err(Error::Overlap);
        }
        Ok(address..end)
    }
    fn flags(permissions: u8) -> Result<u64, Error> {
        Ok(match permissions {
            0 => return Ok(0),
            1 => (3 << 6) | (1 << 54),
            3 => (1 << 6) | (1 << 54),
            5 => 3 << 6,
            _ => return Err(Error::Permission),
        } | 3
            | (3 << 8)
            | (1 << 10)
            | (1 << 53))
    }
    fn allocate(&mut self) -> Result<u64, Error> {
        if self.used > MAX_ARENA - PAGE {
            return Err(Error::Capacity);
        }
        let result = PHYSICAL_BASE + self.used as u64;
        self.used += PAGE;
        Ok(result)
    }
    fn store(&mut self, at: usize, value: u64) {
        self.bytes_mut()[at..at + 8].copy_from_slice(&value.to_le_bytes());
    }
    fn slot(&mut self, address: u64, create: bool) -> Result<Option<usize>, Error> {
        let mut table = ROOT;
        for shift in [30, 21, 12] {
            let at = (table - PHYSICAL_BASE + ((address >> shift) & 511) * 8) as usize;
            if shift == 12 {
                return Ok(Some(at));
            }
            let old = u64::from_le_bytes(self.bytes()[at..at + 8].try_into().expect("table entry"));
            if old == 0 {
                if !create {
                    return Ok(None);
                }
                table = self.allocate()?;
                self.store(at, table | 3);
            } else {
                table = old & ADDRESS_MASK;
            }
        }
        unreachable!()
    }
    fn capacity(&self, range: &std::ops::Range<u64>, data: bool) -> Result<(), Error> {
        let mut tables = std::collections::BTreeSet::new();
        for address in range.clone().step_by(PAGE) {
            let l1 = (ROOT - PHYSICAL_BASE + ((address >> 30) & 511) * 8) as usize;
            let root = u64::from_le_bytes(self.bytes()[l1..l1 + 8].try_into().expect("entry"));
            if root == 0 {
                tables.insert((1, address >> 30));
                tables.insert((2, address >> 21));
            } else {
                let l2 =
                    ((root & ADDRESS_MASK) - PHYSICAL_BASE + ((address >> 21) & 511) * 8) as usize;
                if self.bytes()[l2..l2 + 8] == [0; 8] {
                    tables.insert((2, address >> 21));
                }
            }
        }
        let count = tables.len()
            + if data {
                (((range.end - range.start) / PAGE as u64) as usize).saturating_sub(self.free.len())
            } else {
                0
            };
        if count > (MAX_ARENA - self.used) / PAGE {
            Err(Error::Capacity)
        } else {
            Ok(())
        }
    }
    fn change(&mut self, address: u64, mapping: Mapping) -> Result<(), Error> {
        let slot = self.slot(address, true)?.ok_or(Error::Address)?;
        // Break before make: remove the old descriptor now, publish after a guest
        // DSB/TLBI/DSB/ISB. Each CPU invalidates before it may re-enter EL0.
        self.store(slot, 0);
        let flags =
            Self::flags(mapping.permissions)? | if mapping.attribute & 8 != 0 { 4 } else { 0 };
        self.pending.insert(
            slot,
            if mapping.permissions == 0 {
                0
            } else {
                mapping.physical | flags
            },
        );
        if let Some(old) = self.pages.get(&address)
            && old.permissions & 4 != 0
        {
            let count = self
                .executable
                .get_mut(&old.physical)
                .expect("executable ref");
            *count -= 1;
            if *count == 0 {
                self.executable.remove(&old.physical);
            }
        }
        if mapping.permissions & 4 != 0 {
            *self.executable.entry(mapping.physical).or_default() += 1;
        }
        self.pages.insert(address, mapping);
        Ok(())
    }
    fn flush_code(&self, physical: u64) {
        #[cfg(target_os = "macos")]
        {
            unsafe extern "C" {
                fn sys_icache_invalidate(start: *mut std::ffi::c_void, len: usize);
            }
            // SAFETY: physical denotes an allocated page inside this stable arena.
            // The caller holds exclusive access and excludes guest execution.
            unsafe {
                sys_icache_invalidate(
                    self.arena
                        .as_ptr()
                        .add((physical - PHYSICAL_BASE) as usize)
                        .cast(),
                    PAGE,
                );
            }
        }
        #[cfg(not(target_os = "macos"))]
        let _ = physical;
    }
    /// Cache maintenance is bounded to one advertised 64-byte guest line. No
    /// inaccessible mapping (including EL1 bridge/tables) can name host backing.
    #[cfg(all(feature = "hv", target_os = "macos", target_arch = "aarch64"))]
    pub(crate) fn cache_line(&mut self, address: u64, instruction: bool) -> Result<(), Error> {
        let address = address & !63;
        self.access(address, 64, 1)?;
        let physical = self.pages[&(address & !4095)].physical;
        let offset = (physical - PHYSICAL_BASE + (address & 4095)) as usize;
        unsafe extern "C" {
            fn sys_icache_invalidate(start: *mut std::ffi::c_void, len: usize);
            fn sys_dcache_flush(start: *mut std::ffi::c_void, len: usize);
        }
        // SAFETY: range is validated above and 64-byte alignment cannot cross a
        // 4 KiB mapping. ProcessGuard excludes concurrent guest/host memory use.
        unsafe {
            let pointer = self.arena.as_ptr().add(offset).cast();
            if instruction {
                sys_icache_invalidate(pointer, 64);
            } else {
                sys_dcache_flush(pointer, 64);
            }
        }
        if instruction {
            self.generation += 1;
        }
        Ok(())
    }
    pub(crate) fn publish(&mut self) {
        for (at, value) in std::mem::take(&mut self.pending) {
            if value != 0 && value & (1 << 54) == 0 {
                self.flush_code(value & ADDRESS_MASK);
            }
            self.store(at, value);
        }
    }
    pub fn map_zeroed(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error> {
        Self::flags(permissions)?;
        let range = Self::range(address, size)?;
        if range
            .clone()
            .step_by(PAGE)
            .any(|a| self.pages.contains_key(&a))
        {
            return Err(Error::Overlap);
        }
        self.capacity(&range, true)?;
        for a in range.step_by(PAGE) {
            let physical = if let Some(physical) = self.free.pop() {
                let at = (physical - PHYSICAL_BASE) as usize;
                self.bytes_mut()[at..at + PAGE].fill(0);
                physical
            } else {
                self.allocate()?
            };
            self.references.insert(physical, 1);
            self.change(
                a,
                Mapping {
                    physical,
                    permissions,
                    attribute: 0,
                    state: 5,
                },
            )?;
        }
        self.generation += 1;
        Ok(())
    }
    fn mapped_range(&self, address: u64, size: usize) -> Result<std::ops::Range<u64>, Error> {
        let range = Self::range(address, size)?;
        if range
            .clone()
            .step_by(PAGE)
            .any(|a| !self.pages.contains_key(&a))
        {
            return Err(Error::Unmapped);
        }
        Ok(range)
    }
    pub fn protect(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error> {
        Self::flags(permissions)?;
        let range = self.mapped_range(address, size)?;
        if range
            .clone()
            .step_by(PAGE)
            .any(|a| self.pages[&a].attribute & 16 != 0)
        {
            return Err(Error::Permission);
        }
        for a in range.step_by(PAGE) {
            let mut mapping = self.pages[&a];
            mapping.permissions = permissions;
            self.change(a, mapping)?;
        }
        self.generation += 1;
        Ok(())
    }
    pub fn attribute(
        &mut self,
        address: u64,
        size: usize,
        mask: u32,
        value: u32,
    ) -> Result<(), Error> {
        if mask & !24 != 0 || value & !mask != 0 {
            return Err(Error::Permission);
        }
        let range = self.mapped_range(address, size)?;
        for a in range.step_by(PAGE) {
            let mut m = self.pages[&a];
            m.attribute = (m.attribute & !mask) | value;
            self.change(a, m)?;
        }
        self.generation += 1;
        Ok(())
    }
    pub fn unmap(&mut self, address: u64, size: usize) -> Result<(), Error> {
        let range = self.mapped_range(address, size)?;
        for a in range.step_by(PAGE) {
            let at = self.slot(a, false)?.ok_or(Error::Unmapped)?;
            self.store(at, 0);
            self.pending.remove(&at);
            let old = self.pages.remove(&a).expect("validated");
            let physical = old.physical;
            if old.permissions & 4 != 0 {
                let count = self.executable.get_mut(&physical).expect("executable ref");
                *count -= 1;
                if *count == 0 {
                    self.executable.remove(&physical);
                }
            }
            let references = self
                .references
                .get_mut(&physical)
                .expect("physical reference");
            *references -= 1;
            if *references == 0 {
                self.references.remove(&physical);
                self.free.push(physical);
            }
        }
        self.generation += 1;
        Ok(())
    }
    pub fn alias(
        &mut self,
        destination: u64,
        source: u64,
        size: usize,
        permissions: u8,
    ) -> Result<(), Error> {
        Self::flags(permissions)?;
        let src = self.mapped_range(source, size)?;
        let dst = Self::range(destination, size)?;
        if dst
            .clone()
            .step_by(PAGE)
            .any(|a| self.pages.contains_key(&a))
        {
            return Err(Error::Overlap);
        }
        self.capacity(&dst, false)?;
        for (s, d) in src.step_by(PAGE).zip(dst.step_by(PAGE)) {
            let mut m = self.pages[&s];
            m.permissions = permissions;
            *self
                .references
                .get_mut(&m.physical)
                .expect("physical reference") += 1;
            self.change(d, m)?;
        }
        self.generation += 1;
        Ok(())
    }
    fn access(&self, address: u64, size: usize, permission: u8) -> Result<(), Error> {
        if size == 0 {
            return Ok(());
        }
        let end = address
            .checked_add(size as u64)
            .filter(|v| *v <= LIMIT)
            .ok_or(Error::Address)?;
        for a in ((address & !4095)..end).step_by(PAGE) {
            let m = self.pages.get(&a).ok_or(Error::Unmapped)?;
            if m.permissions & permission != permission {
                return Err(Error::Access);
            }
        }
        Ok(())
    }
    pub fn check_access(&self, address: u64, size: usize, permission: u8) -> Result<(), Error> {
        self.access(address, size, permission)
    }
    pub fn set_state(&mut self, address: u64, size: usize, state: u32) -> Result<(), Error> {
        for a in self.mapped_range(address, size)?.step_by(PAGE) {
            self.pages.get_mut(&a).expect("validated").state = state;
        }
        Ok(())
    }
    pub fn read(&self, address: u64, output: &mut [u8]) -> Result<(), Error> {
        self.access(address, output.len(), 1)?;
        let mut copied = 0;
        while copied < output.len() {
            let a = address + copied as u64;
            let offset = (a & 4095) as usize;
            let n = (PAGE - offset).min(output.len() - copied);
            let p = (self.pages[&(a & !4095)].physical - PHYSICAL_BASE) as usize + offset;
            output[copied..copied + n].copy_from_slice(&self.bytes()[p..p + n]);
            copied += n;
        }
        Ok(())
    }
    pub fn write(&mut self, address: u64, input: &[u8]) -> Result<(), Error> {
        self.access(address, input.len(), 2)?;
        self.write_kernel(address, input)
    }
    pub fn write_kernel(&mut self, address: u64, input: &[u8]) -> Result<(), Error> {
        self.access(address, input.len(), 0)?;
        let mut copied = 0;
        let mut code_changed = false;
        while copied < input.len() {
            let a = address + copied as u64;
            let offset = (a & 4095) as usize;
            let n = (PAGE - offset).min(input.len() - copied);
            let p = (self.pages[&(a & !4095)].physical - PHYSICAL_BASE) as usize + offset;
            self.bytes_mut()[p..p + n].copy_from_slice(&input[copied..copied + n]);
            let physical = self.pages[&(a & !4095)].physical;
            if self.executable.contains_key(&physical) {
                self.flush_code(physical);
                code_changed = true;
            }
            copied += n;
        }
        if code_changed {
            self.generation += 1;
        }
        Ok(())
    }
    pub fn query(&self, address: u64) -> Result<(u64, u64, Option<Mapping>), Error> {
        if address >= LIMIT {
            return Err(Error::Address);
        }
        let page = address & !4095;
        let mapping = self.pages.get(&page).copied();
        let same = |m: &Mapping| {
            mapping.is_some_and(|x| {
                (x.permissions, x.attribute, x.state) == (m.permissions, m.attribute, m.state)
            })
        };
        let mut start = page;
        let mut end = page + PAGE as u64;
        if mapping.is_some() {
            while start >= PAGE as u64 && self.pages.get(&(start - PAGE as u64)).is_some_and(same) {
                start -= PAGE as u64;
            }
            while end < LIMIT && self.pages.get(&end).is_some_and(same) {
                end += PAGE as u64;
            }
        } else {
            start = self
                .pages
                .range(..page)
                .next_back()
                .map_or(0, |(a, _)| a + PAGE as u64);
            end = self.pages.range(page..).next().map_or(LIMIT, |(a, _)| *a);
        }
        Ok((start, end - start, mapping))
    }
}

/// Borrow-only memory interface. It cannot replace or extract the VM allocation.
pub trait GuestMemory {
    fn map_zeroed(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error>;
    fn unmap(&mut self, address: u64, size: usize) -> Result<(), Error>;
    fn protect(&mut self, address: u64, size: usize, permissions: u8) -> Result<(), Error>;
    fn alias(
        &mut self,
        destination: u64,
        source: u64,
        size: usize,
        permissions: u8,
    ) -> Result<(), Error>;
    fn attribute(&mut self, address: u64, size: usize, mask: u32, value: u32) -> Result<(), Error>;
    fn read(&self, address: u64, output: &mut [u8]) -> Result<(), Error>;
    fn write(&mut self, address: u64, input: &[u8]) -> Result<(), Error>;
    fn write_kernel(&mut self, address: u64, input: &[u8]) -> Result<(), Error>;
    fn check_access(&self, address: u64, size: usize, permission: u8) -> Result<(), Error>;
    fn set_state(&mut self, address: u64, size: usize, state: u32) -> Result<(), Error>;
    fn query(&self, address: u64) -> Result<(u64, u64, Option<Mapping>), Error>;
}
impl GuestMemory for MemoryImage {
    fn map_zeroed(&mut self, a: u64, n: usize, p: u8) -> Result<(), Error> {
        Self::map_zeroed(self, a, n, p)
    }
    fn unmap(&mut self, a: u64, n: usize) -> Result<(), Error> {
        Self::unmap(self, a, n)
    }
    fn protect(&mut self, a: u64, n: usize, p: u8) -> Result<(), Error> {
        Self::protect(self, a, n, p)
    }
    fn alias(&mut self, d: u64, s: u64, n: usize, p: u8) -> Result<(), Error> {
        Self::alias(self, d, s, n, p)
    }
    fn attribute(&mut self, a: u64, n: usize, m: u32, v: u32) -> Result<(), Error> {
        Self::attribute(self, a, n, m, v)
    }
    fn read(&self, a: u64, o: &mut [u8]) -> Result<(), Error> {
        Self::read(self, a, o)
    }
    fn write(&mut self, a: u64, i: &[u8]) -> Result<(), Error> {
        Self::write(self, a, i)
    }
    fn write_kernel(&mut self, a: u64, i: &[u8]) -> Result<(), Error> {
        Self::write_kernel(self, a, i)
    }
    fn check_access(&self, a: u64, n: usize, p: u8) -> Result<(), Error> {
        Self::check_access(self, a, n, p)
    }
    fn set_state(&mut self, a: u64, n: usize, s: u32) -> Result<(), Error> {
        Self::set_state(self, a, n, s)
    }
    fn query(&self, a: u64) -> Result<(u64, u64, Option<Mapping>), Error> {
        Self::query(self, a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nro::Region;
    fn boot() -> Homebrew {
        Homebrew {
            regions: vec![Region {
                address: 0x100000000,
                bytes: vec![0; PAGE],
                permissions: 5,
            }],
            pc: 0x100000000,
            sp: 0,
            x0: 0,
            x1: 0,
            x30: 0,
            tpidrro_el0: 0,
        }
    }
    #[test]
    fn rejects_invalid_mappings_without_overwriting_bridge_or_tables() {
        for (address, size, permissions, expected) in [
            (1, PAGE, 5, Error::Alignment),
            (0x100000000, 1, 5, Error::Alignment),
            (1 << 39, PAGE, 5, Error::Address),
            (PHYSICAL_BASE, PAGE, 5, Error::Overlap),
            (0x100000000, PAGE, 7, Error::Permission),
        ] {
            let mut b = boot();
            b.regions[0] = Region {
                address,
                bytes: vec![0; size],
                permissions,
            };
            assert_eq!(MemoryImage::new(&b).err(), Some(expected));
        }
        let mut b = boot();
        b.regions.push(Region {
            address: 0x100000000,
            bytes: vec![0; PAGE],
            permissions: 5,
        });
        assert_eq!(MemoryImage::new(&b).err(), Some(Error::Overlap));
    }
    #[test]
    fn checked_live_memory_and_aliases() {
        let mut m = MemoryImage::new(&boot()).unwrap();
        m.map_zeroed(0x20000, PAGE * 2, 3).unwrap();
        m.write(0x20ffe, &[1, 2, 3, 4]).unwrap();
        m.alias(0x30000, 0x20000, PAGE * 2, 3).unwrap();
        let mut b = [0; 4];
        m.read(0x30ffe, &mut b).unwrap();
        assert_eq!(b, [1, 2, 3, 4]);
        m.protect(0x21000, PAGE, 1).unwrap();
        assert_eq!(m.write(0x20ffe, &[9; 4]), Err(Error::Access));
        m.read(0x20ffe, &mut b).unwrap();
        assert_eq!(b, [1, 2, 3, 4]);
        assert_eq!(m.unmap(0x20000, PAGE * 3), Err(Error::Unmapped));
        m.read(0x20000, &mut b).unwrap();
        m.protect(0x20000, PAGE, 0).unwrap();
        assert_eq!(m.read(0x20000, &mut b), Err(Error::Access));
        assert_eq!(m.map_zeroed(0x20000, PAGE, 3), Err(Error::Overlap));
        m.unmap(0x20000, PAGE * 2).unwrap();
        m.read(0x30ffe, &mut b).unwrap();
        assert_eq!(b, [1, 2, 3, 4]);
    }
}
