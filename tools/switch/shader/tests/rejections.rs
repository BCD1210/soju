mod common;
use common::*;
use soju_switch_shader::{decoder::decode, ir::Program, msl::Options, translate};
fn build(a: Assembler) -> soju_switch_shader::Result<Program> {
    let b = a.finish(fragment_header());
    Program::build(&decode(&b[80..])?)
}
#[test]
fn rejects_stack_disagreement_and_crossed_tokens() {
    let mut a = Assembler::default();
    a.branch(0xe24000000000000f, "merge");
    a.branch(0xe290000000000000, "exit");
    a.label("merge");
    a.emit(NOP);
    a.label("exit");
    a.emit(EXIT);
    assert!(build(a)
        .unwrap_err()
        .0
        .contains("inconsistent control stack"));
    let mut a = Assembler::default();
    a.branch(0xe2a0000000000000, "end");
    a.emit(0xf0f800000000000f | PT);
    a.label("end");
    a.emit(EXIT);
    assert!(build(a).unwrap_err().0.contains("crossed"));
}
#[test]
fn rejects_irreducible_multi_entry_loop() {
    let mut a = Assembler::default();
    a.branch(0xe24000000000000f, "interior");
    a.label("loop");
    a.emit(NOP);
    a.label("interior");
    a.emit(NOP);
    a.branch(0xe24000000000000f, "loop");
    a.emit(EXIT);
    assert!(build(a).unwrap_err().0.contains("loop interior"));
}
#[test]
fn rejects_indirect_branch_and_scheduling_target_outside_code() {
    let mut a = Assembler::default();
    a.emit(0xe24000000000002f | PT);
    a.emit(EXIT);
    assert!(build(a).unwrap_err().0.contains("indirect"));
    let mut a = Assembler::default();
    a.emit(0xe24000000100000f | PT);
    a.emit(EXIT);
    assert!(build(a).unwrap_err().0.contains("outside code"));
}
#[test]
fn rejects_missing_attribute_and_resource_collision() {
    let mut a = Assembler::default();
    a.emit(attr(false, 0x90, 0, 1));
    a.emit(EXIT);
    let error = translate(&a.finish(vertex_header()), &Options::default()).unwrap_err();
    assert!(error.0.contains("absent from SPH"));
    let mut a = Assembler::default();
    a.emit(0xc038000000000000 | PT | 255 << 20 | 1 << 29 | 15 << 31);
    a.emit(0x4c98000000000000 | PT | 15 << 39 | 30 << 34);
    a.emit(EXIT);
    assert!(translate(&a.finish(fragment_header()), &Options::default())
        .unwrap_err()
        .0
        .contains("collides"));
}
#[test]
fn rejects_unsupported_geometry_and_numeric_modes() {
    let mut h = vertex_header();
    h[0] = 1 | (4 << 10);
    let mut a = Assembler::default();
    a.emit(EXIT);
    assert!(translate(&a.finish(h), &Options::default())
        .unwrap_err()
        .0
        .contains("stages"));
    let mut a = Assembler::default();
    a.emit(alu(0x5c58000000000000, 0, 1, 2) | 1 << 39);
    a.emit(EXIT);
    assert!(translate(&a.finish(fragment_header()), &Options::default())
        .unwrap_err()
        .0
        .contains("mode"));
}
#[test]
fn forward_selection_is_structured() {
    let mut a = Assembler::default();
    a.branch(0xe24000000000000f, "end");
    a.emit(mov32(0, 1));
    a.label("end");
    a.emit(EXIT);
    let p = build(a).unwrap();
    assert!(
        matches!(&p.body[0],soju_switch_shader::ir::Node::If {condition,body} if condition.inverted && body.len()==1)
    );
}

#[test]
fn excessive_structured_nesting_is_an_error() {
    let mut a = Assembler::default();
    for i in 0..66 {
        let target = 132 - i;
        let target_address = ((target + target / 3 + 1) * 8) as i64;
        let delta = target_address - i64::from(a.address()) - 8;
        a.emit(0xe24000000000000f | ((delta as u64) << 20));
    }
    for _ in 0..67 {
        a.emit(NOP);
    }
    a.emit(EXIT);
    assert!(build(a).unwrap_err().0.contains("nesting exceeds"));
}
#[test]
fn reserved_header_bits_are_preserved_without_inventing_attributes() {
    let mut h = vertex_header();
    h[13] = 0;
    h[18] = 1 << 12;
    let bytes: Vec<_> = h.into_iter().flat_map(u32::to_le_bytes).collect();
    let h = soju_switch_shader::header::Header::parse(&bytes).unwrap();
    assert!(h.outputs.is_empty());
    assert_eq!(h.omap[5], 1 << 12);
}

#[test]
fn unreachable_resources_do_not_create_bindings_or_collisions() {
    let mut a = Assembler::default();
    a.emit(EXIT);
    a.emit(0xc038000000000000 | PT | 255 << 20 | 1 << 29 | 15 << 31 | 8191 << 36);
    a.emit(0x4c98000000000000 | PT | 15 << 39 | 31 << 34);
    let shader = translate(&a.finish(fragment_header()), &Options::default()).unwrap();
    assert!(!shader.uses_texture_table);
    assert!(shader.constant_buffers.is_empty());
    assert!(shader.texture_bindings.is_empty());
}

#[test]
fn divergent_and_counted_barriers_are_rejected() {
    let mut qmd = [0u32; 64];
    qmd[18] = 0x17 | 32 << 16;
    qmd[19] = 1 | 1 << 16;
    let launch = soju_switch_shader::compute::Launch::parse(
        &qmd.into_iter()
            .flat_map(u32::to_le_bytes)
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let barrier = 0xf0a8008000000000 | PT | 1 << 44 | 1 << 43 | 7 << 39;
    for b in [barrier & !PT, barrier | 32 << 20, barrier | 1 << 8] {
        let mut a = Assembler::default();
        a.emit(b);
        a.emit(EXIT);
        let binary = a.finish(fragment_header());
        assert!(soju_switch_shader::compute::translate(
            &binary[80..],
            &launch,
            &Options::default()
        )
        .is_err());
    }
    let mut a = Assembler::default();
    a.branch(0xe24000000000000f, "end");
    a.emit(barrier);
    a.label("end");
    a.emit(EXIT);
    let binary = a.finish(fragment_header());
    assert!(
        soju_switch_shader::compute::translate(&binary[80..], &launch, &Options::default())
            .unwrap_err()
            .0
            .contains("straight-line")
    );
}

#[test]
fn rejects_all_reachable_out_encodings_and_stream_modes() {
    // Register, immediate and cbuf forms from the pinned ISA table. No guessed
    // export-handle semantics are translated into an apparently valid shader.
    for encoding in [0xfbe0000000000000, 0xf6e0000000000000, 0xebe0000000000000] {
        for mode in 0..4 {
            let mut a = Assembler::default();
            a.emit(encoding | PT | mode << 39);
            a.emit(EXIT);
            let binary = a.finish(vertex_header());
            let error = translate(&binary, &Options::default()).unwrap_err();
            assert!(
                error.0.contains("OUT geometry export/stream semantics"),
                "{error}"
            );
            assert!(error.0.contains("0x8"), "{error}");
        }
    }
}
