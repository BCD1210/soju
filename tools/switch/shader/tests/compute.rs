mod common;
use common::*;
use soju_switch_shader::{
    compute::{self, Launch},
    msl::{GlobalMemory, Options},
};

fn launch() -> Launch {
    let mut words = [0u32; 64];
    words[18] = 0x17 | 32 << 16;
    words[19] = 1 | 1 << 16;
    words[17] = 128;
    Launch::parse(
        &words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
#[test]
fn qmd_fields_and_limits() {
    let l = launch();
    assert_eq!(l.local_size, [32, 1, 1]);
    assert_eq!(l.shared_bytes, 128);
    let mut bytes: Vec<_> = l.raw.into_iter().flat_map(u32::to_le_bytes).collect();
    assert!(Launch::parse(&bytes[..255]).is_err());
    bytes[72] = 0x18;
    assert!(Launch::parse(&bytes).is_err());
    bytes[72] = 0x17;
    bytes[74] = 0;
    assert!(Launch::parse(&bytes).is_err());
}
#[test]
fn memory_predicate_transfers_and_barrier_compile() {
    let mut a = Assembler::default();
    a.emit(0xf0c8000000000000 | PT | 10 | 0x21 << 20);
    a.emit(alu(0x3848000000000000, 10, 10, 2));
    a.emit(0xeed4000000000000 | PT | 1 | 10 << 8);
    a.emit(0x38f0000000000000 | PT | 1 << 8 | 0x7f << 20);
    a.emit(0x38e8000000000000 | PT | 2 | 255 << 8 | 0x7f << 20);
    a.emit(0xef5c000000000000 | PT | 1 | 10 << 8);
    a.emit(0xf0a8008000000000 | PT | 1 << 44 | 1 << 43 | 7 << 39);
    a.emit(0xef4c000000000000 | PT | 3 | 10 << 8);
    a.emit(0xeedc000000000000 | PT | 3 | 10 << 8);
    a.emit(EXIT);
    let binary = a.finish(fragment_header());
    let options = Options {
        global_memory: Some(GlobalMemory {
            buffer: 0,
            base_address: 0,
            bytes: 128,
        }),
        ..Options::default()
    };
    let shader = compute::translate(&binary[80..], &launch(), &options).unwrap();
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target/metal-fixtures/compute_memory.metal");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, shader.source).unwrap();
    if cfg!(target_os = "macos") {
        let o = std::process::Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=metal3.0",
                "-fno-fast-math",
                "-ffp-contract=off",
                "-c",
            ])
            .arg(&path)
            .arg("-o")
            .arg(path.with_extension("air"))
            .output()
            .unwrap();
        assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    }
}
