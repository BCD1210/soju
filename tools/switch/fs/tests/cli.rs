//! End-to-end: the command reads a synthetic NSP and XCI and never prints key material.
use soju_switch_fs::nca::fixture::program_nca;
use soju_switch_fs::{pfs0, xci};
use std::process::Command;

fn run(dir: &std::path::Path, file: &str, title: bool) -> (bool, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_soju-switch-fs"));
    c.arg("info").arg(dir.join(file)).arg("--keys").arg(dir.join("prod.keys"));
    if title {
        c.arg("--title-keys").arg(dir.join("title.keys"));
    }
    let o = c.output().unwrap();
    (o.status.success(), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

#[test]
fn nsp_xci_and_title_key_without_leaking_keys() {
    let dir = std::env::temp_dir().join(format!("soju-switch-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let npdm = soju_switch_fs::npdm::build("Application", 0x0100000000010000, &["hid"], &[]);
    let m = program_nca(&[("main", b"code"), ("main.npdm", &npdm)], false);
    std::fs::write(dir.join("prod.keys"), &m.prod_keys).unwrap();
    std::fs::write(dir.join("game.nsp"), pfs0::build(&[("aabbccdd.nca", &m.image), ("x.tik", b"t")])).unwrap();
    std::fs::write(dir.join("game.xci"), xci::build(&[("aabbccdd.nca", &m.image)], true)).unwrap();
    let secrets: Vec<String> = m.prod_keys.lines().map(|l| l.split('=').nth(1).unwrap().trim().to_string()).collect();

    for f in ["game.nsp", "game.xci"] {
        let (ok, out, err) = run(&dir, f, false);
        assert!(ok, "{err}");
        assert!(out.contains("\"content\":\"program\""), "{out}");
        assert!(out.contains("\"program_id\":\"0100000000010000\""), "{out}");
        assert!(out.contains("\"main.npdm\""), "{out}");
        assert!(out.contains("\"npdm\":{\"name\":\"Application\""), "{out}");
        for s in &secrets {
            assert!(!out.contains(s.as_str()) && !err.contains(s.as_str()));
        }
    }

    let t = program_nca(&[("main", b"hi")], true);
    std::fs::write(dir.join("prod.keys"), &t.prod_keys).unwrap();
    std::fs::write(dir.join("title.keys"), &t.title_keys).unwrap();
    std::fs::write(dir.join("t.nsp"), pfs0::build(&[("t.nca", &t.image)])).unwrap();
    let (ok, out, _) = run(&dir, "t.nsp", true);
    assert!(ok && out.contains("\"title_key\":true") && out.contains("\"main\""), "{out}");
    let (ok, out, _) = run(&dir, "t.nsp", false);
    assert!(ok && out.contains("rights id"), "{out}");

    std::fs::write(dir.join("prod.keys"), "header_key = nothex\n").unwrap();
    let (ok, _, err) = run(&dir, "game.nsp", false);
    assert!(!ok && err.contains("key line") && !err.contains("nothex"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
