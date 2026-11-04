use soju_switch_fs::{
    crypto,
    keys::KeySet,
    nca::{self, fixture},
    npdm, nso, pfs0, romfs,
};
pub fn package(ext: &str, syscall: bool) -> (Vec<u8>, KeySet, Vec<u8>) {
    package_with_metadata(ext, syscall, |_| {})
}
pub fn package_with_metadata(
    ext: &str,
    syscall: bool,
    edit: impl FnOnce(&mut Vec<u8>),
) -> (Vec<u8>, KeySet, Vec<u8>) {
    let pid = 0x0100_0000_0001_0000;
    let caps = [
        7 | (63 << 4) | (3 << 24),
        15 | if syscall { 1 << (7 + 5) } else { 0 },
    ];
    let mut metadata = npdm::build("Own fixture", pid, &["fsp-srv"], &caps);
    metadata[12] = 1 | (3 << 1);
    metadata[14] = 35;
    metadata[15] = 2;
    edit(&mut metadata);
    // rtld branches to main at the next three-page module. No SDK code.
    let rtld = nso::build(&0x14000c00u32.to_le_bytes(), b"rtld", b"data", 0);
    let main = nso::build(&0xd40000e1u32.to_le_bytes(), b"main", b"data", 0);
    let auxiliary = nso::build(&0xd65f03c0u32.to_le_bytes(), b"own module", b"data", 64);
    let files: Vec<(&str, &[u8])> = vec![
        ("rtld", &rtld),
        ("main", &main),
        ("subsdk0", &auxiliary),
        ("sdk", &auxiliary),
        ("main.npdm", &metadata),
    ];
    let made = fixture::program_nca(&files, false);
    let rom = romfs::build(&[("/fixture.txt", b"own RomFS bytes")]);
    let control = fixture::control_nca(&rom);
    let mut header = nca::decrypt_header(&made.image[..0xc00], &made.keys).unwrap();
    let rom_header = nca::decrypt_header(&control.image[..0xc00], &control.keys).unwrap();
    let exefs = pfs0::build(&files);
    // Assemble two unencrypted synthetic sections; only their header is encrypted
    // with the fixture's made-up key, entirely in memory.
    let mut payloads = Vec::new();
    for (i, data) in [&exefs, &rom].into_iter().enumerate() {
        let start = 0xc00 + payloads.len();
        payloads.extend_from_slice(&[0; 0x200]);
        payloads.extend_from_slice(data);
        payloads.resize(payloads.len().div_ceil(512) * 512, 0);
        let end = 0xc00 + payloads.len();
        let entry = 0x240 + i * 16;
        header[entry..entry + 4].copy_from_slice(&((start / 512) as u32).to_le_bytes());
        header[entry + 4..entry + 8].copy_from_slice(&((end / 512) as u32).to_le_bytes());
        if i == 1 {
            header[0x600..0x800].copy_from_slice(&rom_header[0x400..0x600]);
        }
        header[0x400 + i * 0x200 + 4] = 1;
    }
    header[0x208..0x210].copy_from_slice(&((0xc00 + payloads.len()) as u64).to_le_bytes());
    crypto::nintendo_xts_encrypt(&made.keys.get32("header_key").unwrap(), &mut header, 512, 0);
    header.extend_from_slice(&payloads);
    let files = [("own-program.nca", header.as_slice())];
    let package = if ext == "nsp" {
        pfs0::build(&files)
    } else {
        soju_switch_fs::xci::build(&files, false)
    };
    (package, made.keys, rom)
}
