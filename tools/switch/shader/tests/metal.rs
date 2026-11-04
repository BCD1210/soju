mod common;
use common::*;
use soju_switch_shader::{msl::Options, translate};
use std::{fs, path::PathBuf, process::Command};

fn compile(name: &str, binary: &[u8]) -> String {
    let shader = translate(binary, &Options::default()).unwrap();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/metal-fixtures");
    fs::create_dir_all(&dir).unwrap();
    let source = dir.join(format!("{name}.metal"));
    let object = dir.join(format!("{name}.air"));
    fs::write(&source, &shader.source).unwrap();
    let tmp = dir.join("tmp");
    fs::create_dir_all(&tmp).unwrap();
    let output = Command::new("xcrun")
        .args([
            "-sdk",
            "macosx",
            "metal",
            "-std=metal3.0",
            "-fno-fast-math",
            "-ffp-contract=off",
            "-c",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&object)
        .env("TMPDIR", &tmp)
        .output();
    match output {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP {name}: xcrun is not installed")
        }
        Err(e) => panic!("cannot launch xcrun: {e}"),
        Ok(o) => {
            assert!(
                o.status.success(),
                "Metal compile failed for {}:\n{}\n{}",
                source.display(),
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            );
            assert!(fs::metadata(object).unwrap().len() > 0);
            eprintln!("Metal compiled {name}");
        }
    }
    shader.source
}
#[test]
fn passthrough_vertex() {
    let mut a = Assembler::default();
    a.emit(attr(false, 0x80, 0, 4));
    a.emit(attr(true, 0x70, 0, 4));
    a.emit(EXIT);
    let s = compile("passthrough_vertex", &a.finish(vertex_header()));
    assert!(s.contains("float4 a0 [[attribute(0)]]"));
    assert!(s.contains("out.position[3] = as_type<float>(r[3])"));
}
#[test]
fn textured_fragment_with_cbuf_color() {
    let mut h = fragment_header();
    h[6] = 2 | (2 << 2);
    let mut a = Assembler::default();
    a.emit(ipa(4, 0x80));
    a.emit(ipa(5, 0x84));
    // TEX 2D RGBA, coordinates R4:R5, descriptor pair 1, result R0:R3.
    a.emit(0xc038000000000000 | PT | 4 << 8 | 255 << 20 | 1 << 29 | 15 << 31 | 1 << 36);
    for c in 0..4 {
        a.emit(0x4c68000000000000 | PT | c | c << 8 | c << 20 | 2 << 34);
    }
    a.emit(EXIT);
    let s = compile("textured_fragment", &a.finish(h));
    assert!(s.contains("constant uint* cb2 [[buffer(2)]]"));
    assert!(s.contains("resources.images[handle].sample"));
    assert!(s.contains("cb2[3]"));
}
#[test]
fn bindless_texture_level_zero() {
    let mut a = Assembler::default();
    a.emit(mov32(6, 3));
    a.emit(0xdeb8000000000000 | PT | 4 << 8 | 6 << 20 | 1 << 29 | 15 << 31 | 1 << 37);
    a.emit(EXIT);
    let s = compile("bindless_texture", &a.finish(fragment_header()));
    assert!(s.contains("uint handle = r[6]"));
    assert!(s.contains("level(0.0f)"));
}
#[test]
fn loop_with_reconvergence() {
    let mut a = Assembler::default();
    a.branch(0xe290000000000000, "end");
    a.emit(mov32(0, 0));
    a.label("loop");
    a.emit(0x3810000000000000 | PT | 1 << 20);
    // ISETP.LT.U32.AND P0, PT, R0, 4, PT
    a.emit(0x3660000000000000 | PT | 7 | 4 << 20 | 7 << 39 | 1 << 49);
    a.branch(0xe24000000000000f, "loop");
    a.emit(0xf0f800000000000f | PT);
    a.label("end");
    a.emit(EXIT);
    let s = compile("loop_ssy_sync", &a.finish(fragment_header()));
    assert!(s.contains("while (true)"));
    assert!(s.contains("if (p[0]) { continue; }"));
}
#[test]
fn loop_with_break_stack() {
    let mut a = Assembler::default();
    a.branch(0xe2a0000000000000, "end");
    a.label("loop");
    a.emit(0x3810000000000000 | PT | 1 << 20);
    a.emit(0x3660000000000000 | PT | 7 | 4 << 20 | 7 << 39 | 6 << 49);
    a.emit(0xe34000000000000f);
    a.branch(0xe24000000000000f | PT, "loop");
    a.label("end");
    a.emit(EXIT);
    let s = compile("loop_pbk_brk", &a.finish(fragment_header()));
    assert!(s.contains("if (p[0]) { break; }"));
}
#[test]
fn predicated_exit_and_kill() {
    let mut a = Assembler::default();
    a.emit(0xe33000000000000f);
    a.emit(0xe30000000009000f);
    a.emit(EXIT);
    let s = compile("predicated_termination", &a.finish(fragment_header()));
    assert!(s.contains("if (p[0])"));
    assert!(s.contains("discard_fragment()"));
    assert!(s.contains("if ((!p[1]))"));
}
#[test]
fn arithmetic_and_local_memory() {
    let mut a = Assembler::default();
    a.emit(mov32(0, 1f32.to_bits()));
    a.emit(mov32(1, 2f32.to_bits()));
    a.emit(alu(0x5c58000000000000, 2, 0, 1));
    a.emit(alu(0x5c68000000000000, 3, 0, 1));
    a.emit(alu(0x5980000000000000, 4, 0, 1) | 2 << 39);
    a.emit(alu(0x5c60000000000000, 5, 0, 1) | 7 << 39);
    a.emit(alu(0x5c10000000000000, 6, 0, 1));
    a.emit(alu(0x5cc0000000000000, 7, 0, 1) | 2 << 39);
    a.emit(alu(0x5c18000000000000, 8, 0, 1) | 2 << 39);
    a.emit(alu(0x5a00000000000000, 9, 0, 1));
    a.emit(alu(0x5b00000000000000, 10, 0, 1));
    a.emit(alu(0x5c48000000000000, 11, 0, 1));
    a.emit(alu(0x5c28000000000000, 12, 0, 1));
    a.emit(alu(0x5c00000000000000, 13, 0, 1));
    a.emit(alu(0x5bf0000000000000, 14, 0, 1) | 2 << 39);
    a.emit(alu(0x5c08000000000000, 15, 0, 1));
    a.emit(alu(0x5c30000000000000, 16, 0, 1));
    a.emit(alu(0x5c40000000000000, 17, 0, 1) | 7 << 48);
    a.emit(alu(0x5be0000000000000, 18, 0, 1) | 2 << 39 | 7 << 48 | 0x96 << 28);
    a.emit(alu(0x5ca0000000000000, 19, 0, 1) | 7 << 39);
    a.emit(alu(0x5c20000000000000, 20, 0, 1) | 7 << 39);
    a.emit(alu(0x5cb8000000000000, 21, 0, 1) | 0xa00);
    a.emit(alu(0x5cb0000000000000, 22, 0, 1) | 0xa00);
    a.emit(alu(0x5ca8000000000000, 23, 0, 1) | 0xa00);
    a.emit(alu(0x5ce0000000000000, 24, 0, 1) | 0xa00);
    a.emit(alu(0x5080000000000000, 25, 0, 2));
    a.emit(0xef54000000000000 | PT | 25 | 255 << 8);
    a.emit(0xef44000000000000 | PT | 26 | 255 << 8);
    a.emit(0xef94000000000000 | PT | 27 | 255 << 8 | 1 << 36);
    a.emit(EXIT);
    let mut h = fragment_header();
    h[1] = 4;
    compile("arithmetic_local", &a.finish(h));
}

#[test]
fn comparisons_and_predicate_logic() {
    let mut a = Assembler::default();
    a.emit(mov32(2, 1f32.to_bits()));
    a.emit(mov32(3, 2f32.to_bits()));
    a.emit(alu(0x5800000000000000, 0, 2, 3) | 7 << 39 | 1 << 48 | 1 << 52);
    a.emit(alu(0x5bb0000000000000, 7, 2, 3) | 7 << 39 | 1 << 48);
    a.emit(alu(0x5b50000000000000, 1, 2, 3) | 7 << 39 | 1 << 49 | 1 << 44);
    a.emit(0x5090000000000000 | PT | 15 | 7 << 29 | 7 << 39);
    a.emit(EXIT);
    let s = compile("comparisons_predicates", &a.finish(fragment_header()));
    assert!(s.contains("isnan("));
    assert!(s.contains("bool yes"));
}

#[test]
fn vertex_id_is_integer_builtin_without_vertex_buffer() {
    let mut h = vertex_header();
    h[6] = 0;
    h[10] = 1 << 31;
    let mut a = Assembler::default();
    a.emit(attr(false, 0x2fc, 0, 1));
    a.emit(attr(true, 0x70, 0, 1));
    a.emit(EXIT);
    let s = compile("vertex_id", &a.finish(h));
    assert!(s.contains("uint soju_vertex_id [[vertex_id]]"));
    assert!(s.contains("r[0] = soju_vertex_id;"));
    assert!(!s.contains("[[attribute("));
}
#[test]
fn indexed_constant_load_requires_bounds_and_checks_alignment_and_overflow() {
    let mut a = Assembler::default();
    a.emit(0xef94001003070700);
    a.emit(EXIT);
    let binary = a.finish(vertex_header());
    assert!(translate(&binary, &Options::default()).is_err());
    let mut options = Options::default();
    options.constant_buffer_bytes[1] = 96;
    let source = translate(&binary, &options).unwrap().source;
    assert!(source.contains("ulong(r[7]) + 48ul"));
    assert!(source.contains("& 3ul)==0ul"));
    assert!(source.contains("<=92ul"));
    assert!(source.contains(": 0u"));
}
