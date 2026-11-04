//! Package only self-built NSOs with fresh synthetic encryption keys.
use soju_switch_fs::{
    crypto,
    keys::KeySet,
    nca::{self, fixture},
    npdm, nso, pfs0,
};
use std::{
    io::{Read, Write},
    os::unix::fs::OpenOptionsExt,
    path::PathBuf,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let dir = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("output directory required")?,
    );
    let main = std::fs::read(dir.join("storage.nso"))?;
    let rom = std::fs::read(dir.join("probe.romfs"))?;
    let mut caps = vec![7 | (63 << 4) | (3 << 24)];
    for group in 0..8 {
        caps.push(15 | (0xffffff << 5) | (group << 29));
    }
    let mut metadata = npdm::build(
        "Soju storage probe",
        0x0100000000010000,
        &["fsp-srv", "set:sys", "arp:r"],
        &caps,
    );
    metadata[12] = 7;
    let rtld = nso::build(&0x14000c00u32.to_le_bytes(), b"Soju rtld", b"data", 0);
    let files: Vec<(&str, &[u8])> =
        vec![("rtld", &rtld), ("main", &main), ("main.npdm", &metadata)];
    let template = fixture::program_nca(&files, false);
    let rom_template = fixture::control_nca(&rom);
    let mut header = nca::decrypt_header(&template.image[..0xc00], &template.keys)?;
    let rom_header = nca::decrypt_header(&rom_template.image[..0xc00], &rom_template.keys)?;
    header[0x600..0x800].copy_from_slice(&rom_header[0x400..0x600]);
    let mut random = std::fs::File::open("/dev/urandom")?;
    let mut header_key = [0; 32];
    let mut area_key = [0; 16];
    let mut section_key = [0; 16];
    random.read_exact(&mut header_key)?;
    random.read_exact(&mut area_key)?;
    random.read_exact(&mut section_key)?;
    let mut area = [0; 64];
    area[32..48].copy_from_slice(&section_key);
    crypto::ecb_encrypt(&area_key, &mut area);
    header[0x300..0x340].copy_from_slice(&area);
    let exefs = pfs0::build(&files);
    let mut payload = Vec::new();
    for (i, data) in [&exefs, &rom].into_iter().enumerate() {
        let start = 0xc00 + payload.len();
        let mut section = vec![0; 512];
        section.extend_from_slice(data);
        section.resize(section.len().div_ceil(512) * 512, 0);
        let end = start + section.len();
        let entry = 0x240 + i * 16;
        header[entry..entry + 4].copy_from_slice(&((start / 512) as u32).to_le_bytes());
        header[entry + 4..entry + 8].copy_from_slice(&((end / 512) as u32).to_le_bytes());
        let fs = 0x400 + i * 512;
        let mut upper: [u8; 8] = header[fs + 0x140..fs + 0x148].try_into()?;
        upper.reverse();
        crypto::ctr_apply(&section_key, upper, start as u64, &mut section);
        payload.extend_from_slice(&section);
    }
    header[0x208..0x210].copy_from_slice(&((0xc00 + payload.len()) as u64).to_le_bytes());
    crypto::nintendo_xts_encrypt(&header_key, &mut header, 512, 0);
    header.extend_from_slice(&payload);
    let nacp = std::fs::read(dir.join("storage.nacp"))?;
    let control_rom = soju_switch_fs::romfs::build(&[("/control.nacp", &nacp)]);
    let mut control = rom_header;
    control[0x300..0x340].copy_from_slice(&area);
    let mut section = vec![0; 512];
    section.extend_from_slice(&control_rom);
    section.resize(section.len().div_ceil(512) * 512, 0);
    let end = 0xc00 + section.len();
    control[0x208..0x210].copy_from_slice(&(end as u64).to_le_bytes());
    control[0x244..0x248].copy_from_slice(&((end / 512) as u32).to_le_bytes());
    control[0x498..0x4a0].copy_from_slice(&(control_rom.len() as u64).to_le_bytes());
    let mut upper: [u8; 8] = control[0x540..0x548].try_into()?;
    upper.reverse();
    crypto::ctr_apply(&section_key, upper, 0xc00, &mut section);
    crypto::nintendo_xts_encrypt(&header_key, &mut control, 512, 0);
    control.extend_from_slice(&section);
    let package = pfs0::build(&[("soju-probe.nca", &header), ("soju-control.nca", &control)]);
    let hex = |bytes: &[u8]| bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
    let key_text = format!(
        "header_key = {}\nkey_area_key_application_02 = {}\n",
        hex(&header_key),
        hex(&area_key)
    );
    // Private local file; no key material is printed.
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(dir.join("synthetic.keys"))?;
    file.write_all(key_text.as_bytes())?;
    let keys = KeySet::parse_prod(&key_text)?;
    let loaded = soju_switch_core::program::Program::load(&package, "nsp", &keys)?;
    assert_eq!(loaded.modules.len(), 2);
    assert_eq!(loaded.romfs, rom);
    assert_eq!(loaded.control, Some(nacp));
    std::fs::write(dir.join("storage.nsp"), package)?;
    println!("PASS|PFS0 -> encrypted NCA -> NSO rtld/main + RomFS|fresh local synthetic keys");
    Ok(())
}
