use super::*;
impl Machine {
    pub(super) fn instruction(&mut self, x: &Instruction, m: &mut Memory) -> Result<()> {
        use Opcode::*;
        match x.opcode {
            NOP | DEPBAR => Ok(()),
            P2R | R2P => self.transfer(x, m),
            FSET | FSETP | ISET | ISETP | PSETP => self.compare(x, m),
            LD | ST | LDG | STG | LDS | STS | LDL | STL | ALD | AST | IPA => self.memory(x, m),
            MOV | LDC | I2I | IADD | IADD3 | ISCADD | IMAD | XMAD | IMNMX | SEL | LOP | LOP3
            | SHL | SHR | POPC | FLO | BFE | BFI | F2I | FADD | FMUL | FFMA | FMNMX | MUFU
            | F2F | I2F => self.arithmetic(x, m),
            _ => Err(x.error("reference opcode not implemented")),
        }
    }
}
