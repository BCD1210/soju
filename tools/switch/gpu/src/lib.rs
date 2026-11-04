//! B197 method state and headless Metal rendering. Public provenance: SOURCES.md.
pub mod engine;
pub mod memory;
pub mod registers;
pub mod texture;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error(pub String);
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Error {}
pub type Result<T> = std::result::Result<T, Error>;
pub(crate) fn require(ok: bool, message: &str) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error(message.into()))
    }
}
pub mod backend;

pub mod format;
pub mod sync;
pub mod worker;

pub mod compute;

mod protocol;

mod lru;

pub mod profile;
