//! Validated per-invocation control flow with a deliberately conservative structurizer.
use crate::decoder::{Instruction, Opcode, Predicate, ScheduledInstruction};
use crate::{Error, Result};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BasicBlock {
    pub start: u32,
    pub instructions: std::ops::Range<usize>,
    /// Block indices, not byte addresses. EXIT and KIL have no taken successor.
    pub successors: Vec<usize>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Instruction(usize),
    If {
        condition: Predicate,
        body: Vec<Node>,
    },
    Loop {
        body: Vec<Node>,
    },
    Break(Predicate),
    Continue(Predicate),
}
#[derive(Debug, Clone)]
pub struct Program {
    pub instructions: Vec<Instruction>,
    pub blocks: Vec<BasicBlock>,
    pub body: Vec<Node>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Token {
    kind: Opcode,
    target: usize,
}

impl Program {
    pub fn build(decoded: &[ScheduledInstruction]) -> Result<Self> {
        if decoded.is_empty() {
            return Err(Error("empty shader".into()));
        }
        let ins: Vec<_> = decoded.iter().map(|s| s.instruction.clone()).collect();
        let n = ins.len();
        let addresses: BTreeMap<_, _> = ins
            .iter()
            .enumerate()
            .map(|(i, x)| (x.address, i))
            .collect();
        if addresses.len() != n || ins.windows(2).any(|w| w[0].address >= w[1].address) {
            return Err(Error(
                "instruction addresses must be unique and increasing".into(),
            ));
        }
        let target = |x: &Instruction| -> Result<usize> {
            let a = x
                .target()
                .ok_or_else(|| x.error("indirect control flow is unsupported"))?;
            // A branch to the next bundle's scheduling word executes its first instruction.
            let a = if a % 32 == 0 {
                a.checked_add(8).ok_or_else(|| x.error("target overflow"))?
            } else {
                a
            };
            addresses
                .get(&a)
                .copied()
                .ok_or_else(|| x.error(format!("target 0x{a:x} is outside code or unaligned")))
        };
        let mut states: Vec<Option<Vec<Token>>> = vec![None; n];
        states[0] = Some(Vec::new());
        let mut queue = VecDeque::from([0]);
        let mut edges = vec![Vec::new(); n];
        let mut jumps = vec![None; n];
        while let Some(i) = queue.pop_front() {
            let x = &ins[i];
            let mut stack = states[i].clone().unwrap();
            let mut next: Vec<(usize, Vec<Token>)> = Vec::new();
            let never = x.predicate.index == 7 && x.predicate.inverted;
            let conditional = !x.predicate.is_always();
            let fall = |v: &mut Vec<(usize, Vec<Token>)>, s: Vec<Token>| -> Result<()> {
                if i + 1 == n {
                    return Err(x.error("reachable fallthrough past shader end"));
                }
                v.push((i + 1, s));
                Ok(())
            };
            if matches!(
                x.opcode,
                Opcode::BRA | Opcode::SYNC | Opcode::BRK | Opcode::EXIT | Opcode::KIL
            ) && x.raw & 31 != 15
            {
                return Err(
                    x.error("condition-code control flow is unsupported (requires condition 0xf)")
                );
            }
            if never {
                fall(&mut next, stack)?;
            } else {
                match x.opcode {
                    Opcode::SSY | Opcode::PBK => {
                        let t = target(x)?;
                        if t <= i {
                            return Err(x.error("stack setup must target a forward region"));
                        }
                        if stack.len() >= 16 {
                            return Err(x.error("control stack exceeds supported depth 16"));
                        }
                        stack.push(Token {
                            kind: x.opcode,
                            target: t,
                        });
                        fall(&mut next, stack)?;
                    }
                    Opcode::BRA => {
                        let t = target(x)?;
                        jumps[i] = Some(t);
                        next.push((t, stack.clone()));
                        if conditional {
                            fall(&mut next, stack)?;
                        }
                    }
                    Opcode::SYNC | Opcode::BRK => {
                        let expected = if x.opcode == Opcode::SYNC {
                            Opcode::SSY
                        } else {
                            Opcode::PBK
                        };
                        let token = stack
                            .pop()
                            .ok_or_else(|| x.error("control stack underflow"))?;
                        if token.kind != expected {
                            return Err(x.error("crossed reconvergence/break stack"));
                        }
                        jumps[i] = Some(token.target);
                        next.push((token.target, stack));
                        if conditional {
                            fall(&mut next, states[i].clone().unwrap())?;
                        }
                    }
                    Opcode::EXIT | Opcode::KIL => {
                        if conditional {
                            fall(&mut next, stack)?;
                        }
                    }
                    _ => fall(&mut next, stack)?,
                }
            }
            for (to, s) in next {
                edges[i].push(to);
                match &states[to] {
                    Some(old) if *old != s => {
                        return Err(ins[to].error("inconsistent control stack at merge or loop"))
                    }
                    Some(_) => (),
                    None => {
                        states[to] = Some(s);
                        queue.push_back(to);
                    }
                }
            }
        }
        // A supported loop has one lexical header, one backward BRA, and a single
        // lexical exit. Forward entries into its interior are irreducible here.
        let mut loops = BTreeMap::new();
        for (i, j) in jumps.iter().enumerate() {
            if let Some(t) = *j {
                if t <= i && (ins[i].opcode != Opcode::BRA || loops.insert(t, i).is_some()) {
                    return Err(ins[i].error("multiple latches or non-BRA back edge"));
                }
            }
        }
        for (&start, &latch) in &loops {
            for (from, successors) in edges.iter().enumerate() {
                for &to in successors {
                    let inside = (start..=latch).contains(&from);
                    if !inside && to > start && to <= latch {
                        return Err(ins[from].error("branch enters loop interior"));
                    }
                    if inside && !(start..=latch).contains(&to) && to != latch + 1 {
                        return Err(ins[from].error("loop has a nonlexical exit"));
                    }
                }
            }
            for (&other, &end) in &loops {
                if start < other && other <= latch && end > latch {
                    return Err(ins[other].error("crossing loops"));
                }
            }
        }
        let reachable: Vec<_> = states.iter().map(Option::is_some).collect();
        let builder = Builder {
            ins: &ins,
            jumps: &jumps,
            loops: &loops,
            reachable: &reachable,
        };
        let body = builder.range(0, n, None)?;
        let mut leaders = BTreeSet::from([0]);
        for (i, successors) in edges.iter().enumerate() {
            if jumps[i].is_some() || matches!(ins[i].opcode, Opcode::EXIT | Opcode::KIL) {
                leaders.extend(successors);
                if i + 1 < n {
                    leaders.insert(i + 1);
                }
            }
        }
        let starts: Vec<_> = leaders.into_iter().collect();
        let block_of = |i| starts.partition_point(|&s| s <= i) - 1;
        let blocks = starts
            .iter()
            .enumerate()
            .map(|(b, &s)| {
                let end = starts.get(b + 1).copied().unwrap_or(n);
                let mut successors: Vec<_> = edges[end - 1].iter().map(|&i| block_of(i)).collect();
                successors.sort_unstable();
                successors.dedup();
                BasicBlock {
                    start: ins[s].address,
                    instructions: s..end,
                    successors,
                }
            })
            .collect();
        Ok(Self {
            instructions: ins,
            blocks,
            body,
        })
    }
}
struct Builder<'a> {
    ins: &'a [Instruction],
    jumps: &'a [Option<usize>],
    loops: &'a BTreeMap<usize, usize>,
    reachable: &'a [bool],
}
impl Builder<'_> {
    fn range(&self, mut i: usize, end: usize, active: Option<(usize, usize)>) -> Result<Vec<Node>> {
        let mut result = Vec::new();
        while i < end {
            let x = &self.ins[i];
            if !self.reachable[i] {
                i += 1;
                continue;
            }
            if let Some(&latch) = self.loops.get(&i) {
                if active.map(|a| a.0) != Some(i) {
                    if latch >= end {
                        return Err(x.error("loop crosses conditional region"));
                    }
                    let mut body = self.range(i, latch, Some((i, latch + 1)))?;
                    body.push(Node::Continue(self.ins[latch].predicate));
                    body.push(Node::Break(Predicate::ALWAYS));
                    result.push(Node::Loop { body });
                    i = latch + 1;
                    continue;
                }
            }
            if matches!(x.opcode, Opcode::SSY | Opcode::PBK) {
                i += 1;
                continue;
            }
            if let Some(t) = self.jumps[i] {
                if let Some((start, exit)) = active {
                    if t == start {
                        result.push(Node::Continue(x.predicate));
                        i += 1;
                        continue;
                    }
                    if t == exit {
                        result.push(Node::Break(x.predicate));
                        i += 1;
                        continue;
                    }
                }
                if t <= i || t > end {
                    return Err(x.error("branch crosses structured region"));
                }
                let p = Predicate {
                    inverted: !x.predicate.inverted,
                    ..x.predicate
                };
                result.push(Node::If {
                    condition: p,
                    body: self.range(i + 1, t, active)?,
                });
                i = t;
            } else {
                result.push(Node::Instruction(i));
                i += 1;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decoder::decode;
    fn program(words: &[u64]) -> Result<Program> {
        let bytes: Vec<_> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        Program::build(&decode(&bytes)?)
    }
    #[test]
    fn structured_counted_loop() {
        // SSY -> 0x38, NOP, @P0 BRA -> 0x10, SYNC, EXIT.
        let p = program(&[
            0,
            0xe290000002800000,
            0x50b0000000070f00,
            0xe2400fffff00000f,
            0,
            0xf0f800000007000f,
            0x50b0000000070f00,
            0xe30000000007000f,
        ])
        .unwrap();
        assert!(p.body.iter().any(|n| matches!(n, Node::Loop { .. })));
        assert!(p.blocks.len() >= 3);
    }
    #[test]
    fn rejects_bad_flow() {
        assert!(program(&[0, 0xf0f800000007000f])
            .unwrap_err()
            .0
            .contains("underflow"));
        assert!(program(&[0, 0xe24000000017000f]).is_err());
        assert!(program(&[0, 0x50b0000000070f00])
            .unwrap_err()
            .0
            .contains("fallthrough"));
        assert!(program(&[0, 0xe300000000070000])
            .unwrap_err()
            .0
            .contains("condition-code"));
    }
}
