//! A06F host semaphore fields from NVIDIA cla06f.h. Runner owns channel suspension.
use crate::{memory::Memory, require, worker::Fence, Error, Result};
#[derive(Default)]
pub struct Host {
    upper: u32,
    lower: u32,
    payload: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Acquire,
    Release,
    AcquireGe,
    AcquireAnd,
}
#[derive(Debug, Clone, Copy)]
pub struct Semaphore {
    pub address: u64,
    pub payload: u32,
    pub operation: Operation,
}
impl Host {
    /// Byte offsets. Unsupported release sizes and flags fail before side effects.
    pub fn method(&mut self, offset: u32, value: u32) -> Result<Option<Semaphore>> {
        match offset {
            0x10 => {
                require(value <= 255, "semaphore upper address")?;
                self.upper = value;
            }
            0x14 => {
                require(value & 3 == 0, "semaphore alignment")?;
                self.lower = value;
            }
            0x18 => self.payload = value,
            0x1c => {
                require(value & !0x0110100f == 0, "semaphore flags")?;
                let operation = match value & 15 {
                    1 => Operation::Acquire,
                    2 => Operation::Release,
                    4 => Operation::AcquireGe,
                    8 => Operation::AcquireAnd,
                    _ => return Err(Error("semaphore operation".into())),
                };
                if operation == Operation::Release {
                    require(
                        value & (1 << 24) != 0 && value & (1 << 20) == 0 && value & (1 << 12) == 0,
                        "only four-byte WFI semaphore release implemented",
                    )?;
                } else {
                    require(value & 0x01100000 == 0, "release flags on acquire")?;
                }
                return Ok(Some(Semaphore {
                    address: (u64::from(self.upper) << 32) | u64::from(self.lower),
                    payload: self.payload,
                    operation,
                }));
            }
            _ => return Err(Error(format!("unsupported A06F method {offset:#x}"))),
        }
        Ok(None)
    }
}
impl Semaphore {
    /// A false result suspends this guest channel; retry after the producer writes.
    /// GE uses unsigned comparison; AND requires any common set bit.
    pub fn acquired(&self, memory: &Memory) -> Result<bool> {
        let value = u32::from_le_bytes(memory.read(self.address, 4)?.try_into().unwrap());
        match self.operation {
            Operation::Acquire => Ok(value == self.payload),
            Operation::AcquireGe => Ok(value >= self.payload),
            Operation::AcquireAnd => Ok(value & self.payload != 0),
            Operation::Release => Err(Error("release is not an acquire".into())),
        }
    }
    /// Write the release payload only after this channel's preceding GPU fence.
    pub fn release(&self, fence: Fence, memory: &mut Memory) -> Result<String> {
        require(self.operation == Operation::Release, "not a release")?;
        memory.read(self.address, 4)?;
        let diagnostics = fence.wait()?;
        memory.write(self.address, &self.payload.to_le_bytes())?;
        Ok(diagnostics)
    }
}
/// Runner-visible completion counter; the runner maps its guest syncpoint ID to this object.
#[derive(Default)]
pub struct Syncpoint {
    pub value: u32,
}
impl Syncpoint {
    pub fn complete(&mut self, fence: Fence) -> Result<String> {
        let diagnostics = fence.wait()?;
        self.value = self.value.wrapping_add(1);
        Ok(diagnostics)
    }
}
