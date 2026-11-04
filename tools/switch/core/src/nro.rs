//! Bounded port of Soju's trace_nro.py image loader and initial homebrew ABI.
//! Appended assets are not mapped. No firmware, keys, or retail formats are used.
use std::fmt;
use std::io::{self, Read, Seek, SeekFrom};

pub const MAX_IMAGE: usize = 64 * 1024 * 1024;
pub const BASE: u64 = 0x100000000;
pub const STACK: u64 = 0x200000000;
pub const STACK_SIZE: usize = 2 * 1024 * 1024;
pub const TLS: u64 = 0x300000000;
pub const CONFIG: u64 = 0x300001000;
pub const EXCEPTION_CONTEXT: u64 = 0x300010000;
pub const LOADER_RETURN: u64 = 0x300020000;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    Header,
    Size,
    Segment,
    Text,
    PageAlignment,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "NRO input: {e}"),
            Self::Header => f.write_str("invalid or truncated NRO header"),
            Self::Size => f.write_str("unsupported NRO version or image size"),
            Self::Segment => f.write_str("invalid NRO segment layout"),
            Self::Text => f.write_str("invalid NRO text segment"),
            Self::PageAlignment => {
                f.write_str("NRO segments sharing a 4 KiB page cannot have distinct permissions")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Segment {
    pub offset: usize,
    pub size: usize,
}
#[derive(Debug)]
pub struct Nro {
    pub image: Vec<u8>,
    pub bss: usize,
    pub segments: [Segment; 3],
}
fn word(header: &[u8; 0x80], offset: usize) -> usize {
    u32::from_le_bytes(header[offset..offset + 4].try_into().unwrap()) as usize
}
impl Nro {
    /// Build page-separated text, rodata and data/BSS for the hardware MMU.
    pub fn protected_homebrew(&self) -> Result<Homebrew, Error> {
        let mut boot = self.homebrew();
        let image = boot.regions.remove(0).bytes;
        let mut regions = Vec::new();
        for (index, segment) in self.segments.iter().enumerate() {
            let end = if index == 2 {
                image.len()
            } else {
                segment.offset + segment.size
            };
            if end == segment.offset {
                continue;
            }
            if !segment.offset.is_multiple_of(0x1000) || !end.is_multiple_of(0x1000) {
                return Err(Error::PageAlignment);
            }
            regions.push(Region {
                address: BASE + u64::try_from(segment.offset).map_err(|_| Error::Size)?,
                bytes: image[segment.offset..end].to_vec(),
                permissions: [5, 1, 3][index],
            });
        }
        regions.extend(boot.regions);
        boot.regions = regions;
        Ok(boot)
    }

    pub fn read(source: &mut (impl Read + Seek)) -> Result<Self, Error> {
        source.seek(SeekFrom::Start(0))?;
        let mut header = [0; 0x80];
        source.read_exact(&mut header).map_err(|error| {
            if error.kind() == io::ErrorKind::UnexpectedEof {
                Error::Header
            } else {
                Error::Io(error)
            }
        })?;
        if &header[0x10..0x14] != b"NRO0" {
            return Err(Error::Header);
        }
        let size = word(&header, 0x18);
        let bss = word(&header, 0x38);
        if word(&header, 0x14) != 0 || !(0x80..=MAX_IMAGE).contains(&size) || bss > MAX_IMAGE - size
        {
            return Err(Error::Size);
        }
        let mut segments = [Segment { offset: 0, size: 0 }; 3];
        let mut end = 0;
        for (i, segment) in segments.iter_mut().enumerate() {
            let offset = word(&header, 0x20 + i * 8);
            let length = word(&header, 0x24 + i * 8);
            if offset < end || offset > size || length > size - offset {
                return Err(Error::Segment);
            }
            if i == 0 && (offset != 0 || length < 0x80 || !length.is_multiple_of(4)) {
                return Err(Error::Text);
            }
            *segment = Segment {
                offset,
                size: length,
            };
            end = offset + length;
        }
        source.seek(SeekFrom::Start(0))?;
        let mut image = vec![0; size];
        source.read_exact(&mut image)?;
        Ok(Self {
            image,
            bss,
            segments,
        })
    }

    /// Initial diagnostic mappings; these do not yet enforce NRO segment permissions.
    pub fn homebrew(&self) -> Homebrew {
        let mapped = (self.image.len() + self.bss + 0xfff) & !0xfff;
        let mut image = vec![0; mapped];
        image[..self.image.len()].copy_from_slice(&self.image);
        let mut config = vec![0; 0x1000];
        let argv = b"sdmc:/switch/soju-current.nro\0";
        config[0x200..0x200 + argv.len()].copy_from_slice(argv);
        config[0x300] = 1; // Local offline user, matching account and player-selector services.
        let entries: [(u32, u32, u64, u64); 5] = [
            (1, 0, 0x101, 0),
            (5, 0, 0, CONFIG + 0x200),
            (7, 0, 2, 0),
            (15, 0, CONFIG + 0x300, 0),
            (0, 0, 0, 0),
        ];
        for (index, (key, flags, a, b)) in entries.into_iter().enumerate() {
            let at = index * 24;
            config[at..at + 4].copy_from_slice(&key.to_le_bytes());
            config[at + 4..at + 8].copy_from_slice(&flags.to_le_bytes());
            config[at + 8..at + 16].copy_from_slice(&a.to_le_bytes());
            config[at + 16..at + 24].copy_from_slice(&b.to_le_bytes());
        }
        let mut loader_return = vec![0; 0x1000];
        loader_return[..4].copy_from_slice(&0xd4000fc1u32.to_le_bytes()); // private SVC 0x7e
        Homebrew {
            regions: vec![
                Region {
                    address: BASE,
                    bytes: image,
                    permissions: 7,
                },
                Region {
                    address: STACK,
                    bytes: vec![0; STACK_SIZE],
                    permissions: 3,
                },
                Region {
                    address: TLS,
                    bytes: vec![0; 0x1000],
                    permissions: 3,
                },
                Region {
                    address: CONFIG,
                    bytes: config,
                    permissions: 3,
                },
                Region {
                    address: EXCEPTION_CONTEXT,
                    bytes: vec![0; 0x1000],
                    permissions: 3,
                },
                Region {
                    address: LOADER_RETURN,
                    bytes: loader_return,
                    permissions: 5,
                },
            ],
            pc: BASE,
            sp: STACK + STACK_SIZE as u64 - 0x1000,
            x0: CONFIG,
            x1: u64::MAX,
            x30: LOADER_RETURN,
            tpidrro_el0: TLS,
        }
    }
}
#[derive(Debug)]
pub struct Region {
    pub address: u64,
    pub bytes: Vec<u8>,
    /// Read=1, write=2, execute=4.
    pub permissions: u8,
}
#[derive(Debug)]
pub struct Homebrew {
    pub regions: Vec<Region>,
    pub pc: u64,
    pub sp: u64,
    pub x0: u64,
    pub x1: u64,
    pub x30: u64,
    pub tpidrro_el0: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    fn put(data: &mut [u8], offset: usize, value: u32) {
        data[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    }
    fn fixture() -> Vec<u8> {
        let mut bytes = vec![0; 0x100];
        put(&mut bytes, 0, 0x14000020);
        bytes[0x10..0x14].copy_from_slice(b"NRO0");
        put(&mut bytes, 0x18, 0x100);
        for offset in [0x24, 0x28, 0x30] {
            put(&mut bytes, offset, 0x100);
        }
        put(&mut bytes, 0x80, 0xd28000e0); // mov x0,#7
        put(&mut bytes, 0x84, 0xd40004c1); // svc #0x26
        bytes
    }
    #[test]
    fn loads_image_and_ignores_appended_assets() {
        let mut bytes = fixture();
        bytes.extend_from_slice(b"synthetic appended asset");
        let loaded = Nro::read(&mut Cursor::new(bytes)).unwrap();
        assert_eq!(loaded.image, fixture());
        assert_eq!(loaded.bss, 0);
        assert_eq!(
            loaded.segments[0],
            Segment {
                offset: 0,
                size: 0x100
            }
        );
    }
    #[test]
    fn rejects_headers_sizes_and_truncation() {
        for length in [0, 4, 0x7f] {
            assert!(matches!(
                Nro::read(&mut Cursor::new(vec![0; length])),
                Err(Error::Header)
            ));
        }
        for (offset, value) in [
            (0x10, 0),
            (0x14, 1),
            (0x18, 0x70),
            (0x18, u32::MAX),
            (0x38, u32::MAX),
        ] {
            let mut bytes = fixture();
            put(&mut bytes, offset, value);
            assert!(Nro::read(&mut Cursor::new(bytes)).is_err());
        }
        let mut bytes = fixture();
        bytes.pop();
        assert!(matches!(
            Nro::read(&mut Cursor::new(bytes)),
            Err(Error::Io(_))
        ));
    }
    #[test]
    fn rejects_overlapping_outside_and_bad_text_segments() {
        for (offset, value) in [
            (0x20, 4),
            (0x24, 0x104),
            (0x24, 0x81),
            (0x24, 0x7c),
            (0x28, 0xfc),
            (0x2c, 1),
            (0x30, u32::MAX),
        ] {
            let mut bytes = fixture();
            put(&mut bytes, offset, value);
            assert!(
                Nro::read(&mut Cursor::new(bytes)).is_err(),
                "offset {offset:x}"
            );
        }
    }
    #[test]
    fn builds_zeroed_memory_and_homebrew_abi() {
        let mut bytes = fixture();
        put(&mut bytes, 0x38, 0x1001);
        let loaded = Nro::read(&mut Cursor::new(bytes.clone())).unwrap();
        let boot = loaded.homebrew();
        assert_eq!(boot.regions[0].bytes.len(), 0x2000);
        assert_eq!(&boot.regions[0].bytes[..0x100], &bytes);
        assert!(boot.regions[0].bytes[0x100..].iter().all(|&b| b == 0));
        assert_eq!(
            (
                boot.pc,
                boot.sp,
                boot.x0,
                boot.x1,
                boot.x30,
                boot.tpidrro_el0
            ),
            (
                BASE,
                STACK + STACK_SIZE as u64 - 0x1000,
                CONFIG,
                u64::MAX,
                LOADER_RETURN,
                TLS
            )
        );
        let config = &boot.regions[3].bytes;
        assert_eq!(&config[..8], &[1, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(&config[8..16], &0x101u64.to_le_bytes());
        assert_eq!(&config[40..48], &(CONFIG + 0x200).to_le_bytes());
        assert_eq!(&config[48..52], &7u32.to_le_bytes());
        assert_eq!(&config[56..64], &2u64.to_le_bytes());
        assert_eq!(&config[72..76], &15u32.to_le_bytes());
        assert_eq!(&config[80..88], &(CONFIG + 0x300).to_le_bytes());
        assert_eq!(&config[0x300..0x310], &1u128.to_le_bytes());
        assert!(config[96..120].iter().all(|&b| b == 0));
        assert_eq!(&config[0x200..0x21e], b"sdmc:/switch/soju-current.nro\0");
        assert_eq!(&boot.regions[5].bytes[..4], &0xd4000fc1u32.to_le_bytes());
        assert_eq!(boot.regions[5].permissions, 5);
    }

    #[test]
    fn protected_segments_reject_shared_pages_and_preserve_bss() {
        let unaligned = Nro::read(&mut Cursor::new(fixture())).unwrap();
        assert!(matches!(
            unaligned.protected_homebrew(),
            Err(Error::PageAlignment)
        ));
        let mut bytes = fixture();
        bytes.resize(0x3000, 0);
        for (offset, value) in [
            (0x18, 0x3000),
            (0x24, 0x1000),
            (0x28, 0x1000),
            (0x2c, 0x1000),
            (0x30, 0x2000),
            (0x34, 0x1000),
            (0x38, 0x1001),
        ] {
            put(&mut bytes, offset, value);
        }
        bytes[0x1000] = 0x42;
        bytes[0x2000] = 0x73;
        let boot = Nro::read(&mut Cursor::new(bytes))
            .unwrap()
            .protected_homebrew()
            .unwrap();
        assert_eq!(
            boot.regions[..3]
                .iter()
                .map(|r| (r.address, r.bytes.len(), r.permissions))
                .collect::<Vec<_>>(),
            [
                (BASE, 0x1000, 5),
                (BASE + 0x1000, 0x1000, 1),
                (BASE + 0x2000, 0x3000, 3)
            ]
        );
        assert_eq!(boot.regions[1].bytes[0], 0x42);
        assert_eq!(boot.regions[2].bytes[0], 0x73);
        assert!(
            boot.regions[2].bytes[0x1000..]
                .iter()
                .all(|&byte| byte == 0)
        );
    }
}
