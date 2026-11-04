use super::*;
#[test]
#[cfg(target_os = "macos")]
fn declared_texture_types_compile() {
    use TextureType::*;
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/texture-types");
    fs::create_dir_all(&dir).unwrap();
    for (name, ty, dimension, array, shadow) in [
        ("1d", D1, 0, false, false),
        ("2d", D2, 1, false, false),
        ("3d", D3, 2, false, false),
        ("cube", Cube, 3, false, false),
        ("1da", D1Array, 0, true, false),
        ("2da", D2Array, 1, true, false),
        ("cubea", CubeArray, 3, true, false),
        ("shadow", Shadow2D, 1, false, true),
        ("shadowa", Shadow2DArray, 1, true, true),
    ] {
        for lz in [false, true] {
            let mut a = Assembler::default();
            a.emit(
                0xc038000000000000
                    | PT
                    | 4 << 8
                    | 255 << 20
                    | dimension << 29
                    | 15 << 31
                    | u64::from(array) << 28
                    | u64::from(shadow) << 50
                    | u64::from(lz) << 55,
            );
            a.emit(EXIT);
            let options = Options {
                texture_count: 1,
                texture_bindings: vec![Binding {
                    handle: Handle::Bound(0),
                    texture_index: 0,
                    sampler_index: 0,
                    texture_type: ty,
                }],
                ..Options::default()
            };
            let shader = translate(&a.finish(fragment_header()), &options).unwrap();
            let path = dir.join(format!("{name}_{lz}")).with_extension("metal");
            fs::write(&path, shader.source).unwrap();
            command(
                Command::new("xcrun")
                    .args([
                        "metal",
                        "-std=metal3.0",
                        "-fno-fast-math",
                        "-ffp-contract=off",
                        "-c",
                    ])
                    .arg(&path)
                    .arg("-o")
                    .arg(path.with_extension("air")),
            );
        }
    }
}
