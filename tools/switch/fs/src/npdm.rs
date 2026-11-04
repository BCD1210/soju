//! NPDM (`main.npdm`): program metadata. "META" header (0x80) with the 64-bit flag,
//! main thread priority/core/stack size, system resource size, name, and offsets of
//! the ACI0 block (program id, FS/service/kernel capability regions).

use crate::{u32le, u64le, Error, Result};

#[derive(Debug, Clone)]
pub struct Npdm {
    pub name: String,
    pub is_64bit: bool,
    pub address_space: u8,
    pub main_thread_priority: u8,
    pub main_thread_core: u8,
    pub main_thread_stack_size: u32,
    pub system_resource_size: u32,
    pub version: u32,
    pub program_id: u64,
    /// Kernel capability descriptors (raw u32 words) from ACI0.
    pub kernel_caps: Vec<u32>,
    /// Service access control: (name, is_server).
    pub services: Vec<(String, bool)>,
}

fn region(b: &[u8], off: u32, size: u32) -> Result<&[u8]> {
    let s = off as usize;
    b.get(s..s.checked_add(size as usize).ok_or(Error::Truncated)?).ok_or(Error::Truncated)
}

pub fn parse(b: &[u8]) -> Result<Npdm> {
    if b.len() < 0x80 || &b[..4] != b"META" {
        return Err(Error::BadMagic("NPDM"));
    }
    let flags = b[0xC];
    let name_raw = &b[0x20..0x30];
    let name_end = name_raw.iter().position(|&c| c == 0).unwrap_or(16);
    let name = String::from_utf8_lossy(&name_raw[..name_end]).into_owned();
    let aci = region(b, u32le(b, 0x70)?, u32le(b, 0x74)?)?;
    if aci.len() < 0x40 || &aci[..4] != b"ACI0" {
        return Err(Error::BadMagic("ACI0"));
    }
    let sac = region(aci, u32le(aci, 0x28)?, u32le(aci, 0x2C)?)?;
    let mut services = Vec::new();
    let mut i = 0;
    while i < sac.len() {
        let ctl = sac[i];
        let len = (ctl & 7) as usize + 1;
        let name = sac.get(i + 1..i + 1 + len).ok_or(Error::Truncated)?;
        services.push((String::from_utf8_lossy(name).into_owned(), ctl & 0x80 != 0));
        i += 1 + len;
    }
    let kc = region(aci, u32le(aci, 0x30)?, u32le(aci, 0x34)?)?;
    let kernel_caps = kc.chunks_exact(4).map(|w| u32::from_le_bytes(w.try_into().unwrap())).collect();
    Ok(Npdm {
        name,
        is_64bit: flags & 1 != 0,
        address_space: (flags >> 1) & 7,
        main_thread_priority: b[0xE],
        main_thread_core: b[0xF],
        system_resource_size: u32le(b, 0x14)?,
        version: u32le(b, 0x18)?,
        main_thread_stack_size: u32le(b, 0x1C)?,
        program_id: u64le(aci, 0x10)?,
        kernel_caps,
        services,
    })
}

/// Build an NPDM for tests.
pub fn build(name: &str, program_id: u64, services: &[&str], caps: &[u32]) -> Vec<u8> {
    let mut sac = Vec::new();
    for s in services {
        sac.push((s.len() - 1) as u8);
        sac.extend_from_slice(s.as_bytes());
    }
    let kc: Vec<u8> = caps.iter().flat_map(|c| c.to_le_bytes()).collect();
    let mut aci = vec![0u8; 0x40];
    aci[..4].copy_from_slice(b"ACI0");
    aci[0x10..0x18].copy_from_slice(&program_id.to_le_bytes());
    aci[0x28..0x2C].copy_from_slice(&0x40u32.to_le_bytes());
    aci[0x2C..0x30].copy_from_slice(&(sac.len() as u32).to_le_bytes());
    aci[0x30..0x34].copy_from_slice(&(0x40 + sac.len() as u32).to_le_bytes());
    aci[0x34..0x38].copy_from_slice(&(kc.len() as u32).to_le_bytes());
    aci.extend_from_slice(&sac);
    aci.extend_from_slice(&kc);
    let mut m = vec![0u8; 0x80];
    m[..4].copy_from_slice(b"META");
    m[0xC] = 1 | (2 << 1);
    m[0xE] = 44;
    m[0xF] = 0;
    m[0x1C..0x20].copy_from_slice(&0x100000u32.to_le_bytes());
    m[0x20..0x20 + name.len()].copy_from_slice(name.as_bytes());
    m[0x70..0x74].copy_from_slice(&0x80u32.to_le_bytes());
    m[0x74..0x78].copy_from_slice(&(aci.len() as u32).to_le_bytes());
    m.extend_from_slice(&aci);
    m
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let n = parse(&build("Application", 0x0100_0000_0001_0000, &["fsp-srv", "hid", "vi:u"], &[0x3, 0x0000_63FF])).unwrap();
        assert_eq!(n.name, "Application");
        assert!(n.is_64bit);
        assert_eq!(n.address_space, 2);
        assert_eq!(n.main_thread_priority, 44);
        assert_eq!(n.main_thread_stack_size, 0x100000);
        assert_eq!(n.program_id, 0x0100_0000_0001_0000);
        assert_eq!(n.services.iter().map(|s| s.0.as_str()).collect::<Vec<_>>(), ["fsp-srv", "hid", "vi:u"]);
        assert_eq!(n.kernel_caps, [0x3, 0x63FF]);
    }

    #[test]
    fn rejects_bad_offsets() {
        let mut b = build("x", 1, &[], &[]);
        b[0x74..0x78].copy_from_slice(&0xFFFFu32.to_le_bytes());
        assert!(matches!(parse(&b), Err(Error::Truncated)));
        assert!(matches!(parse(&[0u8; 0x80]), Err(Error::BadMagic(_))));
    }
}
