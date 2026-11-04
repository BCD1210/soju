//! Tickets (`.tik` files that ship inside NSPs). A ticket starts with a signature block
//! whose size depends on its type, followed by the ticket data: issuer (0x40), title key
//! block (0x100), format version, title key type (0 = common, 1 = personalized), ...,
//! master key revision at +0x145 and the rights id at +0x160.
//!
//! A common ticket carries the encrypted title key in the first 16 bytes of the block, so
//! an NSP with its ticket opens with the player's prod.keys alone. Personalized tickets
//! are encrypted to one console and are reported as such.

use crate::{u32le, Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub rights_id: [u8; 16],
    pub master_key_revision: u8,
    pub common: bool,
    /// Encrypted title key (common tickets only).
    pub title_key: Option<[u8; 16]>,
}

/// Size of the signature block (signature + padding) for each signature type.
fn signature_block(kind: u32) -> Result<usize> {
    Ok(match kind {
        0x10000 | 0x10003 => 0x200 + 0x3C, // RSA-4096
        0x10001 | 0x10004 => 0x100 + 0x3C, // RSA-2048
        0x10002 | 0x10005 => 0x3C + 0x40,  // ECDSA
        _ => return Err(Error::Format("unknown ticket signature type")),
    })
}

pub fn parse(b: &[u8]) -> Result<Ticket> {
    let data = 4 + signature_block(u32le(b, 0)?)?;
    let t = b.get(data..data + 0x180).ok_or(Error::Truncated)?;
    let common = match t[0x141] {
        0 => true,
        1 => false,
        _ => return Err(Error::Format("unknown title key type in ticket")),
    };
    Ok(Ticket {
        rights_id: t[0x160..0x170].try_into().unwrap(),
        master_key_revision: t[0x145],
        common,
        title_key: if common { Some(t[0x40..0x50].try_into().unwrap()) } else { None },
    })
}

/// Build a common RSA-2048 ticket for tests.
pub fn build(rights_id: [u8; 16], encrypted_title_key: [u8; 16], personalized: bool) -> Vec<u8> {
    let mut b = vec![0u8; 4 + 0x13C + 0x180];
    b[..4].copy_from_slice(&0x10004u32.to_le_bytes());
    let d = 4 + 0x13C;
    b[d..d + 6].copy_from_slice(b"Root-C");
    b[d + 0x40..d + 0x50].copy_from_slice(&encrypted_title_key);
    b[d + 0x140] = 2;
    b[d + 0x141] = personalized as u8;
    b[d + 0x145] = rights_id[15];
    b[d + 0x160..d + 0x170].copy_from_slice(&rights_id);
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn common_and_personalized() {
        let mut rid = [0u8; 16];
        rid[0] = 1;
        rid[15] = 2;
        let t = parse(&build(rid, [9; 16], false)).unwrap();
        assert!(t.common);
        assert_eq!(t.rights_id, rid);
        assert_eq!(t.title_key, Some([9; 16]));
        assert_eq!(t.master_key_revision, 2);
        let p = parse(&build(rid, [9; 16], true)).unwrap();
        assert!(!p.common && p.title_key.is_none());
    }

    #[test]
    fn rejects_unknown_signature_and_short_files() {
        let mut b = build([0; 16], [0; 16], false);
        b[0] = 0x77;
        assert!(parse(&b).is_err());
        assert!(matches!(parse(&build([0; 16], [0; 16], false)[..0x200]), Err(Error::Truncated)));
    }
}
