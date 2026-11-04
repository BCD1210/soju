//! What game is in an NSP/XCI: title id, kind (application/update/add-on), version,
//! name and publisher from control.nacp, and the icon JPEG. Used by the launcher.

use crate::cnmt::{self, Cnmt};
use crate::keys::KeySet;
use crate::nacp::{self, Nacp};
use crate::nca::{ContentType, FsType, Nca};
use crate::pfs0::{self, Entry, Storage, Sub};
use crate::romfs;
use crate::ticket;
use crate::xci::Xci;
use crate::{Error, Result};

pub const PREFERRED: [&str; 4] = ["Korean", "AmericanEnglish", "BritishEnglish", "Japanese"];
const MAX_SMALL_FILE: u64 = 4 << 20;

#[derive(Debug, Clone)]
pub struct TitleInfo {
    pub title_id: Option<u64>,
    pub kind: Option<&'static str>,
    pub version: Option<u32>,
    pub nacp: Option<Nacp>,
    pub icon: Option<Vec<u8>>,
    pub nca_count: usize,
    /// Per-NCA problems (e.g. a missing key) by file name, without key values.
    pub problems: Vec<(String, String)>,
}

/// NCA entries of a container, by file extension ("nsp" or "xci").
pub fn nca_entries(s: &dyn Storage, ext: &str) -> Result<Vec<Entry>> {
    Ok(container_entries(s, ext)?.into_iter().filter(|e| e.name.ends_with(".nca")).collect())
}

/// The player's keys plus title keys from the container's own common tickets (.tik).
/// Personalized tickets cannot be used without the console they belong to.
pub fn keys_with_tickets(s: &dyn Storage, ext: &str, keys: &KeySet) -> Result<(KeySet, Vec<(String, String)>)> {
    let mut ks = keys.clone();
    let mut problems = Vec::new();
    for e in container_entries(s, ext)?.iter().filter(|e| e.name.ends_with(".tik")) {
        match small(s, e.offset, e.size).and_then(|b| ticket::parse(&b)) {
            Ok(t) => match t.title_key {
                Some(k) => ks.insert_title(t.rights_id, k),
                None => problems.push((e.name.clone(), "personalized ticket: this game must be dumped with its title key from your own Switch".into())),
            },
            Err(err) => problems.push((e.name.clone(), err.to_string())),
        }
    }
    Ok((ks, problems))
}

fn container_entries(s: &dyn Storage, ext: &str) -> Result<Vec<Entry>> {
    let all = match ext {
        "nsp" | "nsz" => pfs0::parse(s)?,
        "xci" | "xcz" => Xci::open(s)?.partition("secure")?.into_iter().map(|h| h.entry).collect(),
        _ => return Err(Error::Unsupported("only .nsp and .xci game files")),
    };
    if all.iter().any(|e| e.name.ends_with(".ncz")) {
        return Err(Error::Unsupported("compressed NSZ/XCZ files; decompress them first"));
    }
    Ok(all)
}

fn small(s: &dyn Storage, offset: u64, size: u64) -> Result<Vec<u8>> {
    if size > MAX_SMALL_FILE {
        return Err(Error::Format("metadata file is too large"));
    }
    s.read_vec(offset, size as usize)
}

pub fn read(s: &dyn Storage, ext: &str, keys: &KeySet) -> Result<TitleInfo> {
    let entries = nca_entries(s, ext)?;
    let (keys, ticket_problems) = keys_with_tickets(s, ext, keys)?;
    let keys = &keys;
    let mut info = TitleInfo { title_id: None, kind: None, version: None, nacp: None, icon: None, nca_count: entries.len(), problems: ticket_problems };
    let mut meta: Option<Cnmt> = None;
    let mut program_id = None;
    for e in &entries {
        let sub = Sub { inner: s, offset: e.offset, size: e.size };
        let nca = match Nca::open(&sub, keys) {
            Ok(n) => n,
            Err(err) => {
                info.problems.push((e.name.clone(), err.to_string()));
                continue;
            }
        };
        let mut step = || -> Result<()> {
            match nca.header.content_type {
                ContentType::Meta if meta.is_none() => {
                    let files = nca.pfs0_files(0)?;
                    if let Some(f) = files.iter().find(|f| f.name.ends_with(".cnmt")) {
                        meta = Some(cnmt::parse(&small(&nca.section(0)?, f.offset, f.size)?)?);
                    }
                }
                ContentType::Program if program_id.is_none() => program_id = Some(nca.header.program_id),
                ContentType::Control if info.nacp.is_none() => {
                    let sec = nca.header.sections.iter().find(|x| x.fs_type == FsType::RomFs).ok_or(Error::Format("control NCA has no RomFS"))?;
                    let r = nca.section(sec.index)?;
                    let files = romfs::list(&r)?;
                    if let Some(f) = files.iter().find(|f| f.path == "/control.nacp") {
                        let n = nacp::parse(&small(&r, f.offset, f.size)?)?;
                        let icon_lang = PREFERRED.iter().find(|l| n.titles.iter().any(|t| t.0 == **l)).copied().or(n.titles.first().map(|t| t.0));
                        if let Some(lang) = icon_lang {
                            let want = format!("/icon_{lang}.dat");
                            if let Some(icon) = files.iter().find(|f| f.path == want).or_else(|| files.iter().find(|f| f.path.starts_with("/icon_"))) {
                                info.icon = Some(small(&r, icon.offset, icon.size)?);
                            }
                        }
                        info.nacp = Some(n);
                    }
                }
                _ => {}
            }
            Ok(())
        };
        if let Err(err) = step() {
            info.problems.push((e.name.clone(), err.to_string()));
        }
    }
    if let Some(m) = &meta {
        info.title_id = Some(m.title_id);
        info.kind = Some(m.meta_type.name());
        info.version = Some(m.version);
    } else {
        info.title_id = program_id;
    }
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nca::fixture::{control_nca, meta_nca, program_nca};

    fn package(ext: &str) -> (Vec<u8>, KeySet) {
        let nacp = nacp::build(&[(0, "Kart Racer", "Example Co"), (12, "카트 레이서", "예시 회사")], "1.0.0");
        let ctrl = control_nca(&romfs::build(&[("/control.nacp", &nacp), ("/icon_Korean.dat", b"\xff\xd8KOJPEG"), ("/icon_AmericanEnglish.dat", b"\xff\xd8ENJPEG")]));
        let prog = program_nca(&[("main", b"code")], false);
        let cn = cnmt::build(0x0100_0000_0001_0000, 0, 0x80, None, &[([1; 16], prog.image.len() as u64, 1), ([2; 16], ctrl.image.len() as u64, 3)]);
        let meta = meta_nca("Application_0100000000010000.cnmt", &cn);
        let tik = crate::ticket::build([7; 16], [8; 16], false);
        let files: [(&str, &[u8]); 4] = [
            ("01010101010101010101010101010101.nca", &prog.image),
            ("02020202020202020202020202020202.nca", &ctrl.image),
            ("03030303030303030303030303030303.cnmt.nca", &meta.image),
            ("07070707070707070707070707070707.tik", &tik),
        ];
        let img = if ext == "nsp" { pfs0::build(&files) } else { crate::xci::build(&files, false) };
        (img, prog.keys) // every fixture NCA uses the same made-up keys
    }

    #[test]
    fn nsp_and_xci_title_name_icon_and_kind() {
        for ext in ["nsp", "xci"] {
            let (img, keys) = package(ext);
            let t = read(&img, ext, &keys).unwrap();
            assert!(t.problems.is_empty(), "{:?}", t.problems);
            assert_eq!(t.nca_count, 3);
            assert_eq!(t.title_id, Some(0x0100_0000_0001_0000));
            assert_eq!(t.kind, Some("application"));
            let n = t.nacp.unwrap();
            assert_eq!(n.name(&PREFERRED), Some("카트 레이서"));
            assert_eq!(n.display_version, "1.0.0");
            assert_eq!(t.icon.as_deref(), Some(&b"\xff\xd8KOJPEG"[..]));
        }
    }

    #[test]
    fn missing_keys_are_reported_per_file_without_values() {
        let (img, _) = package("nsp");
        let mut partial = KeySet::default();
        partial.insert("header_key", &[0x5au8; 32]);
        let t = read(&img, "nsp", &partial).unwrap();
        assert!(t.nacp.is_none());
        assert_eq!(t.problems.len(), 3);
        assert!(t.problems.iter().all(|p| p.1.contains("key_area_key_application_02")));
    }

    #[test]
    fn common_ticket_in_the_nsp_replaces_title_keys() {
        let prog = program_nca(&[("main", b"code")], true);
        let mut rid = [0u8; 16];
        rid[0] = 1;
        rid[15] = 2;
        let enc = prog.keys.encrypted_title_key(&rid).unwrap();
        let mut prod_only = KeySet::default();
        prod_only.insert("header_key", &[0x5au8; 32]);
        prod_only.insert("titlekek_02", &[0x19u8; 16]);
        let tik = crate::ticket::build(rid, enc, false);
        let with = pfs0::build(&[("p.nca", &prog.image), ("0100.tik", &tik)]);
        let t = read(&with, "nsp", &prod_only).unwrap();
        assert!(t.problems.is_empty(), "{:?}", t.problems);
        assert_eq!(t.title_id, Some(0x0100_0000_0001_0000));
        let without = pfs0::build(&[("p.nca", &prog.image)]);
        assert!(read(&without, "nsp", &prod_only).unwrap().problems[0].1.contains("rights id"));
        let personal = pfs0::build(&[("p.nca", &prog.image), ("x.tik", &crate::ticket::build(rid, enc, true))]);
        assert!(read(&personal, "nsp", &prod_only).unwrap().problems.iter().any(|p| p.1.contains("personalized")));
    }

    #[test]
    fn rejects_other_extensions_and_ncz() {
        assert!(read(&vec![0u8; 16], "zip", &KeySet::default()).is_err());
        let img = pfs0::build(&[("a.ncz", b"x")]);
        assert!(read(&img, "nsp", &KeySet::default()).unwrap_err().to_string().contains("NSZ"));
    }
}
