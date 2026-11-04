mod common;
use common::*;
use soju_switch_shader::{
    decoder,
    ir::Program,
    msl::Options,
    reference::{Machine, Memory, Sampler, Texture},
    texture::{Binding, Handle, TextureType},
    translate,
};
use std::{fs, path::PathBuf, process::Command};
fn command(c: &mut Command) {
    let o = c.output().unwrap();
    assert!(
        o.status.success(),
        "{c:?}\n{}",
        String::from_utf8_lossy(&o.stderr)
    );
}
fn bindings() -> Vec<Binding> {
    vec![
        Binding {
            handle: Handle::Bound(5),
            texture_index: 1,
            sampler_index: 0,
            texture_type: TextureType::D2,
        },
        Binding {
            handle: Handle::Bound(8),
            texture_index: 0,
            sampler_index: 1,
            texture_type: TextureType::D2,
        },
        Binding {
            handle: Handle::Bindless(0x1234),
            texture_index: 1,
            sampler_index: 0,
            texture_type: TextureType::D2,
        },
    ]
}
#[test]
fn handle_conflicts_and_independent_sampler_indices() {
    let mut b = bindings();
    assert_eq!(
        soju_switch_shader::texture::normalize(&b, 2).unwrap().len(),
        3
    );
    b.push(Binding {
        sampler_index: 1,
        ..b[0]
    });
    assert!(soju_switch_shader::texture::normalize(&b, 2).is_err());
    assert!(soju_switch_shader::texture::normalize(&bindings(), 1).is_err());
}
#[test]
#[cfg(target_os = "macos")]
fn texture_families_render_against_reference() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/texture-equivalence");
    fs::create_dir_all(&dir).unwrap();
    command(
        Command::new("xcrun")
            .arg("swiftc")
            .arg(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/texture_runner.swift"))
            .arg("-o")
            .arg(dir.join("runner")),
    );
    let mut vertex = Assembler::default();
    vertex.emit(attr(false, 0x80, 0, 4));
    vertex.emit(attr(true, 0x70, 0, 4));
    vertex.emit(EXIT);
    let binary = vertex.finish(vertex_header());
    let translated = translate(&binary, &Options::default()).unwrap();
    let path = dir.join("vertex");
    fs::write(path.with_extension("metal"), translated.source).unwrap();
    command(
        Command::new("xcrun")
            .args([
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
    let tex = |base: u64, d: u64, h: u64| {
        base | PT | d | 4 << 8 | 255 << 20 | 1 << 29 | 15 << 31 | h << 36
    };
    let cases = vec![
        (
            "two_textures",
            vec![
                tex(0xc038000000000000, 0, 5),
                tex(0xc038000000000000, 8, 8),
                alu(0x5c58000000000000, 0, 0, 8),
                alu(0x5c58000000000000, 1, 1, 9),
                alu(0x5c58000000000000, 2, 2, 10),
                alu(0x5c58000000000000, 3, 3, 11),
            ],
            false,
        ),
        (
            "bindless",
            vec![
                mov32(6, 0x1234),
                0xdeb8000000000000 | PT | 4 << 8 | 6 << 20 | 1 << 29 | 15 << 31,
            ],
            false,
        ),
        (
            "texs",
            vec![
                0xd000000000000000
                    | PT
                    | 4 << 8
                    | 5 << 20
                    | 255 << 28
                    | 5 << 36
                    | 1 << 53
                    | 4 << 50,
            ],
            false,
        ),
        ("tld", vec![tex(0xdc38000000000000, 0, 5)], true),
        (
            "tlds",
            vec![
                0xd200000000000000
                    | PT
                    | 4 << 8
                    | 5 << 20
                    | 255 << 28
                    | 5 << 36
                    | 2 << 53
                    | 4 << 50,
            ],
            true,
        ),
        ("gather", vec![tex(0xc838000000000000, 0, 5)], false),
        (
            "txq",
            vec![
                0xdf48000000000000 | PT | 4 << 8 | 1 << 22 | 15 << 31 | 5 << 36,
                alu(0x5cb8000000000000, 0, 0, 0) | 0xa00,
                alu(0x5cb8000000000000, 1, 0, 1) | 0xa00,
                alu(0x5cb8000000000000, 2, 0, 2) | 0xa00,
                alu(0x5cb8000000000000, 3, 0, 3) | 0xa00,
            ],
            true,
        ),
    ];
    for (name, ops, integer) in cases {
        let mut a = Assembler::default();
        a.emit(mov32(
            4,
            if integer {
                0
            } else {
                if name == "gather" {
                    0.5f32.to_bits()
                } else {
                    1.25f32.to_bits()
                }
            },
        ));
        a.emit(mov32(
            5,
            if integer {
                0
            } else {
                if name == "gather" {
                    0.5f32.to_bits()
                } else {
                    0.25f32.to_bits()
                }
            },
        ));
        for op in ops {
            a.emit(op);
        }
        a.emit(EXIT);
        let binary = a.finish(fragment_header());
        let options = Options {
            texture_count: 2,
            texture_bindings: bindings(),
            ..Options::default()
        };
        let shader = translate(&binary, &options).unwrap();
        assert_eq!(shader.texture_bindings.len(), 3);
        let p = Program::build(&decoder::decode(&binary[80..]).unwrap()).unwrap();
        let mut memory = Memory {
            textures: (0..2)
                .map(|i| Texture {
                    width: 2,
                    height: 2,
                    texels: (0..4)
                        .map(|t| std::array::from_fn(|c| (i * 100 + t * 4 + c + 1) as f32))
                        .collect(),
                })
                .collect(),
            samplers: vec![Sampler::ClampNearest, Sampler::RepeatNearest],
            bindings: bindings(),
            ..Memory::default()
        };
        let mut cpu = Machine::default();
        cpu.run(&p, &mut memory).unwrap();
        let path = dir.join(name);
        fs::write(path.with_extension("metal"), shader.source).unwrap();
        command(
            Command::new("xcrun")
                .args([
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
        command(
            Command::new(dir.join("runner"))
                .arg(path.with_extension("metallib"))
                .arg(path.with_extension("output"))
                .arg(dir.join("vertex.metallib")),
        );
        let result = fs::read(path.with_extension("output")).unwrap();
        for (i, chunk) in result.chunks_exact(4).enumerate() {
            let actual = u32::from_le_bytes(chunk.try_into().unwrap());
            assert_eq!(
                actual,
                cpu.registers[i % 4],
                "{name} pixel {} component {}",
                i / 4,
                i % 4
            );
        }
    }
    eprintln!("GPU texture equivalence: 7 programs × 16 pixels = 112 cases");
}

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
        let mut a = Assembler::default();
        a.emit(
            0xc038000000000000
                | PT
                | 4 << 8
                | 255 << 20
                | dimension << 29
                | 15 << 31
                | u64::from(array) << 28
                | u64::from(shadow) << 50,
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
        let path = dir.join(name).with_extension("metal");
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
