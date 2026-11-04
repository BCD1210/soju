//! A firmware dump from the player's own console: a folder (or the files of a ZIP the
//! player extracted) of NCAs named `<content id>.nca` or `<content id>.cnmt.nca`.
//! We open every NCA with the player's keys, read the Meta NCAs' CNMTs to learn which
//! system title each content belongs to, and report the firmware's system titles and the
//! ones Soju's runner needs (shared fonts, time zones, NG words, system version).

use crate::cnmt::{self, Cnmt};
use crate::keys::KeySet;
use crate::nca::{ContentType, Nca};
use crate::pfs0::Storage;
use crate::{Error, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// System titles the runner needs, by title id.
pub const NEEDED: [(u64, &str); 9] = [
    (0x0100000000000809, "SystemVersion"),
    (0x010000000000080E, "TimeZoneBinary"),
    (0x0100000000000810, "FontNintendoExtension"),
    (0x0100000000000811, "FontStandard"),
    (0x0100000000000812, "FontKorean"),
    (0x0100000000000813, "FontChineseTraditional"),
    (0x0100000000000814, "FontChineseSimple"),
    (0x0100000000000818, "SystemUpdateNgWord"),
    (0x0100000000000823, "NgWord2"),
];

#[derive(Debug, Clone)]
pub struct FirmwareReport {
    pub nca_count: usize,
    /// title id -> (version, content NCA file names)
    pub titles: BTreeMap<u64, (u32, Vec<String>)>,
    pub missing_needed: Vec<(u64, &'static str)>,
    /// (file, problem) for NCAs that could not be opened, without key values.
    pub problems: Vec<(String, String)>,
}

pub const MAX_FILES: usize = 4096;
const MAX_CNMT: u64 = 1 << 20;

fn nca_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let e = e?;
        let p = e.path();
        let ft = e.file_type()?;
        // Some dumps store each NCA as a folder holding "00"; accept both, never follow links.
        if ft.is_symlink() {
            continue;
        }
        let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.ends_with(".nca") {
            continue;
        }
        if ft.is_file() {
            v.push(p);
        } else if ft.is_dir() && p.join("00").is_file() {
            v.push(p.join("00"));
        }
        if v.len() > MAX_FILES {
            return Err(Error::Format("too many files for a firmware folder"));
        }
    }
    v.sort();
    Ok(v)
}

fn display_name(p: &Path) -> String {
    if p.file_name().and_then(|n| n.to_str()) == Some("00") {
        p.parent().and_then(|d| d.file_name()).and_then(|n| n.to_str()).unwrap_or("?").to_string()
    } else {
        p.file_name().and_then(|n| n.to_str()).unwrap_or("?").to_string()
    }
}

pub fn scan(dir: &Path, keys: &KeySet) -> Result<FirmwareReport> {
    let files = nca_files(dir)?;
    if files.is_empty() {
        return Err(Error::Format("no .nca files in this folder; choose the folder of your own firmware dump"));
    }
    let mut metas: Vec<Cnmt> = Vec::new();
    let mut problems = Vec::new();
    for path in &files {
        let name = display_name(path);
        let f = std::fs::File::open(path)?;
        let nca = match Nca::open(&f, keys) {
            Ok(n) => n,
            Err(e) => {
                problems.push((name, e.to_string()));
                continue;
            }
        };
        if nca.header.content_type != ContentType::Meta {
            continue;
        }
        let read = || -> Result<Cnmt> {
            let files = nca.pfs0_files(0)?;
            let c = files.iter().find(|e| e.name.ends_with(".cnmt")).ok_or(Error::Format("meta NCA without a .cnmt"))?;
            if c.size > MAX_CNMT {
                return Err(Error::Format("cnmt is too large"));
            }
            cnmt::parse(&nca.section(0)?.read_vec(c.offset, c.size as usize)?)
        };
        match read() {
            Ok(c) => metas.push(c),
            Err(e) => problems.push((name, e.to_string())),
        }
    }
    let mut titles = BTreeMap::new();
    for m in &metas {
        let names = m.contents.iter().map(|c| c.file_name()).collect();
        titles.insert(m.title_id, (m.version, names));
    }
    let missing_needed = NEEDED.iter().copied().filter(|(id, _)| !titles.contains_key(id)).collect();
    Ok(FirmwareReport { nca_count: files.len(), titles, missing_needed, problems })
}

/// The firmware version string from SystemVersion's RomFS ("file" at offset 0x68, e.g. "18.1.0").
pub fn system_version(dir: &Path, report: &FirmwareReport, keys: &KeySet) -> Option<String> {
    let (_, contents) = report.titles.get(&0x0100000000000809)?;
    contents.iter().filter(|n| !n.ends_with(".cnmt.nca")).find_map(|name| version_from(dir, name, keys))
}

fn version_from(dir: &Path, name: &str, keys: &KeySet) -> Option<String> {
    let path = [dir.join(name), dir.join(name).join("00")].into_iter().find(|p| p.is_file())?;
    let f = std::fs::File::open(path).ok()?;
    let nca = Nca::open(&f, keys).ok()?;
    let sec = nca.header.sections.iter().find(|s| s.fs_type == crate::nca::FsType::RomFs)?;
    let r = nca.section(sec.index).ok()?;
    let file = crate::romfs::list(&r).ok()?.into_iter().find(|f| f.path == "/file")?;
    let b = r.read_vec(file.offset, file.size.min(0x100) as usize).ok()?;
    let raw = b.get(0x68..0x80)?;
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    Some(String::from_utf8_lossy(&raw[..end]).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nca::fixture::{control_nca, meta_nca};

    fn dir() -> PathBuf {
        let d = std::env::temp_dir().join(format!("soju-fw-{}-{}", std::process::id(), rand_suffix()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn rand_suffix() -> u64 {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64
    }

    #[test]
    fn reads_system_titles_version_and_missing_ones() {
        let d = dir();
        // SystemVersion: a data NCA with RomFS "/file" holding the version string at 0x68.
        let mut file = vec![0u8; 0x100];
        file[0x68..0x6E].copy_from_slice(b"18.1.0");
        let data = control_nca(&crate::romfs::build(&[("/file", &file)]));
        let data_id = [0xAAu8; 16];
        let sv = cnmt::build(0x0100000000000809, 0x12100000, 0x01, None, &[(data_id, data.image.len() as u64, 2)]);
        let meta1 = meta_nca("SystemData_0100000000000809.cnmt", &sv);
        let font = cnmt::build(0x0100000000000811, 0, 0x01, None, &[]);
        let meta2 = meta_nca("SystemData_0100000000000811.cnmt", &font);
        std::fs::write(d.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca"), &data.image).unwrap();
        std::fs::write(d.join("11111111111111111111111111111111.cnmt.nca"), &meta1.image).unwrap();
        // The folder layout some dumps use: "<id>.cnmt.nca/00".
        std::fs::create_dir_all(d.join("22222222222222222222222222222222.cnmt.nca")).unwrap();
        std::fs::write(d.join("22222222222222222222222222222222.cnmt.nca/00"), &meta2.image).unwrap();
        std::fs::write(d.join("notes.txt"), b"ignored").unwrap();
        let r = scan(&d, &data.keys).unwrap();
        assert_eq!(r.nca_count, 3);
        assert!(r.problems.is_empty(), "{:?}", r.problems);
        assert_eq!(r.titles.len(), 2);
        assert_eq!(r.titles[&0x0100000000000809].0, 0x12100000);
        assert!(r.missing_needed.iter().any(|m| m.1 == "TimeZoneBinary"));
        assert!(!r.missing_needed.iter().any(|m| m.1 == "FontStandard"));
        assert_eq!(system_version(&d, &r, &data.keys).as_deref(), Some("18.1.0"));
        std::fs::remove_dir_all(&d).ok();
    }

    #[test]
    fn empty_folder_and_wrong_keys() {
        let d = dir();
        assert!(scan(&d, &KeySet::default()).unwrap_err().to_string().contains("no .nca"));
        let m = meta_nca("x.cnmt", &cnmt::build(1, 0, 1, None, &[]));
        std::fs::write(d.join("33333333333333333333333333333333.cnmt.nca"), &m.image).unwrap();
        let mut wrong = KeySet::default();
        wrong.insert("header_key", &[1u8; 32]);
        let r = scan(&d, &wrong).unwrap();
        assert_eq!(r.problems.len(), 1);
        assert!(r.titles.is_empty());
        std::fs::remove_dir_all(&d).ok();
    }
}
