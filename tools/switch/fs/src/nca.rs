//! NCA (Nintendo Content Archive), format version 3.
//!
//! The first 0xC00 bytes (header + four filesystem headers) are AES-128-XTS encrypted
//! with the player's `header_key`, 0x200-byte sectors, big-endian sector tweak.
//! Sections are AES-128-CTR encrypted with a key from the key area (decrypted with
//! `key_area_key_{application,ocean,system}_XX`) or, for titles with a rights id,
//! with the title key (decrypted with `titlekek_XX`).

use crate::crypto;
use crate::keys::KeySet;
use crate::pfs0::{self, Entry, Storage, Sub};
use crate::{u16le, u32le, u64le, Error, Result};

pub const HEADER_LEN: usize = 0xC00;
pub const MEDIA: u64 = 0x200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentType {
    Program,
    Meta,
    Control,
    Manual,
    Data,
    PublicData,
    Other(u8),
}

impl ContentType {
    fn from(v: u8) -> Self {
        match v {
            0 => Self::Program,
            1 => Self::Meta,
            2 => Self::Control,
            3 => Self::Manual,
            4 => Self::Data,
            5 => Self::PublicData,
            o => Self::Other(o),
        }
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Program => "program",
            Self::Meta => "meta",
            Self::Control => "control",
            Self::Manual => "manual",
            Self::Data => "data",
            Self::PublicData => "public-data",
            Self::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FsType {
    RomFs,
    PartitionFs,
}

#[derive(Debug, Clone)]
pub struct Section {
    pub index: usize,
    pub start: u64,
    pub end: u64,
    pub fs_type: FsType,
    pub encryption: u8,
    /// Upper 8 bytes of the CTR counter (already in counter byte order).
    pub ctr_upper: [u8; 8],
    /// Data region inside the section: PFS0 image or RomFS level-6 data.
    pub data_offset: u64,
    pub data_size: u64,
}

#[derive(Debug, Clone)]
pub struct Header {
    pub content_type: ContentType,
    pub content_size: u64,
    pub program_id: u64,
    pub key_generation: u8,
    pub key_area_index: u8,
    pub rights_id: [u8; 16],
    pub sections: Vec<Section>,
}

pub struct Nca<'a> {
    pub storage: &'a dyn Storage,
    pub header: Header,
    key: Option<[u8; 16]>,
}

/// Effective master key revision used to pick key_area_key_*_XX / titlekek_XX.
fn revision(old: u8, new: u8) -> u8 {
    old.max(new).saturating_sub(1)
}

pub fn decrypt_header(raw: &[u8], keys: &KeySet) -> Result<Vec<u8>> {
    if raw.len() < HEADER_LEN {
        return Err(Error::Truncated);
    }
    let mut h = raw[..HEADER_LEN].to_vec();
    crypto::nintendo_xts_decrypt(&keys.get32("header_key")?, &mut h, 0x200, 0);
    match &h[0x200..0x204] {
        b"NCA3" => Ok(h),
        b"NCA2" | b"NCA0" => Err(Error::Unsupported("NCA2/NCA0 archives (very early titles)")),
        _ => Err(Error::BadMagic("NCA (or header_key does not match)")),
    }
}

fn parse_header(h: &[u8]) -> Result<Header> {
    let mut sections = Vec::new();
    for i in 0..4 {
        let e = 0x240 + i * 0x10;
        let start = u32le(h, e)? as u64 * MEDIA;
        let end = u32le(h, e + 4)? as u64 * MEDIA;
        if end <= start {
            continue;
        }
        let fs = &h[0x400 + i * 0x200..0x600 + i * 0x200];
        let _version = u16le(fs, 0)?;
        let fs_type = match fs[2] {
            0 => FsType::RomFs,
            1 => FsType::PartitionFs,
            _ => return Err(Error::Format("unknown section filesystem type")),
        };
        let hash_type = fs[3];
        let encryption = fs[4];
        let mut ctr_upper = [0u8; 8];
        for j in 0..8 {
            ctr_upper[j] = fs[0x140 + 7 - j];
        }
        let (data_offset, data_size) = match (fs_type, hash_type) {
            // Hierarchical SHA-256: region 1 is the PFS0 image.
            (FsType::PartitionFs, 2) => (u64le(fs, 0x8 + 0x38)?, u64le(fs, 0x8 + 0x40)?),
            // IVFC: the last of the six levels holds the RomFS data.
            (FsType::RomFs, 3) => {
                if &fs[0x8..0xC] != b"IVFC" {
                    return Err(Error::BadMagic("IVFC"));
                }
                let level = 0x8 + 0x10 + 5 * 0x18;
                (u64le(fs, level)?, u64le(fs, level + 8)?)
            }
            _ => return Err(Error::Unsupported("this section hash layout")),
        };
        if data_offset.checked_add(data_size).ok_or(Error::Truncated)? > end - start {
            return Err(Error::Format("section data lies outside the section"));
        }
        sections.push(Section { index: i, start, end, fs_type, encryption, ctr_upper, data_offset, data_size });
    }
    Ok(Header {
        content_type: ContentType::from(h[0x205]),
        content_size: u64le(h, 0x208)?,
        program_id: u64le(h, 0x210)?,
        key_generation: revision(h[0x206], h[0x220]),
        key_area_index: h[0x207],
        rights_id: h[0x230..0x240].try_into().unwrap(),
        sections,
    })
}

impl<'a> Nca<'a> {
    pub fn open(storage: &'a dyn Storage, keys: &KeySet) -> Result<Self> {
        let raw = storage.read_vec(0, HEADER_LEN)?;
        let h = decrypt_header(&raw, keys)?;
        let header = parse_header(&h)?;
        if header.sections.iter().any(|s| s.end > storage.len()) {
            return Err(Error::Truncated);
        }
        let needs_key = header.sections.iter().any(|s| s.encryption == 3);
        let key = if !needs_key {
            None
        } else if header.rights_id != [0u8; 16] {
            let mut tk = keys.encrypted_title_key(&header.rights_id)?;
            crypto::ecb_decrypt(&keys.get16(&format!("titlekek_{:02x}", header.key_generation))?, &mut tk);
            Some(tk)
        } else {
            let kind = match header.key_area_index {
                0 => "application",
                1 => "ocean",
                2 => "system",
                _ => return Err(Error::Format("unknown key area index")),
            };
            let kak = keys.get16(&format!("key_area_key_{kind}_{:02x}", header.key_generation))?;
            let mut area = h[0x300..0x340].to_vec();
            crypto::ecb_decrypt(&kak, &mut area);
            Some(area[0x20..0x30].try_into().unwrap())
        };
        if header.sections.iter().any(|s| !matches!(s.encryption, 1 | 3)) {
            return Err(Error::Unsupported("update (BKTR) or XTS sections"));
        }
        Ok(Nca { storage, header, key })
    }

    /// Decrypted view of a section's data region.
    pub fn section(&self, index: usize) -> Result<SectionReader<'_>> {
        let s = self.header.sections.iter().find(|s| s.index == index).ok_or(Error::Format("no such section"))?;
        Ok(SectionReader { nca: self, section: s.clone() })
    }

    /// Files of a PartitionFs section (ExeFS of a program NCA, or a meta NCA's cnmt).
    pub fn pfs0_files(&self, index: usize) -> Result<Vec<Entry>> {
        let r = self.section(index)?;
        if r.section.fs_type != FsType::PartitionFs {
            return Err(Error::Format("section is not a PartitionFs"));
        }
        pfs0::parse(&r)
    }
}

pub struct SectionReader<'a> {
    nca: &'a Nca<'a>,
    pub section: Section,
}

impl Storage for SectionReader<'_> {
    fn len(&self) -> u64 {
        self.section.data_size
    }
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<()> {
        let end = offset.checked_add(buf.len() as u64).ok_or(Error::Truncated)?;
        if end > self.section.data_size {
            return Err(Error::Truncated);
        }
        // Absolute position inside the section, aligned for the CTR counter.
        let rel = self.section.data_offset + offset;
        let aligned = rel & !15;
        let lead = (rel - aligned) as usize;
        let mut tmp = vec![0u8; lead + buf.len()];
        let raw = Sub { inner: self.nca.storage, offset: self.section.start, size: self.section.end - self.section.start };
        raw.read_at(aligned, &mut tmp)?;
        if self.section.encryption == 3 {
            let key = self.nca.key.as_ref().ok_or(Error::Format("missing section key"))?;
            crypto::ctr_apply(key, self.section.ctr_upper, self.section.start + aligned, &mut tmp);
        }
        buf.copy_from_slice(&tmp[lead..]);
        Ok(())
    }
}

#[doc(hidden)]
pub mod fixture {
    //! Builds synthetic NCAs encrypted with made-up keys. No Nintendo data is used.
    use super::*;

    pub struct Made {
        pub image: Vec<u8>,
        pub keys: KeySet,
        /// The made-up keys as prod.keys / title.keys text, for command-line tests.
        pub prod_keys: String,
        pub title_keys: String,
    }

    pub fn program_nca(files: &[(&str, &[u8])], title_key: bool) -> Made {
        let header_key = [0x5au8; 32];
        let kak = [0x3cu8; 16];
        let section_key = [0x77u8; 16];
        let titlekek = [0x19u8; 16];
        let generation = 3u8; // stored revision 3 -> keys suffixed _02
        let mut keys = KeySet::default();
        keys.insert("header_key", &header_key);
        keys.insert("key_area_key_application_02", &kak);
        keys.insert("titlekek_02", &titlekek);

        let pfs = pfs0::build(files);
        let section_start = 0xC00u64; // media 6
        let data_offset = 0x200u64; // a fake hash table precedes the PFS0
        let mut section = vec![0u8; data_offset as usize];
        section.extend_from_slice(&pfs);
        while section.len() % MEDIA as usize != 0 {
            section.push(0);
        }
        let section_end = section_start + section.len() as u64;

        let mut h = vec![0u8; HEADER_LEN];
        h[0x200..0x204].copy_from_slice(b"NCA3");
        h[0x204] = 0; // system distribution
        h[0x205] = 0; // program
        h[0x206] = 0;
        h[0x207] = 0; // application key area
        h[0x208..0x210].copy_from_slice(&section_end.to_le_bytes());
        h[0x210..0x218].copy_from_slice(&0x0100_0000_0001_0000u64.to_le_bytes());
        h[0x220] = generation;
        if title_key {
            h[0x230] = 0x01;
            h[0x23f] = 0x02;
        }
        h[0x240..0x244].copy_from_slice(&((section_start / MEDIA) as u32).to_le_bytes());
        h[0x244..0x248].copy_from_slice(&((section_end / MEDIA) as u32).to_le_bytes());
        h[0x248] = 1;
        let fs = 0x400;
        h[fs..fs + 2].copy_from_slice(&2u16.to_le_bytes());
        h[fs + 2] = 1; // PartitionFs
        h[fs + 3] = 2; // HierarchicalSha256
        h[fs + 4] = 3; // AES-CTR
        h[fs + 0x8 + 0x38..fs + 0x8 + 0x40].copy_from_slice(&data_offset.to_le_bytes());
        h[fs + 0x8 + 0x40..fs + 0x8 + 0x48].copy_from_slice(&(pfs.len() as u64).to_le_bytes());
        let stored_ctr = [1u8, 2, 3, 4, 5, 6, 7, 8];
        h[fs + 0x140..fs + 0x148].copy_from_slice(&stored_ctr);
        let mut upper = stored_ctr;
        upper.reverse();

        if title_key {
            let mut rid = [0u8; 16];
            rid[0] = 1;
            rid[15] = 2;
            let mut enc = section_key;
            crypto::ecb_encrypt(&titlekek, &mut enc);
            keys.insert_title(rid, enc);
        } else {
            let mut area = [0u8; 0x40];
            area[0x20..0x30].copy_from_slice(&section_key);
            crypto::ecb_encrypt(&kak, &mut area);
            h[0x300..0x340].copy_from_slice(&area);
        }

        crypto::ctr_apply(&section_key, upper, section_start, &mut section);
        crypto::nintendo_xts_encrypt(&header_key, &mut h, 0x200, 0);
        let mut image = h;
        image.extend_from_slice(&section);
        let hx = |b: &[u8]| b.iter().map(|x| format!("{x:02x}")).collect::<String>();
        let prod_keys = format!(
            "header_key = {}\nkey_area_key_application_02 = {}\ntitlekek_02 = {}\n",
            hx(&header_key), hx(&kak), hx(&titlekek)
        );
        let title_keys = if title_key {
            let mut enc = section_key;
            crypto::ecb_encrypt(&titlekek, &mut enc);
            let mut rid = [0u8; 16];
            rid[0] = 1;
            rid[15] = 2;
            format!("{} = {}\n", hx(&rid), hx(&enc))
        } else {
            String::new()
        };
        Made { image, keys, prod_keys, title_keys }
    }
}

#[cfg(test)]
mod tests {
    use super::fixture::program_nca;
    use super::*;

    #[test]
    fn reads_exefs_from_key_area_nca() {
        let m = program_nca(&[("main", b"\x7fcode bytes"), ("main.npdm", b"META....")], false);
        let nca = Nca::open(&m.image, &m.keys).unwrap();
        assert_eq!(nca.header.content_type, ContentType::Program);
        assert_eq!(nca.header.program_id, 0x0100_0000_0001_0000);
        assert_eq!(nca.header.key_generation, 2);
        let files = nca.pfs0_files(0).unwrap();
        assert_eq!(files.iter().map(|f| f.name.as_str()).collect::<Vec<_>>(), ["main", "main.npdm"]);
        let sec = nca.section(0).unwrap();
        assert_eq!(sec.read_vec(files[1].offset, 4).unwrap(), b"META");
        // Unaligned read in the middle of a CTR block.
        assert_eq!(sec.read_vec(files[0].offset + 3, 4).unwrap(), b"de b");
    }

    #[test]
    fn reads_title_key_nca() {
        let m = program_nca(&[("main", b"hello")], true);
        let nca = Nca::open(&m.image, &m.keys).unwrap();
        let files = nca.pfs0_files(0).unwrap();
        assert_eq!(nca.section(0).unwrap().read_vec(files[0].offset, 5).unwrap(), b"hello");
    }

    #[test]
    fn wrong_or_missing_keys_are_reported_by_name() {
        let m = program_nca(&[("main", b"x")], false);
        let mut wrong = KeySet::default();
        wrong.insert("header_key", &[1u8; 32]);
        assert!(matches!(Nca::open(&m.image, &wrong), Err(Error::BadMagic(_))));
        let mut no_kak = KeySet::default();
        no_kak.insert("header_key", &[0x5au8; 32]);
        let e = Nca::open(&m.image, &no_kak).err().unwrap().to_string();
        assert!(e.contains("key_area_key_application_02"), "{e}");
        let t = program_nca(&[("main", b"x")], true);
        let mut no_title = KeySet::default();
        no_title.insert("header_key", &[0x5au8; 32]);
        no_title.insert("titlekek_02", &[0x19u8; 16]);
        assert!(Nca::open(&t.image, &no_title).err().unwrap().to_string().contains("rights id"));
    }

    #[test]
    fn truncated_nca_is_rejected() {
        let m = program_nca(&[("main", b"x")], false);
        let cut = m.image.len() - 0x200;
        assert!(matches!(Nca::open(&m.image[..cut].to_vec(), &m.keys), Err(Error::Truncated)));
        assert!(matches!(Nca::open(&m.image[..0x800].to_vec(), &m.keys), Err(Error::Truncated)));
    }
}
