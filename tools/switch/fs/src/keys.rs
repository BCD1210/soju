//! Parser for the player's own `prod.keys` / `title.keys` text files.
//!
//! Format (one per line): `name = hexvalue`. Comments start with `;` or `#`.
//! Values are never printed: `Debug` shows only key names.

use crate::{Error, Result};
use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;

#[derive(Default, Clone)]
pub struct KeySet {
    named: BTreeMap<String, Vec<u8>>,
    /// rights id (16 bytes) -> encrypted title key (16 bytes)
    titles: BTreeMap<[u8; 16], [u8; 16]>,
}

impl fmt::Debug for KeySet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeySet")
            .field("keys", &self.named.keys().collect::<Vec<_>>())
            .field("title_keys", &self.titles.len())
            .finish()
    }
}

pub(crate) fn unhex(text: &str) -> Option<Vec<u8>> {
    let t = text.trim();
    if t.is_empty() || !t.len().is_multiple_of(2) {
        return None;
    }
    (0..t.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(t.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Parse `name = hex` lines. Bad lines are reported by line number only, never by content.
fn lines(text: &str) -> Result<Vec<(String, Vec<u8>)>> {
    let mut out = Vec::new();
    for (n, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        let (name, value) = line.split_once('=').ok_or(Error::Format("a key line has no '='"))?;
        let name = name.trim().to_ascii_lowercase();
        let ok_name = !name.is_empty() && name.len() <= 64 && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
        let bytes = unhex(value);
        match (ok_name, bytes) {
            (true, Some(b)) if b.len() == 16 || b.len() == 32 => out.push((name, b)),
            _ => {
                let _ = n; // line number available for callers' diagnostics if needed
                return Err(Error::Format("a key line is not 'name = 32 or 64 hex digits'"));
            }
        }
    }
    Ok(out)
}

impl KeySet {
    pub fn parse_prod(text: &str) -> Result<Self> {
        let mut ks = KeySet::default();
        for (name, value) in lines(text)? {
            ks.named.insert(name, value);
        }
        Ok(ks)
    }

    pub fn add_titles(&mut self, text: &str) -> Result<()> {
        for (name, value) in lines(text)? {
            let rights = unhex(&name).filter(|r| r.len() == 16).ok_or(Error::Format("title.keys name is not a 32-digit rights id"))?;
            if value.len() != 16 {
                return Err(Error::Format("a title key must be 32 hex digits"));
            }
            self.titles.insert(rights.try_into().unwrap(), value.try_into().unwrap());
        }
        Ok(())
    }

    pub fn load(prod: &Path, title: Option<&Path>) -> Result<Self> {
        let mut ks = Self::parse_prod(&std::fs::read_to_string(prod)?)?;
        if let Some(t) = title {
            ks.add_titles(&std::fs::read_to_string(t)?)?;
        }
        Ok(ks)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.named.keys().map(|s| s.as_str())
    }

    pub fn title_key_count(&self) -> usize {
        self.titles.len()
    }

    pub fn get16(&self, name: &str) -> Result<[u8; 16]> {
        match self.named.get(name) {
            Some(v) if v.len() == 16 => Ok(v.as_slice().try_into().unwrap()),
            Some(_) => Err(Error::Format("a key has the wrong length")),
            None => Err(Error::MissingKey(name.to_string())),
        }
    }

    pub fn get32(&self, name: &str) -> Result<[u8; 32]> {
        match self.named.get(name) {
            Some(v) if v.len() == 32 => Ok(v.as_slice().try_into().unwrap()),
            Some(_) => Err(Error::Format("a key has the wrong length")),
            None => Err(Error::MissingKey(name.to_string())),
        }
    }

    pub fn encrypted_title_key(&self, rights_id: &[u8; 16]) -> Result<[u8; 16]> {
        self.titles.get(rights_id).copied().ok_or_else(|| {
            let id: String = rights_id.iter().map(|b| format!("{b:02x}")).collect();
            Error::MissingKey(format!("title key for rights id {id}"))
        })
    }

    #[doc(hidden)]
    pub fn insert(&mut self, name: &str, value: &[u8]) {
        self.named.insert(name.to_string(), value.to_vec());
    }

    #[doc(hidden)]
    pub fn insert_title(&mut self, rights: [u8; 16], key: [u8; 16]) {
        self.titles.insert(rights, key);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_never_debug_prints_values() {
        let text = "; comment\nheader_key = 00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff\nTITLEKEK_00=0f0e0d0c0b0a09080706050403020100\n";
        let ks = KeySet::parse_prod(text).unwrap();
        assert_eq!(ks.get32("header_key").unwrap()[1], 0x11);
        assert_eq!(ks.get16("titlekek_00").unwrap()[0], 0x0f);
        let shown = format!("{ks:?}");
        assert!(shown.contains("header_key"));
        assert!(!shown.contains("00112233"));
        assert!(!shown.contains("0f0e0d"));
    }

    #[test]
    fn missing_key_names_only_the_key() {
        let ks = KeySet::default();
        let e = ks.get16("key_area_key_application_00").unwrap_err().to_string();
        assert!(e.contains("key_area_key_application_00"));
    }

    #[test]
    fn rejects_bad_lines_without_echoing_them() {
        for bad in ["header_key = xyz", "header_key 0011", "bad name = 00112233445566778899aabbccddeeff", "k = 0011"] {
            let e = KeySet::parse_prod(bad).unwrap_err().to_string();
            assert!(!e.contains("0011"), "{e}");
        }
    }

    #[test]
    fn title_keys() {
        let mut ks = KeySet::default();
        ks.add_titles("0100000000010000000000000000000a = 000102030405060708090a0b0c0d0e0f\n").unwrap();
        let mut rid = [0u8; 16];
        rid[..8].copy_from_slice(&[1, 0, 0, 0, 0, 1, 0, 0]);
        rid[15] = 0x0a;
        assert_eq!(ks.encrypted_title_key(&rid).unwrap()[15], 0x0f);
        assert_eq!(ks.title_key_count(), 1);
    }
}
