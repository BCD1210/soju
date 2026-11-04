//! Independent CPU execution oracle for the supported per-invocation IR contract.
//! It is not a GM20B hardware oracle. See SEMANTICS.md for floating-point limits.
use crate::decoder::{bits, Instruction, Opcode, Operand, Predicate, Space};
use crate::ir::Program;
use crate::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Machine {
    pub registers: [u32; 256],
    pub predicates: [bool; 8],
    pub local: Vec<u32>,
    pub attributes: BTreeMap<u16, u32>,
    pub local_id: [u32; 3],
    pub group_id: [u32; 3],
    pub killed: bool,
    pc: usize,
    tokens: Vec<usize>,
}
impl Default for Machine {
    fn default() -> Self {
        Self {
            registers: [0; 256],
            predicates: [false; 8],
            local: vec![],
            attributes: BTreeMap::new(),
            local_id: [0; 3],
            group_id: [0; 3],
            killed: false,
            pc: 0,
            tokens: vec![],
        }
    }
}
#[derive(Debug, Default)]
pub struct Memory {
    pub global: Vec<u32>,
    pub global_base: u64,
    pub shared: Vec<u32>,
    pub constants: BTreeMap<u8, Vec<u32>>,
    pub local_negative: u32,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stop {
    Exit,
    Barrier(usize),
}
impl Machine {
    pub fn predicate(&self, p: Predicate) -> bool {
        (p.index == 7 || self.predicates[usize::from(p.index)]) ^ p.inverted
    }
    fn register(&self, r: u8) -> u32 {
        if r == 255 {
            0
        } else {
            self.registers[usize::from(r)]
        }
    }
    fn put(&mut self, x: &Instruction, i: usize, value: u32) -> Result<()> {
        match x.operands.get(i) {
            Some(Operand::Register(r)) => {
                if *r != 255 {
                    self.registers[usize::from(*r)] = value;
                }
                Ok(())
            }
            _ => Err(x.error("expected destination register")),
        }
    }
    fn p(&self, x: &Instruction, i: usize) -> Result<bool> {
        match x.operands.get(i) {
            Some(Operand::Predicate(p)) => Ok(self.predicate(*p)),
            _ => Err(x.error("expected predicate")),
        }
    }
    fn target(program: &Program, x: &Instruction) -> Result<usize> {
        let a = x
            .target()
            .ok_or_else(|| x.error("missing control target"))?;
        let a = if a % 32 == 0 { a + 8 } else { a };
        program
            .instructions
            .iter()
            .position(|i| i.address == a)
            .ok_or_else(|| x.error("invalid control target"))
    }
    fn resume(
        &mut self,
        program: &Program,
        memory: &mut Memory,
        budget: &mut usize,
    ) -> Result<Stop> {
        while let Some(x) = program.instructions.get(self.pc) {
            if *budget == 0 {
                return Err(x.error("reference execution budget exhausted"));
            }
            *budget -= 1;
            self.pc += 1;
            if !self.predicate(x.predicate) {
                continue;
            }
            match x.opcode {
                Opcode::EXIT => return Ok(Stop::Exit),
                Opcode::KIL => {
                    self.killed = true;
                    return Ok(Stop::Exit);
                }
                Opcode::BRA => self.pc = Self::target(program, x)?,
                Opcode::SSY | Opcode::PBK => self.tokens.push(Self::target(program, x)?),
                Opcode::SYNC | Opcode::BRK => {
                    self.pc = self
                        .tokens
                        .pop()
                        .ok_or_else(|| x.error("reference stack underflow"))?
                }
                Opcode::BAR => return Ok(Stop::Barrier(self.pc - 1)),
                _ => self.instruction(x, memory)?,
            }
        }
        Err(Error("reference execution fell off code".into()))
    }
    pub fn run(&mut self, program: &Program, memory: &mut Memory) -> Result<()> {
        match self.resume(program, memory, &mut 1_000_000)? {
            Stop::Exit => Ok(()),
            Stop::Barrier(_) => Err(Error("use run_group for barriers".into())),
        }
    }
}
/// Cooperative CTA execution. Every lane must reach the same barrier or terminate.
/// Shared-memory races remain a program error; fixtures have disjoint writes between barriers.
pub fn run_group(program: &Program, lanes: &mut [Machine], memory: &mut Memory) -> Result<()> {
    let mut budget = 10_000_000;
    loop {
        let mut stop = None;
        for lane in lanes.iter_mut() {
            let next = lane.resume(program, memory, &mut budget)?;
            if stop.is_some_and(|s| s != next) {
                return Err(Error("divergent barrier or early lane termination".into()));
            }
            stop = Some(next);
        }
        if stop == Some(Stop::Exit) || stop.is_none() {
            return Ok(());
        }
    }
}
mod arithmetic;
mod instructions;
mod memory;
mod operands;
mod predicates;
