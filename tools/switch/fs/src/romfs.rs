//! RomFS: the game's read-only data filesystem (NCA RomFS section level 6).
//!
//! Header (0x50): header size, then offset/size pairs for the directory hash table,
//! directory entries, file hash table and file entries, and the file data offset.
//! Directory entry: parent, sibling, first child dir, first file, hash, name length, name.
//! File entry: parent, sibling, data offset u64, size u64, hash, name length, name.

use crate::pfs0::Storage;
use crate::{u32le, u64le, Error, Result};

const NONE: u32 = 0xFFFF_FFFF;
pub const MAX_ENTRIES: usize = 1 << 20;
pub const MAX_TABLE: u64 = 64 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub path: String,
    /// Offset inside the RomFS storage.
    pub offset: u64,
    pub size: u64,
}

fn name(t: &[u8], at: usize) -> Result<String> {
    let len = u32le(t, at)? as usize;
    let raw = t.get(at + 4..at + 4 + len).ok_or(Error::Truncated)?;
    let s = std::str::from_utf8(raw).map_err(|_| Error::Format("RomFS name is not UTF-8"))?;
    if s.contains('/') || s.contains('\\') || s == "." || s == ".." {
        return Err(Error::Format("unsafe RomFS name"));
    }
    Ok(s.to_string())
}

pub fn list(s: &dyn Storage) -> Result<Vec<File>> {
    let h = s.read_vec(0, 0x50)?;
    if u64le(&h, 0)? != 0x50 {
        return Err(Error::BadMagic("RomFS"));
    }
    let (doff, dsize) = (u64le(&h, 0x18)?, u64le(&h, 0x20)?);
    let (foff, fsize) = (u64le(&h, 0x38)?, u64le(&h, 0x40)?);
    let data = u64le(&h, 0x48)?;
    if dsize > MAX_TABLE || fsize > MAX_TABLE {
        return Err(Error::Format("RomFS tables are too large"));
    }
    let dirs = s.read_vec(doff, dsize as usize)?;
    let files = s.read_vec(foff, fsize as usize)?;
    let mut out = Vec::new();
    // Iterative walk: (dir offset, path prefix); guard against loops with a visit budget.
    let mut stack = vec![(0u32, String::new())];
    let mut budget = MAX_ENTRIES;
    while let Some((d, prefix)) = stack.pop() {
        let d = d as usize;
        let mut f = u32le(&dirs, d + 12)?;
        while f != NONE {
            budget = budget.checked_sub(1).ok_or(Error::Format("RomFS has too many entries"))?;
            let e = f as usize;
            let off = u64le(&files, e + 8)?;
            let size = u64le(&files, e + 16)?;
            let abs = data.checked_add(off).ok_or(Error::Truncated)?;
            if abs.checked_add(size).ok_or(Error::Truncated)? > s.len() {
                return Err(Error::Truncated);
            }
            out.push(File { path: format!("{prefix}/{}", name(&files, e + 28)?), offset: abs, size });
            f = u32le(&files, e + 4)?;
        }
        let mut c = u32le(&dirs, d + 8)?;
        while c != NONE {
            budget = budget.checked_sub(1).ok_or(Error::Format("RomFS has too many entries"))?;
            let e = c as usize;
            stack.push((c, format!("{prefix}/{}", name(&dirs, e + 20)?)));
            c = u32le(&dirs, e + 4)?;
        }
    }
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// Build a RomFS image for tests from (path, data) pairs ("/a/b.txt"). Hash tables are
/// left empty (one bucket of NONE), which readers that walk the tree do not need.
pub fn build(entries: &[(&str, &[u8])]) -> Vec<u8> {
    use std::collections::BTreeMap;
    #[derive(Default)]
    struct D {
        dirs: BTreeMap<String, D>,
        files: Vec<(String, Vec<u8>)>,
    }
    let mut root = D::default();
    for (p, data) in entries {
        let parts: Vec<&str> = p.trim_start_matches('/').split('/').collect();
        let mut d = &mut root;
        for part in &parts[..parts.len() - 1] {
            d = d.dirs.entry(part.to_string()).or_default();
        }
        d.files.push((parts[parts.len() - 1].to_string(), data.to_vec()));
    }
    let pad4 = |n: usize| (n + 3) & !3;
    let mut dt: Vec<u8> = Vec::new();
    let mut ft: Vec<u8> = Vec::new();
    let mut blob: Vec<u8> = Vec::new();
    fn put(t: &mut [u8], at: usize, v: u32) {
        t[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    // Recursive layout: returns this dir's offset in dt.
    fn lay(d: &D, name: &str, parent: u32, dt: &mut Vec<u8>, ft: &mut Vec<u8>, blob: &mut Vec<u8>, pad4: &dyn Fn(usize) -> usize) -> u32 {
        let me = dt.len() as u32;
        dt.extend_from_slice(&[0u8; 24]);
        dt.extend_from_slice(name.as_bytes());
        dt.resize(me as usize + 24 + pad4(name.len()), 0);
        put(dt, me as usize, parent);
        put(dt, me as usize + 4, NONE);
        put(dt, me as usize + 16, NONE);
        put(dt, me as usize + 20, name.len() as u32);
        let mut prev: Option<usize> = None;
        let mut first_file = NONE;
        for (fname, data) in &d.files {
            let at = ft.len();
            ft.extend_from_slice(&[0u8; 32]);
            ft.extend_from_slice(fname.as_bytes());
            ft.resize(at + 32 + pad4(fname.len()), 0);
            put(ft, at, me);
            put(ft, at + 4, NONE);
            ft[at + 8..at + 16].copy_from_slice(&(blob.len() as u64).to_le_bytes());
            ft[at + 16..at + 24].copy_from_slice(&(data.len() as u64).to_le_bytes());
            put(ft, at + 24, NONE);
            put(ft, at + 28, fname.len() as u32);
            blob.extend_from_slice(data);
            while !blob.len().is_multiple_of(16) {
                blob.push(0);
            }
            match prev {
                Some(p) => put(ft, p + 4, at as u32),
                None => first_file = at as u32,
            }
            prev = Some(at);
        }
        put(dt, me as usize + 12, first_file);
        let mut prev_dir: Option<u32> = None;
        let mut first_dir = NONE;
        for (dname, sub) in &d.dirs {
            let child = lay(sub, dname, me, dt, ft, blob, pad4);
            match prev_dir {
                Some(p) => put(dt, p as usize + 4, child),
                None => first_dir = child,
            }
            prev_dir = Some(child);
        }
        put(dt, me as usize + 8, first_dir);
        me
    }
    lay(&root, "", 0, &mut dt, &mut ft, &mut blob, &pad4);
    let empty_hash = NONE.to_le_bytes().to_vec();
    let mut out = vec![0u8; 0x50];
    let mut cursor = 0x50u64;
    let mut place = |out: &mut Vec<u8>, field: usize, bytes: &[u8]| {
        out[field..field + 8].copy_from_slice(&cursor.to_le_bytes());
        out[field + 8..field + 16].copy_from_slice(&(bytes.len() as u64).to_le_bytes());
        cursor += bytes.len() as u64;
    };
    let mut body = Vec::new();
    for (field, bytes) in [(0x08, &empty_hash), (0x18, &dt), (0x28, &empty_hash), (0x38, &ft)] {
        place(&mut out, field, bytes);
        body.extend_from_slice(bytes);
    }
    while !(0x50 + body.len()).is_multiple_of(16) {
        body.push(0);
    }
    out[0..8].copy_from_slice(&0x50u64.to_le_bytes());
    out[0x48..0x50].copy_from_slice(&(0x50 + body.len() as u64).to_le_bytes());
    out.extend_from_slice(&body);
    out.extend_from_slice(&blob);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn walks_nested_tree() {
        let img = build(&[("/data/a.bin", b"AAAA"), ("/data/sub/b.txt", b"hello"), ("/root.txt", b"r"), ("/data/c", b"")]);
        let files = list(&img).unwrap();
        let paths: Vec<_> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["/data/a.bin", "/data/c", "/data/sub/b.txt", "/root.txt"]);
        let b = files.iter().find(|f| f.path == "/data/sub/b.txt").unwrap();
        assert_eq!(img.read_vec(b.offset, b.size as usize).unwrap(), b"hello");
    }

    #[test]
    fn rejects_loops_and_bad_headers() {
        let mut img = build(&[("/a", b"x")]);
        // Make the only file its own sibling: the walk must stop, not spin forever.
        let foff = u64le(&img, 0x38).unwrap() as usize;
        img[foff + 4..foff + 8].copy_from_slice(&0u32.to_le_bytes());
        assert!(list(&img).is_err());
        let mut bad = build(&[("/a", b"x")]);
        bad[0] = 0x40;
        assert!(matches!(list(&bad), Err(Error::BadMagic(_))));
    }
}
