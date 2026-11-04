use soju_switch_shader::{
    compute::Launch,
    msl::{GlobalMemory, Options},
};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};
pub fn launch(width: u32, shared: u32) -> Launch {
    let mut words = [0u32; 64];
    words[18] = 0x17 | width << 16;
    words[19] = 1 | 1 << 16;
    words[17] = shared;
    Launch::parse(
        &words
            .into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
pub fn options(words: usize) -> Options {
    Options {
        global_memory: Some(GlobalMemory {
            buffer: 0,
            base_address: 0,
            bytes: (words * 4) as u32,
        }),
        ..Options::default()
    }
}
pub fn command(c: &mut Command) {
    let output = c.output().unwrap();
    assert!(
        output.status.success(),
        "{c:?}\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
pub fn setup() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(build_runner).clone()
}
fn build_runner() -> PathBuf {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dir = root.join("target/gpu-equivalence");
    fs::create_dir_all(&dir).unwrap();
    command(
        Command::new("xcrun")
            .arg("swiftc")
            .arg(root.join("tests/metal_runner.swift"))
            .arg("-o")
            .arg(dir.join("runner")),
    );
    dir
}
pub fn execute(dir: &Path, name: &str, source: &str, values: &[u32], width: u32) -> Vec<u32> {
    let path = dir.join(name);
    fs::write(path.with_extension("metal"), source).unwrap();
    command(
        Command::new("xcrun")
            .args([
                "-sdk",
                "macosx",
                "metal",
                "-std=metal3.0",
                "-fno-fast-math",
                "-ffp-contract=off",
                "-c",
            ])
            .arg(path.with_extension("metal"))
            .arg("-o")
            .arg(path.with_extension("air")),
    );
    command(
        Command::new("xcrun")
            .arg("metallib")
            .arg(path.with_extension("air"))
            .arg("-o")
            .arg(path.with_extension("metallib")),
    );
    fs::write(
        path.with_extension("input"),
        values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<_>>(),
    )
    .unwrap();
    command(
        Command::new(dir.join("runner"))
            .arg(path.with_extension("metallib"))
            .arg(path.with_extension("input"))
            .arg(path.with_extension("output"))
            .arg((values.len() / 8).to_string())
            .arg(width.to_string())
            .arg("soju_compute"),
    );
    fs::read(path.with_extension("output"))
        .unwrap()
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
