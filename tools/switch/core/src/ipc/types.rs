//! Normalized IPC values shared by service implementations and wire codecs.
use crate::kernel::{Handle, Kernel, ObjectId};
use crate::memory::GuestMemory;
use std::fmt;

pub const MAX_BUFFER: usize = 64 * 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Sm,
    Platform,
    Filesystem,
    Nv,
    Media,
}
impl Family {
    pub const fn code(self) -> u64 {
        match self {
            Self::Sm => 0,
            Self::Platform => 1,
            Self::Filesystem => 2,
            Self::Nv => 3,
            Self::Media => 4,
        }
    }
    pub fn decode(code: u64) -> Result<Self, Error> {
        match code {
            0 => Ok(Self::Sm),
            1 => Ok(Self::Platform),
            2 => Ok(Self::Filesystem),
            3 => Ok(Self::Nv),
            4 => Ok(Self::Media),
            _ => Err(Error::Malformed("service family")),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Cmif,
    Tipc,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageKind {
    Request,
    Control,
    Close,
    DomainClose,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BufferKind {
    Pointer,
    Send,
    Receive,
    Exchange,
    ReceiveList,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Buffer {
    pub kind: BufferKind,
    pub address: u64,
    pub len: u64,
    pub index: u16,
}
#[derive(Clone, Debug)]
pub struct Request {
    pub protocol: Protocol,
    pub kind: MessageKind,
    pub command_id: u32,
    pub pid: Option<u64>,
    pub buffers: Vec<Buffer>,
    pub data: Vec<u8>,
    pub copy_handles: Vec<Handle>,
    pub move_handles: Vec<Handle>,
    pub domain_objects: Vec<u32>,
    /// Stable identities resolved by the router from domain IDs and input handles.
    pub objects: Vec<ObjectId>,
    pub domain_target: Option<u32>,
    pub token: u32,
}
impl Request {
    pub fn u32(&self, offset: usize) -> Result<u32, Error> {
        read_u32(&self.data, offset)
    }
    pub fn u64(&self, offset: usize) -> Result<u64, Error> {
        read_u64(&self.data, offset)
    }
    pub fn buffer(&self, kind: BufferKind, index: usize) -> Result<Buffer, Error> {
        self.buffers
            .iter()
            .filter(|b| b.kind == kind)
            .nth(index)
            .copied()
            .ok_or(Error::Malformed("missing IPC buffer"))
    }
    pub fn input(&self) -> Result<Buffer, Error> {
        self.buffers
            .iter()
            .find(|b| b.len != 0 && matches!(b.kind, BufferKind::Send | BufferKind::Exchange))
            .or_else(|| {
                self.buffers
                    .iter()
                    .find(|b| b.len != 0 && b.kind == BufferKind::Pointer)
            })
            .or_else(|| {
                self.buffers.iter().find(|b| {
                    matches!(
                        b.kind,
                        BufferKind::Send | BufferKind::Exchange | BufferKind::Pointer
                    )
                })
            })
            .copied()
            .ok_or(Error::Malformed("missing input buffer"))
    }
    pub fn output(&self) -> Result<Buffer, Error> {
        self.buffers
            .iter()
            .find(|b| b.len != 0 && matches!(b.kind, BufferKind::Receive | BufferKind::Exchange))
            .or_else(|| {
                self.buffers
                    .iter()
                    .find(|b| b.len != 0 && b.kind == BufferKind::ReceiveList)
            })
            .or_else(|| {
                self.buffers.iter().find(|b| {
                    matches!(
                        b.kind,
                        BufferKind::Receive | BufferKind::Exchange | BufferKind::ReceiveList
                    )
                })
            })
            .copied()
            .ok_or(Error::Malformed("missing output buffer"))
    }
    pub fn count(&self, kind: BufferKind) -> usize {
        self.buffers.iter().filter(|b| b.kind == kind).count()
    }
}
#[derive(Clone, Debug)]
pub struct BufferWrite {
    pub buffer_index: u16,
    pub offset: u64,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct MemoryWrite {
    pub address: u64,
    pub bytes: Vec<u8>,
}
#[derive(Debug, Default)]
pub struct Reply {
    pub result: u32,
    pub data: Vec<u8>,
    /// Original event/shared-memory handles: the router duplicates these for the guest.
    pub copy_handles: Vec<Handle>,
    pub move_handles: Vec<Handle>,
    /// Newly registered session handles, converted to domain objects when needed.
    pub objects: Vec<Handle>,
    pub writes: Vec<BufferWrite>,
    /// Staged GPU effects committed atomically with descriptor writes and the reply.
    pub memory_writes: Vec<MemoryWrite>,
    pub transaction: Option<u64>,
}
impl Reply {
    pub fn success() -> Self {
        Self::default()
    }
    pub fn result(result: u32) -> Self {
        Self {
            result,
            ..Self::default()
        }
    }
    pub fn data(data: Vec<u8>) -> Self {
        Self {
            data,
            ..Self::default()
        }
    }
    pub fn object(handle: Handle) -> Self {
        Self {
            objects: vec![handle],
            ..Self::default()
        }
    }
    pub fn copy(handle: Handle) -> Self {
        Self {
            copy_handles: vec![handle],
            ..Self::default()
        }
    }
    pub fn write(mut self, buffer: Buffer, bytes: Vec<u8>) -> Result<Self, Error> {
        if u64::try_from(bytes.len()).map_err(|_| Error::Malformed("output size"))? > buffer.len {
            return Err(Error::Malformed("output buffer too small"));
        }
        self.writes.push(BufferWrite {
            buffer_index: buffer.index,
            offset: 0,
            bytes,
        });
        Ok(self)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClientId(pub u64);
#[derive(Clone, Copy, Debug)]
pub struct NvBufferView {
    pub allocation: u32,
    pub generation: u64,
    pub guest_address: u64,
    pub size: u64,
    pub alignment: u64,
    pub kind: u32,
}
pub trait NvBufferLookup {
    fn resolve(&self, client: ClientId, id: u32) -> Result<NvBufferView, Error>;
}
pub struct Context<'a> {
    pub kernel: &'a mut Kernel,
    pub memory: &'a mut dyn GuestMemory,
    pub object: ObjectId,
    pub session: Handle,
    pub client: ClientId,
    pub now_ns: u64,
    pub tls: u64,
    pub nv: Option<&'a dyn NvBufferLookup>,
}
impl Context<'_> {
    pub fn new_session(&mut self, family: Family) -> Result<(Handle, ObjectId), Error> {
        let handle = self
            .kernel
            .register_session(family.code())
            .map_err(Error::Kernel)?;
        Ok((
            handle,
            self.kernel.object_id(handle).map_err(Error::Kernel)?,
        ))
    }
    pub fn read_buffer(&self, buffer: Buffer, limit: usize) -> Result<Vec<u8>, Error> {
        let len = usize::try_from(buffer.len).map_err(|_| Error::Malformed("buffer size"))?;
        if len > limit || len > MAX_BUFFER {
            return Err(Error::Malformed("buffer limit"));
        }
        let mut bytes = vec![0; len];
        self.memory.read(buffer.address, &mut bytes)?;
        Ok(bytes)
    }
}
#[derive(Clone, Copy)]
pub struct Schema {
    pub min_data: usize,
    pub max_data: usize,
}
impl Schema {
    pub const ANY: Self = Self {
        min_data: 0,
        max_data: 256,
    };
    pub const fn at_least(n: usize) -> Self {
        Self {
            min_data: n,
            max_data: 256,
        }
    }
}
pub struct CommandSpec<S: 'static> {
    pub id: u32,
    pub schema: Schema,
    pub handler: fn(&mut S, &Request, &mut Context<'_>) -> Result<Reply, Error>,
}
pub fn dispatch_table<S>(
    state: &mut S,
    table: &[CommandSpec<S>],
    request: &Request,
    context: &mut Context<'_>,
) -> Result<Reply, Error> {
    let command = table
        .iter()
        .find(|c| c.id == request.command_id)
        .ok_or(Error::Unsupported {
            object: context.object,
            command: request.command_id,
        })?;
    if request.data.len() < command.schema.min_data || request.data.len() > command.schema.max_data
    {
        return Err(Error::Malformed("command payload size"));
    }
    (command.handler)(state, request, context)
}
pub fn read_u32(bytes: &[u8], offset: usize) -> Result<u32, Error> {
    let end = offset
        .checked_add(4)
        .ok_or(Error::Malformed("integer offset"))?;
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(Error::Malformed("truncated u32"))?
            .try_into()
            .map_err(|_| Error::Malformed("u32"))?,
    ))
}
pub fn read_u64(bytes: &[u8], offset: usize) -> Result<u64, Error> {
    let end = offset
        .checked_add(8)
        .ok_or(Error::Malformed("integer offset"))?;
    Ok(u64::from_le_bytes(
        bytes
            .get(offset..end)
            .ok_or(Error::Malformed("truncated u64"))?
            .try_into()
            .map_err(|_| Error::Malformed("u64"))?,
    ))
}
#[derive(Debug)]
pub enum Error {
    Malformed(&'static str),
    Unsupported { object: ObjectId, command: u32 },
    Kernel(u64),
    Memory(crate::memory::Error),
    Io(std::io::Error),
    Message(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "IPC: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<crate::memory::Error> for Error {
    fn from(value: crate::memory::Error) -> Self {
        Self::Memory(value)
    }
}
impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
