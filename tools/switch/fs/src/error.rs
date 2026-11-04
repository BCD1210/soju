use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Truncated,
    Format(&'static str),
    BadMagic(&'static str),
    /// A key the file needs is not in the player's key files. Only the key *name* is kept.
    MissingKey(String),
    Unsupported(&'static str),
}

pub type Result<T> = std::result::Result<T, Error>;

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(e) => write!(f, "cannot read the file: {e}"),
            Error::Truncated => write!(f, "the file is shorter than its header says"),
            Error::Format(m) => write!(f, "unexpected file layout: {m}"),
            Error::BadMagic(m) => write!(f, "this is not a {m} file"),
            Error::MissingKey(k) => write!(f, "your key file has no '{k}' (dump keys again from your own Switch)"),
            Error::Unsupported(m) => write!(f, "not supported yet: {m}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Error::Io(e)
    }
}
