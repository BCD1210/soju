//! GPU virtual address allocation and backing identity.
use super::super::types::{Error, NvBufferView};
use std::collections::BTreeMap;
const LIMIT: u64 = 1 << 40;
#[derive(Clone, Debug)]
pub(crate) struct Mapping {
    pub size: u64,
    pub view: NvBufferView,
    pub offset: u64,
    pub kind: u32,
    span: u64,
}
#[derive(Clone, Default, Debug)]
pub(crate) struct AddressSpace {
    pub mappings: BTreeMap<u64, Mapping>,
    reservations: BTreeMap<u64, u64>,
}
impl AddressSpace {
    fn vacant(&self, size: u64, alignment: u64) -> Result<u64, Error> {
        let mut occupied: Vec<_> = self
            .reservations
            .iter()
            .map(|(&a, &n)| (a, n))
            .chain(self.mappings.iter().map(|(&a, m)| (a, m.span)))
            .collect();
        occupied.sort_unstable();
        let mut base = align(0x100000000, alignment)?;
        for (address, length) in occupied {
            if base.checked_add(size).is_some_and(|end| end <= address) {
                break;
            }
            if base < address + length {
                base = align(address + length, alignment)?;
            }
        }
        if base.checked_add(size).is_none_or(|end| end > LIMIT) {
            return Err(Error::Malformed("GPU address space exhausted"));
        }
        Ok(base)
    }
    pub fn reserve(&mut self, data: &[u8]) -> Result<u64, Error> {
        use super::super::types::{read_u32 as u32at, read_u64 as u64at};
        let pages = u64::from(u32at(data, 0)?);
        let page = u64::from(u32at(data, 4)?);
        let alignment = u64at(data, 16)?;
        if pages == 0
            || !matches!(page, 4096 | 65536 | 131072)
            || u32at(data, 8)? != 0
            || u32at(data, 12)? != 0
            || alignment < page
            || !alignment.is_power_of_two()
            || self.reservations.len() >= 4096
        {
            return Err(Error::Malformed("GPU reservation parameters"));
        }
        let size = pages
            .checked_mul(page)
            .filter(|&n| n <= LIMIT)
            .ok_or(Error::Malformed("GPU reservation size"))?;
        let base = self.vacant(size, alignment)?;
        self.reservations.insert(base, size);
        Ok(base)
    }
    pub fn map(&mut self, view: NvBufferView, data: &[u8]) -> Result<u64, Error> {
        use super::super::types::{read_u32 as u32at, read_u64 as u64at};
        let kind = u32at(data, 4)?;
        let page = u64::from(u32at(data, 12)?);
        let offset = u64at(data, 16)?;
        let requested = u64at(data, 24)?;
        if u32at(data, 0)? & !4 != 0
            || !matches!(kind, 0 | 0x11 | 0xfe)
            || !matches!(page, 4096 | 65536 | 131072)
            || u64at(data, 32)? != 0
            || view.guest_address == 0
            || !view.guest_address.is_multiple_of(4096)
            || view.size == 0
            || !view.size.is_multiple_of(4096)
            || view.size > 1 << 30
            || offset % 4096 != 0
            || offset >= view.size
            || self.mappings.len() >= 4096
        {
            return Err(Error::Message(format!(
                "GPU dynamic mapping parameters: {:02x?}",
                data
            )));
        }
        let size = if requested == 0 {
            view.size - offset
        } else {
            requested
        };
        if size == 0 || size % 4096 != 0 || size > view.size - offset {
            return Err(Error::Malformed("GPU mapping exceeds allocation"));
        }
        if self.mappings.values().any(|m| {
            m.view.allocation == view.allocation
                && (m.view.generation != view.generation
                    || m.view.guest_address != view.guest_address
                    || m.view.size != view.size)
        }) {
            return Err(Error::Malformed("GPU allocation backing changed"));
        }
        let span = align(size, page)?;
        let base = self.vacant(span, page)?;
        self.mappings.insert(
            base,
            Mapping {
                size,
                view,
                offset,
                kind,
                span,
            },
        );
        Ok(base)
    }
    pub fn resolve(&self, address: u64, size: usize) -> Result<(u64, u32), Error> {
        let size = u64::try_from(size).map_err(|_| Error::Malformed("GPU range size"))?;
        let end = address
            .checked_add(size)
            .filter(|&n| size > 0 && n <= LIMIT)
            .ok_or(Error::Malformed("GPU address range"))?;
        let (&base, mapping) = self
            .mappings
            .range(..=address)
            .next_back()
            .ok_or(Error::Malformed("unmapped GPU address"))?;
        if end > base + mapping.size {
            return Err(Error::Malformed("GPU mapping bounds"));
        }
        let guest = mapping
            .view
            .guest_address
            .checked_add(mapping.offset)
            .and_then(|n| n.checked_add(address - base))
            .ok_or(Error::Malformed("GPU backing overflow"))?;
        Ok((guest, mapping.kind))
    }
}
fn align(value: u64, alignment: u64) -> Result<u64, Error> {
    value
        .checked_add(alignment - 1)
        .map(|n| n & !(alignment - 1))
        .ok_or(Error::Malformed("GPU alignment overflow"))
}
