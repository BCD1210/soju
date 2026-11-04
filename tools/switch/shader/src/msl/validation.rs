use super::*;
impl Emitter<'_> {
    pub(super) fn validate(&self, x: &Instruction) -> Result<()> {
        use Opcode::*;
        // Every decoded modifier must be explicitly consumed or safely scheduling-only.
        for m in &x.modifiers {
            let allowed = match m.name {
                "neg" => matches!(
                    x.opcode,
                    FADD | FMUL
                        | FFMA
                        | FMNMX
                        | FSET
                        | FSETP
                        | MUFU
                        | F2F
                        | F2I
                        | IADD
                        | IADD3
                        | ISCADD
                ),
                "abs" => matches!(x.opcode, FADD | FMNMX | FSET | FSETP | MUFU | F2F | F2I),
                "inv" => matches!(x.opcode, LOP | POPC | FLO),
                "sat" => matches!(x.opcode, FADD | FMUL | FFMA | MUFU | F2F | IPA),
                "ftz" => matches!(x.opcode, FADD),
                "bf" => matches!(x.opcode, FSET | ISET),
                "w" => matches!(x.opcode, SHL | SHR),
                "nodep" => matches!(x.opcode, TEX | TEXS | TLD | TLDS | TLD4 | TXQ),
                "array" => matches!(x.opcode, TEX | TLD),
                "dc" => x.opcode == TEX,
                "e" => matches!(x.opcode, LD | ST | LDG | STG),
                _ => false,
            };
            if !allowed {
                return Err(x.error(format!("unsupported modifier {}", m.name)));
            }
        }
        for o in &x.options {
            let allowed = match o.name {
                "5cf0_0" => o.bits == 0,
                "5cf0_1" => true,
                "eed0sz" => o.bits == 4 << 48,
                "a000_1" => o.bits == 4 << 53,
                "eed0_0" | "eed8_0" | "8000_0" | "a000_0" => o.bits == 0,
                "f0a8_0" => o.bits == 1 << 39,
                "f0a8_1" => true,
                "5cb8_2" | "5980_1" | "5c68_1" | "5cc0_0" | "5cc0_1" | "5cc0_2" | "5cc0_3"
                | "5b00_0" | "5b00_1" | "5100_0" | "5c20_0" | "5c40_1" | "5be0_0" | "ef90_0"
                | "ef40_0" | "ef50_0" => o.bits == 0,
                "5c68_0" => o.bits == 0 || o.bits == 1 << 44,
                "5980_0" => o.bits == 0 || o.bits == 1 << 53,
                "5bb0_0" | "5b60_0" | "5c30_0" | "5a00_0" | "5a00_1" | "5c40_0" | "5cb0_1"
                | "5080_0" | "5ca8_0" => true,
                "5bb0_1" => bits(o.bits, 45, 2) < 3,
                "5090_0" => bits(o.bits, 24, 2) < 3,
                "5cb8_0" => o.bits == 0x200,
                "5cb8_1" | "5ce0_1" => o.bits == 0x800 || o.bits == 0x2800,
                "5cb0_0" => o.bits == 0x800,
                "5cb0_2" | "5ce0_0" => o.bits == 0x200 || o.bits == 0x1200,
                "ef90sz" => o.bits == 4 << 48,
                "ef58sz" => o.bits == 4 << 48,
                "eff0_0" => true,
                "e000_0" => o.bits == 0 || o.bits == 2 << 54,
                "e000_1" => o.bits == 0,
                "df60_0" => true,
                "dc38_0" => o.bits == 0,
                "df50_0" => o.bits == 1 << 22,
                "c838_0" | "def8_0" => true,
                "c838_1" | "def8_1" => o.bits == 0,
                "d000_0" | "d000_1" => matches!(bits(o.bits, 53, 4), 1 | 2),
                "d200_0" | "d200_1" => bits(o.bits, 53, 4) == 2,
                "d200_2" => true,
                "c038_0" => o.bits == 0 || o.bits == 1 << 55,
                "deb8_0" => o.bits == 0 || o.bits == 1 << 37,
                "f0f8_0" => o.bits == 15,
                "50b0_0" => o.bits == 0xf00,
                "f0f0_0" => true,
                "b" | "0x0" | "lut" => true,
                _ => false,
            };
            if !allowed {
                return Err(x.error(format!("unsupported mode {} = 0x{:x}", o.name, o.bits)));
            }
        }
        if x.opcode == MOV {
            if let Some(Operand::Immediate(mask)) = x.operands.get(2) {
                if *mask != 15 {
                    return Err(x.error("MOV component mask must be 0xf"));
                }
            }
        }
        if matches!(x.opcode, LOP | LOP3) {
            if !matches!(
                x.operands.first(),
                Some(Operand::Predicate(Predicate { index: 7, .. }))
            ) {
                return Err(x.error("logic predicate destination is not supported"));
            }
            if x.opcode == LOP3 && x.encoding != 0x5be0000000000000 {
                return Err(x.error("only register LOP3 is lowered"));
            }
        }
        Ok(())
    }
}
