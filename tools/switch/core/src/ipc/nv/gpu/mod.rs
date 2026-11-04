//! Bounded CPU execution of the supported virtual GPU profile.
mod effects;
mod masks;
mod mme;
mod packets;
mod state;
mod surface;
#[cfg(test)]
mod tests;
use crate::ipc::types::Error;
pub(crate) use packets::{decode_entry, decode_packets};
pub(crate) use state::ExecutionState;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Method {
    pub address: u64,
    pub subchannel: u32,
    pub method: u32,
    pub value: u32,
}
pub(crate) trait Access {
    fn check(&self, address: u64, size: usize, kind: u32, permission: u8) -> bool;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Action {
    Write {
        address: u64,
        bytes: Vec<u8>,
        kind: u32,
    },
    Copy {
        chunks: Vec<(u64, u64, usize)>,
    },
    Increment {
        syncpoint: u32,
        increment: u32,
        wait: bool,
    },
    Barrier,
}
fn invalid(message: &'static str) -> Error {
    Error::Malformed(message)
}
#[cfg(test)]
mod mme_tests;
#[cfg(test)]
mod surface_tests;
