//! NACP (`control.nacp` in the Control NCA's RomFS, 0x4000 bytes): the game's name and
//! publisher in 16 languages (0x300 bytes each: name 0x200, publisher 0x100) and the
//! display version string at 0x3060. Icons are `icon_<Language>.dat` JPEGs beside it.

use crate::{Error, Result};

pub const LANGUAGES: [&str; 16] = [
    "AmericanEnglish",
    "BritishEnglish",
    "Japanese",
    "French",
    "German",
    "LatinAmericanSpanish",
    "Spanish",
    "Italian",
    "Dutch",
    "CanadianFrench",
    "Portuguese",
    "Russian",
    "Korean",
    "TraditionalChinese",
    "SimplifiedChinese",
    "BrazilianPortuguese",
];

#[derive(Debug, Clone)]
pub struct Nacp {
    /// (language, name, publisher) for every language that has a name.
    pub titles: Vec<(&'static str, String, String)>,
    pub display_version: String,
}

fn text(b: &[u8]) -> String {
    let end = b.iter().position(|&c| c == 0).unwrap_or(b.len());
    String::from_utf8_lossy(&b[..end]).trim().to_string()
}

impl Nacp {
    /// Name in the first available of the preferred languages, else any.
    pub fn name(&self, prefer: &[&str]) -> Option<&str> {
        prefer
            .iter()
            .find_map(|l| self.titles.iter().find(|t| t.0 == *l))
            .or_else(|| self.titles.first())
            .map(|t| t.1.as_str())
    }

    pub fn publisher(&self, prefer: &[&str]) -> Option<&str> {
        prefer
            .iter()
            .find_map(|l| self.titles.iter().find(|t| t.0 == *l))
            .or_else(|| self.titles.first())
            .map(|t| t.2.as_str())
    }
}

pub fn parse(b: &[u8]) -> Result<Nacp> {
    if b.len() < 0x3070 {
        return Err(Error::Truncated);
    }
    let mut titles = Vec::new();
    for (i, lang) in LANGUAGES.iter().enumerate() {
        let e = &b[i * 0x300..(i + 1) * 0x300];
        let name = text(&e[..0x200]);
        if !name.is_empty() {
            titles.push((*lang, name, text(&e[0x200..0x300])));
        }
    }
    if titles.is_empty() {
        return Err(Error::Format("control.nacp has no title name"));
    }
    Ok(Nacp { titles, display_version: text(&b[0x3060..0x3070]) })
}

/// Build a NACP for tests: (language index, name, publisher).
pub fn build(titles: &[(usize, &str, &str)], version: &str) -> Vec<u8> {
    let mut b = vec![0u8; 0x4000];
    for (i, name, publisher) in titles {
        b[i * 0x300..i * 0x300 + name.len()].copy_from_slice(name.as_bytes());
        b[i * 0x300 + 0x200..i * 0x300 + 0x200 + publisher.len()].copy_from_slice(publisher.as_bytes());
    }
    b[0x3060..0x3060 + version.len()].copy_from_slice(version.as_bytes());
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_and_preference() {
        let n = parse(&build(&[(0, "Kart Racer", "Example Co"), (12, "카트 레이서", "예시 회사")], "1.2.3")).unwrap();
        assert_eq!(n.name(&["Korean", "AmericanEnglish"]), Some("카트 레이서"));
        assert_eq!(n.name(&["Japanese"]), Some("Kart Racer"));
        assert_eq!(n.publisher(&["Korean"]), Some("예시 회사"));
        assert_eq!(n.display_version, "1.2.3");
    }

    #[test]
    fn empty_or_short() {
        assert!(parse(&vec![0u8; 0x4000]).is_err());
        assert!(matches!(parse(&[0u8; 0x100]), Err(Error::Truncated)));
    }
}
