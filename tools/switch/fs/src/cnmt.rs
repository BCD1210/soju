//! CNMT (content meta, the `.cnmt` file inside a Meta NCA): which title this package is
//! (application, update or add-on), its version, and the NCAs it consists of.
//!
//! Header (0x20): title id u64, version u32, type u8 at 0xC, extended header size u16
//! at 0xE, content count u16 at 0x10, meta count u16 at 0x12. After the extended
//! header come 0x38-byte content records: SHA-256, content id (16), size (6 bytes),
//! content type, id offset.

use crate::{u16le, u32le, u64le, Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaType {
    SystemProgram,
    Application,
    Patch,
    AddOnContent,
    Delta,
    Other(u8),
}

impl MetaType {
    pub fn name(&self) -> &'static str {
        match self {
            Self::SystemProgram => "system",
            Self::Application => "application",
            Self::Patch => "update",
            Self::AddOnContent => "add-on",
            Self::Delta => "delta",
            Self::Other(_) => "other",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Meta,
    Program,
    Data,
    Control,
    HtmlDocument,
    LegalInformation,
    DeltaFragment,
    Other(u8),
}

#[derive(Debug, Clone)]
pub struct Content {
    pub id: [u8; 16],
    pub size: u64,
    pub kind: ContentKind,
}

impl Content {
    /// File name of this content inside an NSP/XCI.
    pub fn file_name(&self) -> String {
        let hex: String = self.id.iter().map(|b| format!("{b:02x}")).collect();
        if self.kind == ContentKind::Meta { format!("{hex}.cnmt.nca") } else { format!("{hex}.nca") }
    }
}

#[derive(Debug, Clone)]
pub struct Cnmt {
    pub title_id: u64,
    pub version: u32,
    pub meta_type: MetaType,
    /// For updates and add-ons: the base application's title id.
    pub application_id: Option<u64>,
    pub contents: Vec<Content>,
}

pub fn parse(b: &[u8]) -> Result<Cnmt> {
    if b.len() < 0x20 {
        return Err(Error::Truncated);
    }
    let meta_type = match b[0xC] {
        0x01 => MetaType::SystemProgram,
        0x80 => MetaType::Application,
        0x81 => MetaType::Patch,
        0x82 => MetaType::AddOnContent,
        0x83 => MetaType::Delta,
        o => MetaType::Other(o),
    };
    let ext = u16le(b, 0xE)? as usize;
    let count = u16le(b, 0x10)? as usize;
    let application_id = match meta_type {
        MetaType::Patch | MetaType::AddOnContent if ext >= 8 => Some(u64le(b, 0x20)?),
        _ => None,
    };
    let mut contents = Vec::with_capacity(count);
    for i in 0..count {
        let r = 0x20 + ext + i * 0x38;
        let rec = b.get(r..r + 0x38).ok_or(Error::Truncated)?;
        let mut size = [0u8; 8];
        size[..6].copy_from_slice(&rec[0x30..0x36]);
        let kind = match rec[0x36] {
            0 => ContentKind::Meta,
            1 => ContentKind::Program,
            2 => ContentKind::Data,
            3 => ContentKind::Control,
            4 => ContentKind::HtmlDocument,
            5 => ContentKind::LegalInformation,
            6 => ContentKind::DeltaFragment,
            o => ContentKind::Other(o),
        };
        contents.push(Content { id: rec[0x20..0x30].try_into().unwrap(), size: u64::from_le_bytes(size), kind });
    }
    Ok(Cnmt { title_id: u64le(b, 0)?, version: u32le(b, 8)?, meta_type, application_id, contents })
}

/// Build a CNMT for tests.
pub fn build(title_id: u64, version: u32, meta_type: u8, application_id: Option<u64>, contents: &[([u8; 16], u64, u8)]) -> Vec<u8> {
    let ext: u16 = if application_id.is_some() || meta_type == 0x80 { 0x10 } else { 0 };
    let mut b = vec![0u8; 0x20 + ext as usize];
    b[..8].copy_from_slice(&title_id.to_le_bytes());
    b[8..12].copy_from_slice(&version.to_le_bytes());
    b[0xC] = meta_type;
    b[0xE..0x10].copy_from_slice(&ext.to_le_bytes());
    b[0x10..0x12].copy_from_slice(&(contents.len() as u16).to_le_bytes());
    if let Some(a) = application_id {
        b[0x20..0x28].copy_from_slice(&a.to_le_bytes());
    }
    for (id, size, kind) in contents {
        let mut rec = [0u8; 0x38];
        rec[0x20..0x30].copy_from_slice(id);
        rec[0x30..0x36].copy_from_slice(&size.to_le_bytes()[..6]);
        rec[0x36] = *kind;
        b.extend_from_slice(&rec);
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn application_and_update() {
        let c = parse(&build(0x0100_0000_0001_0000, 0, 0x80, None, &[([1; 16], 0x12_3456_789a, 1), ([2; 16], 99, 3)])).unwrap();
        assert_eq!(c.meta_type, MetaType::Application);
        assert_eq!(c.contents.len(), 2);
        assert_eq!(c.contents[0].size, 0x12_3456_789a);
        assert_eq!(c.contents[1].kind, ContentKind::Control);
        assert_eq!(c.contents[1].file_name(), "02020202020202020202020202020202.nca");
        let u = parse(&build(0x0100_0000_0001_0800, 65536, 0x81, Some(0x0100_0000_0001_0000), &[])).unwrap();
        assert_eq!(u.meta_type.name(), "update");
        assert_eq!(u.application_id, Some(0x0100_0000_0001_0000));
        assert_eq!(u.version, 65536);
    }

    #[test]
    fn truncated_records() {
        let mut b = build(1, 0, 0x80, None, &[([1; 16], 1, 1)]);
        b.truncate(b.len() - 1);
        assert!(matches!(parse(&b), Err(Error::Truncated)));
    }
}
