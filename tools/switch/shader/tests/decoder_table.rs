use soju_switch_shader::decoder::{decode_instruction, Opcode, Operand};

/// Known opcode/mask representatives from the pinned gm107.c root table.
/// These literals are independent of the generated production table. They test
/// classification and operand extraction, not claims of hardware execution.
#[test]
fn every_requested_class_has_a_reference_encoding() {
    use Opcode::*;
    let cases = [
        (0x5980, FFMA),
        (0x5c58, FADD),
        (0x5c68, FMUL),
        (0x5c60, FMNMX),
        (0x5800, FSET),
        (0x5bb0, FSETP),
        (0x5080, MUFU),
        (0x5ca8, F2F),
        (0x5cb0, F2I),
        (0x5cb8, I2F),
        (0x5ce0, I2I),
        (0x5c10, IADD),
        (0x5cc0, IADD3),
        (0x5c18, ISCADD),
        (0x5a00, IMAD),
        (0x5b00, XMAD),
        (0x5b50, ISET),
        (0x5b60, ISETP),
        (0x5c20, IMNMX),
        (0x5c40, LOP),
        (0x5be0, LOP3),
        (0x5c48, SHL),
        (0x5c28, SHR),
        (0x5c00, BFE),
        (0x5bf0, BFI),
        (0x5c08, POPC),
        (0x5c30, FLO),
        (0x5ca0, SEL),
        (0x5c98, MOV),
        (0x5090, PSETP),
        (0x5ce8, P2R),
        (0x5cf0, R2P),
        (0x8000, LD),
        (0xa000, ST),
        (0xeed0, LDG),
        (0xeed8, STG),
        (0xef48, LDS),
        (0xef58, STS),
        (0xef40, LDL),
        (0xef50, STL),
        (0xef90, LDC),
        (0xefd8, ALD),
        (0xeff0, AST),
        (0xfbe0, OUT),
        (0xc038, TEX),
        (0xd000, TEXS),
        (0xdc38, TLD),
        (0xd200, TLDS),
        (0xc838, TLD4),
        (0xdf48, TXQ),
        (0xe240, BRA),
        (0xe290, SSY),
        (0xf0f8, SYNC),
        (0xe2a0, PBK),
        (0xe340, BRK),
        (0xe300, EXIT),
        (0xe330, KIL),
        (0x50b0, NOP),
        (0xf0f0, DEPBAR),
        (0xf0a8, BAR),
    ];
    for (prefix, expected) in cases {
        let raw = (prefix as u64) << 48 | 0x70000;
        assert_eq!(
            decode_instruction(8, raw).unwrap().opcode,
            expected,
            "{raw:016x}"
        );
    }
    assert_eq!(
        decode_instruction(8, 0xe00000000007ff00).unwrap().opcode,
        IPA
    );
}
#[test]
fn operands_predicates_and_constant_swapped_forms() {
    let i = decode_instruction(0x88, 0x5c580000004b0302).unwrap();
    assert_eq!(
        i.operands,
        [
            Operand::Register(2),
            Operand::Register(3),
            Operand::Register(4)
        ]
    );
    assert_eq!(i.predicate.index, 3);
    assert!(i.predicate.inverted);
    // FFMA's constant-third form uses R39 for B, C34 for C.
    let i = decode_instruction(8, 0x5180000000070201 | 5 << 39 | 3 << 34 | 4 << 20).unwrap();
    assert_eq!(
        i.operands,
        [
            Operand::Register(1),
            Operand::Register(2),
            Operand::Register(5),
            Operand::Constant {
                slot: 3,
                base: 255,
                offset: 16
            }
        ]
    );
    let i = decode_instruction(0x28, 0xe2400ffffe07000f).unwrap();
    assert_eq!(i.target(), Some(0x10));
    let i = decode_instruction(8, 0x0103f8000007f002).unwrap();
    assert_eq!(i.opcode, Opcode::MOV);
    assert_eq!(i.operands[1], Operand::Immediate(0x3f800000));
}

#[test]
fn bar_operand_selectors_and_memory_spaces() {
    use soju_switch_shader::decoder::{Predicate, Space};
    let i = decode_instruction(
        8,
        0xf0a8000000070300 | 7 << 39 | 1 << 44 | 1 << 43 | 128 << 20,
    )
    .unwrap();
    assert_eq!(
        i.operands,
        [
            Operand::Immediate(3),
            Operand::Immediate(128),
            Operand::Predicate(Predicate::ALWAYS)
        ]
    );
    let i = decode_instruction(8, 0xef4000000007ff02 | 0xfffffc << 20).unwrap();
    assert_eq!(
        i.operands,
        [
            Operand::Register(2),
            Operand::Memory {
                space: Space::Local,
                base: 255,
                offset: -4
            }
        ]
    );
    let i = decode_instruction(8, 0xef90000000070302 | 5 << 36 | 16 << 20).unwrap();
    assert_eq!(
        i.operands,
        [
            Operand::Register(2),
            Operand::Constant {
                slot: 5,
                base: 3,
                offset: 16
            }
        ]
    );
}

#[test]
fn input_buffer_read_preserves_address_register_and_modes() {
    let x = decode_instruction(8, 0xefd0000000000000 | 7 << 16 | 4 | 12 << 8 | 2 << 33).unwrap();
    assert_eq!(x.opcode, Opcode::ISBERD);
    assert_eq!(x.operands, [Operand::Register(4), Operand::Register(12)]);
    assert!(x
        .options
        .iter()
        .any(|o| o.name == "efd0_0" && o.bits == 2 << 33));
}
