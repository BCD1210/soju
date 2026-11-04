//! 4 KiB stage-1 tables for a 39-bit user address space. Table pages are never
//! mapped into user space; all user mappings are privileged-execute-never.
use crate::nro::Homebrew;
use std::fmt;

pub const PAGE: usize = 4096;
pub const PHYSICAL_BASE: u64 = 0x10000;
pub const ROOT: u64 = PHYSICAL_BASE + 0x1000;
const ADDRESS_MASK: u64 = 0x0000_ffff_ffff_f000;
const MAX_ARENA: usize = 512 * 1024 * 1024;

#[derive(Debug, PartialEq, Eq)]
pub enum Error {
    Alignment,
    Address,
    Overlap,
    Permission,
    Capacity,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "guest memory: {self:?}")
    }
}
impl std::error::Error for Error {}

pub struct MemoryImage {
    pub bytes: Vec<u8>,
}
impl MemoryImage {
    pub fn new(boot: &Homebrew) -> Result<Self, Error> {
        // First page: EL1 exception bridge. Second page: L1 table.
        let mut image = Self {
            bytes: vec![0; 2 * PAGE],
        };
        for offset in (0..0x800).step_by(0x80) {
            image.bytes[offset..offset + 4].copy_from_slice(&0xd4000002u32.to_le_bytes());
            image.bytes[offset + 4..offset + 8].copy_from_slice(&0xd69f03e0u32.to_le_bytes());
        }
        // EL1 RX, EL0 inaccessible, normal inner-shareable memory, AF set.
        image.map(
            PHYSICAL_BASE,
            PHYSICAL_BASE | 3 | (1 << 7) | (3 << 8) | (1 << 10) | (1 << 54),
        )?;
        for region in &boot.regions {
            if !region.address.is_multiple_of(4096) || !region.bytes.len().is_multiple_of(PAGE) {
                return Err(Error::Alignment);
            }
            let flags = match region.permissions {
                1 => (3 << 6) | (1 << 54),
                3 => (1 << 6) | (1 << 54),
                5 => 3 << 6,
                _ => return Err(Error::Permission),
            } | 3
                | (3 << 8)
                | (1 << 10)
                | (1 << 53);
            for (index, page) in region.bytes.chunks_exact(PAGE).enumerate() {
                let offset = u64::try_from(index * PAGE).map_err(|_| Error::Address)?;
                let address = region.address.checked_add(offset).ok_or(Error::Address)?;
                let physical = image.allocate()?;
                let start =
                    usize::try_from(physical - PHYSICAL_BASE).map_err(|_| Error::Address)?;
                image.bytes[start..start + PAGE].copy_from_slice(page);
                image.map(address, physical | flags)?;
            }
        }
        // Hypervisor.framework maps host pages (16 KiB on Apple Silicon).
        let size = (image.bytes.len() + 0x3fff) & !0x3fff;
        image.bytes.resize(size, 0);
        Ok(image)
    }

    fn allocate(&mut self) -> Result<u64, Error> {
        let offset = self.bytes.len();
        if offset > MAX_ARENA - PAGE {
            return Err(Error::Capacity);
        }
        self.bytes.resize(offset + PAGE, 0);
        Ok(PHYSICAL_BASE + u64::try_from(offset).map_err(|_| Error::Capacity)?)
    }

    fn map(&mut self, address: u64, descriptor: u64) -> Result<(), Error> {
        if address >= 1 << 39 {
            return Err(Error::Address);
        }
        let mut table = ROOT;
        for shift in [30, 21, 12] {
            let index = (address >> shift) & 511;
            let at =
                usize::try_from(table - PHYSICAL_BASE + index * 8).map_err(|_| Error::Address)?;
            let old = u64::from_le_bytes(
                self.bytes[at..at + 8]
                    .try_into()
                    .map_err(|_| Error::Address)?,
            );
            if shift == 12 {
                if old != 0 {
                    return Err(Error::Overlap);
                }
                self.bytes[at..at + 8].copy_from_slice(&descriptor.to_le_bytes());
            } else if old == 0 {
                table = self.allocate()?;
                self.bytes[at..at + 8].copy_from_slice(&(table | 3).to_le_bytes());
            } else {
                table = old & ADDRESS_MASK;
            }
        }
        Ok(())
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
            let mut boot = boot();
            boot.regions[0] = Region {
                address,
                bytes: vec![0; size],
                permissions,
            };
            assert_eq!(MemoryImage::new(&boot).err(), Some(expected));
        }
        let mut boot = boot();
        boot.regions.push(Region {
            address: 0x100000000,
            bytes: vec![0; PAGE],
            permissions: 3,
        });
        assert_eq!(MemoryImage::new(&boot).err(), Some(Error::Overlap));
    }
}
