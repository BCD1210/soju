use super::*;
/// Resource declarations follow the structured reachable body, including constant predicates.
pub(super) fn instructions(program: &Program) -> Vec<&Instruction> {
    fn visit<'a>(program: &'a Program, nodes: &[Node], out: &mut Vec<&'a Instruction>) {
        for node in nodes {
            match node {
                Node::Instruction(i) => {
                    let x = &program.instructions[*i];
                    if x.predicate.index != 7 || !x.predicate.inverted {
                        out.push(x);
                    }
                }
                Node::If { condition, body } => {
                    if condition.index != 7 || !condition.inverted {
                        visit(program, body, out);
                    }
                }
                Node::Loop { body } => visit(program, body, out),
                Node::Break(_) | Node::Continue(_) => (),
            }
        }
    }
    let mut out = Vec::new();
    visit(program, &program.body, &mut out);
    out
}
