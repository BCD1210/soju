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
    originals: BTreeMap<u64, Mapping>,
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
        if u32at(data, 0)? & !5 != 0
            || !matches!(
                kind,
                0 | 0x1 | 0x2 | 0x11 | 0x17 | 0x51 | 0x53 | 0xdb | 0xe0 | 0xe9 | 0xfe
            )
            || !matches!(page, 4096 | 65536 | 131072)
            || (u32at(data, 0)? & 1 == 0 && u64at(data, 32)? != 0)
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
        let base = if u32at(data, 0)? & 1 != 0 {
            let base = u64at(data, 32)?;
            let end = base
                .checked_add(span)
                .filter(|&n| n <= LIMIT)
                .ok_or(Error::Malformed("GPU fixed mapping overflow"))?;
            if !base.is_multiple_of(page)
                || !self
                    .reservations
                    .iter()
                    .any(|(&a, &n)| a <= base && end <= a + n)
                || self
                    .mappings
                    .iter()
                    .any(|(&a, m)| base < a + m.span && a < end)
            {
                return Err(Error::Malformed(
                    "GPU fixed mapping outside reservation or occupied",
                ));
            }
            base
        } else {
            self.vacant(span, page)?
        };
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
        self.originals.insert(base, self.mappings[&base].clone());
        Ok(base)
    }
    pub fn modify(&mut self, data: &[u8]) -> Result<(), Error> {
        use super::super::types::{read_u32 as u32at, read_u64 as u64at};
        let kind = u32at(data, 4)?;
        let base = u64at(data, 32)?;
        let offset = u64at(data, 16)?;
        let size = u64at(data, 24)?;
        if u32at(data, 0)? != 0x100
            || u32at(data, 8)? != 0
            || u32at(data, 12)? != 0
            || !matches!(
                kind,
                0 | 0x1 | 0x2 | 0x11 | 0x17 | 0x51 | 0x53 | 0xdb | 0xe0 | 0xe9 | 0xfe
            )
            || !offset.is_multiple_of(4096)
            || size == 0
            || !size.is_multiple_of(4096)
        {
            return Err(Error::Message(format!(
                "GPU mapping modification parameters: kind={kind:#x} offset={offset:#x} size={size:#x} page={:#x}",
                u32at(data, 12)?
            )));
        }
        let mapping = self
            .originals
            .get(&base)
            .ok_or(Error::Malformed(
                "GPU modification requires existing mapping",
            ))?
            .clone();
        let end = offset
            .checked_add(size)
            .filter(|&n| n <= mapping.size)
            .ok_or_else(|| Error::Message(format!("GPU modification exceeds mapping: base={base:#x} offset={offset:#x} size={size:#x} mapping={mapping:?}")))?;
        let start = base + offset;
        let stop = base + end;
        let affected: Vec<_> = self
            .mappings
            .range(base..base + mapping.size)
            .filter(|(address, m)| **address < stop && start < **address + m.size)
            .map(|(&address, m)| (address, m.clone()))
            .collect();
        let mut updated = self.mappings.clone();
        for (address, old) in affected {
            updated.remove(&address);
            if address < start {
                updated.insert(
                    address,
                    Mapping {
                        size: start - address,
                        span: start - address,
                        ..old.clone()
                    },
                );
            }
            if address + old.size > stop {
                updated.insert(
                    stop,
                    Mapping {
                        size: address + old.size - stop,
                        offset: old.offset + stop - address,
                        span: address + old.span - stop,
                        ..old
                    },
                );
            }
        }
        updated.insert(
            start,
            Mapping {
                size,
                offset: mapping.offset + offset,
                kind,
                span: if end == mapping.size {
                    mapping.span - offset
                } else {
                    size
                },
                ..mapping
            },
        );
        if updated.len() > 4096 {
            return Err(Error::Malformed("GPU mapping limit"));
        }
        self.mappings = updated;
        Ok(())
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
