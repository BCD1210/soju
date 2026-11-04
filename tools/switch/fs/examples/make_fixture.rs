//! Writes a synthetic NSP (made-up keys, no Nintendo data) and its prod.keys to a folder,
//! for the launcher's Python tests: make_fixture <dir>
use soju_switch_fs::nca::fixture::{control_nca, meta_nca, program_nca};
use soju_switch_fs::{cnmt, nacp, pfs0, romfs};

fn main() {
    let dir = std::path::PathBuf::from(std::env::args().nth(1).expect("usage: make_fixture <dir>"));
    let n = nacp::build(&[(0, "Kart Racer", "Example Co"), (12, "카트 레이서", "예시 회사")], "1.0.0");
    let ctrl = control_nca(&romfs::build(&[("/control.nacp", &n), ("/icon_Korean.dat", b"\xff\xd8\xff\xe0KOREAN-ICON")]));
    let prog = program_nca(&[("main", b"code")], false);
    let cn = cnmt::build(0x0100_0000_0001_0000, 0, 0x80, None, &[]);
    let meta = meta_nca("Application_0100000000010000.cnmt", &cn);
    std::fs::write(dir.join("prod.keys"), &ctrl.prod_keys).unwrap();
    // A tiny firmware dump: SystemVersion data + its meta, and a font meta.
    let fw = dir.join("firmware");
    std::fs::create_dir_all(&fw).unwrap();
    let mut file = vec![0u8; 0x100];
    file[0x68..0x6E].copy_from_slice(b"18.1.0");
    let data = control_nca(&romfs::build(&[("/file", &file)]));
    let sv = cnmt::build(0x0100000000000809, 0x12100000, 0x01, None, &[([0xAA; 16], data.image.len() as u64, 2)]);
    std::fs::write(fw.join("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.nca"), &data.image).unwrap();
    std::fs::write(fw.join("11111111111111111111111111111111.cnmt.nca"), &meta_nca("SystemData_0100000000000809.cnmt", &sv).image).unwrap();
    let font = cnmt::build(0x0100000000000811, 0, 0x01, None, &[]);
    std::fs::write(fw.join("22222222222222222222222222222222.cnmt.nca"), &meta_nca("SystemData_0100000000000811.cnmt", &font).image).unwrap();
    std::fs::write(dir.join("Kart.nsp"), pfs0::build(&[("a.nca", &prog.image), ("b.nca", &ctrl.image), ("c.cnmt.nca", &meta.image)])).unwrap();
}
