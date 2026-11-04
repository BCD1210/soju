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
    std::fs::write(dir.join("Kart.nsp"), pfs0::build(&[("a.nca", &prog.image), ("b.nca", &ctrl.image), ("c.cnmt.nca", &meta.image)])).unwrap();
}
