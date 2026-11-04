//! Stable source-cache framing. This stores code, never compiled device binaries.
use crate::{Error, Result};

pub const VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"SOJUMSL\0";
const MAX_CODE: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key(pub [u8; 16]);

/// Exact inputs to compilation, including the caller's canonical render-state bytes.
/// Include resource layouts, compiler flags and target features in `render_state`.
/// `program` contains scheduling words; `header` is the exact SPH or compute QMD.
#[derive(Debug, Clone, Copy)]
pub struct Inputs<'a> {
    pub program: &'a [u8],
    pub header: &'a [u8],
    pub render_state: &'a [u8],
}
impl Key {
    /// Defined, non-cryptographic 128-bit hash. No platform-dependent Rust Hasher.
    /// Length framing prevents ambiguity between adjacent fields.
    pub fn new(inputs: Inputs<'_>) -> Self {
        let mut h = 0x6c62272e07bb014262b821756295c58du128;
        for field in [
            VERSION.to_le_bytes().as_slice(),
            inputs.program,
            inputs.header,
            inputs.render_state,
        ] {
            for b in (field.len() as u64).to_le_bytes().iter().chain(field) {
                h ^= u128::from(*b);
                h = h.wrapping_mul(0x0000000001000000000000000000013b);
            }
        }
        Self(h.to_le_bytes())
    }
}

/// Portable disk representation: magic, version, key, byte length, payload digest, UTF-8 MSL.
/// The caller owns paths and atomic publication; decoding never executes cached code.
pub fn encode(key: Key, source: &str) -> Result<Vec<u8>> {
    if source.len() > MAX_CODE {
        return Err(Error("cache code exceeds 16 MiB".into()));
    }
    let digest = Key::new(Inputs {
        program: source.as_bytes(),
        header: &[],
        render_state: &[],
    });
    let mut bytes = Vec::with_capacity(52 + source.len());
    bytes.extend(MAGIC);
    bytes.extend(VERSION.to_le_bytes());
    bytes.extend(key.0);
    bytes.extend((source.len() as u64).to_le_bytes());
    bytes.extend(digest.0);
    bytes.extend(source.as_bytes());
    Ok(bytes)
}

pub fn decode(bytes: &[u8], expected: Key) -> Result<&str> {
    if bytes.len() < 52 || bytes.get(..8) != Some(MAGIC.as_slice()) {
        return Err(Error("invalid cache magic or truncated header".into()));
    }
    if bytes[8..12] != VERSION.to_le_bytes() {
        return Err(Error("unsupported cache version".into()));
    }
    if bytes[12..28] != expected.0 {
        return Err(Error("cache key mismatch".into()));
    }
    let length = u64::from_le_bytes(
        bytes[28..36]
            .try_into()
            .map_err(|_| Error("cache length".into()))?,
    );
    if length > MAX_CODE as u64 || length != (bytes.len() - 52) as u64 {
        return Err(Error("cache length mismatch or oversized code".into()));
    }
    let code = &bytes[52..];
    let digest = Key::new(Inputs {
        program: code,
        header: &[],
        render_state: &[],
    });
    if bytes[36..52] != digest.0 {
        return Err(Error("cache payload checksum mismatch".into()));
    }
    std::str::from_utf8(code).map_err(|_| Error("cache source is not UTF-8".into()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_and_each_compilation_input_affect_key() {
        let key = |p, h, s| {
            Key::new(Inputs {
                program: p,
                header: h,
                render_state: s,
            })
        };
        let base = key(b"ab", b"c", b"d");
        assert_ne!(base, key(b"a", b"bc", b"d"));
        assert_ne!(base, key(b"Ab", b"c", b"d"));
        assert_ne!(base, key(b"ab", b"C", b"d"));
        assert_ne!(base, key(b"ab", b"c", b"D"));
        assert_eq!(base, key(b"ab", b"c", b"d"));
    }
    #[test]
    fn cache_rejects_corruption_truncation_wrong_key_and_version() {
        let key = Key([17; 16]);
        let bytes = encode(key, "kernel void test() {}\n").unwrap();
        assert_eq!(decode(&bytes, key).unwrap(), "kernel void test() {}\n");
        for end in 0..bytes.len() {
            assert!(decode(&bytes[..end], key).is_err());
        }
        for index in 0..bytes.len() {
            let mut corrupted = bytes.clone();
            corrupted[index] ^= 1;
            assert!(decode(&corrupted, key).is_err(), "byte {index}");
        }
        assert!(decode(&bytes, Key([18; 16])).is_err());
        let mut trailing = bytes.clone();
        trailing.push(0);
        assert!(decode(&trailing, key).is_err());
    }
}

mod state;
