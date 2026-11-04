use super::*;
impl Emitter<'_> {
    pub(super) fn finish(&self) -> String {
        let mut s = String::new();
        if self.header.stage == Stage::Fragment {
            let mut r = 0;
            for (i, mask) in self.header.color_masks.iter().enumerate() {
                for c in 0..4 {
                    if mask & (1 << c) != 0 {
                        writeln!(s, "out.color{i}[{c}] = as_type<float>(r[{r}]);").unwrap();
                        r += 1;
                    }
                }
            }
        }
        s.push_str("return out;\n");
        s
    }
    pub(super) fn nodes(&mut self, nodes: &[Node], s: &mut String) -> Result<()> {
        for n in nodes {
            match n {
                Node::If { condition, body } => {
                    writeln!(s, "if ({}) {{", pred(*condition)).unwrap();
                    self.nodes(body, s)?;
                    s.push_str("}\n");
                }
                Node::Loop { body } => {
                    s.push_str("while (true) {\n");
                    self.nodes(body, s)?;
                    s.push_str("}\n");
                }
                Node::Break(p) => writeln!(s, "if ({}) {{ break; }}", pred(*p)).unwrap(),
                Node::Continue(p) => writeln!(s, "if ({}) {{ continue; }}", pred(*p)).unwrap(),
                Node::Instruction(i) => {
                    let x = &self.program.instructions[*i];
                    if x.predicate.index == 7 && x.predicate.inverted {
                        continue;
                    }
                    let line = self.instruction(x)?;
                    writeln!(
                        s,
                        "// 0x{:x} {:?}\nif ({}) {{\n{line}}}",
                        x.address,
                        x.opcode,
                        pred(x.predicate)
                    )
                    .unwrap();
                }
            }
        }
        Ok(())
    }
}
