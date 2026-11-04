use super::*;
use crate::compute::Launch;

pub fn generate_compute(
    launch: &Launch,
    program: &Program,
    options: &Options,
) -> Result<MetalShader> {
    launch.validate()?;
    let header = launch.header()?;
    let has_barrier = program.instructions.iter().any(|x| x.opcode == Opcode::BAR);
    if has_barrier {
        for node in &program.body {
            match node {
                Node::Instruction(i)
                    if program.instructions[*i].opcode != Opcode::EXIT
                        || program.instructions[*i].predicate.is_always() => {}
                _ => return Err(Error(
                    "barriers require straight-line control flow without conditional termination"
                        .into(),
                )),
            }
        }
    }
    let cb: BTreeSet<_> = program
        .instructions
        .iter()
        .flat_map(|x| &x.operands)
        .filter_map(|o| match o {
            Operand::Constant { slot, .. } => Some(*slot),
            _ => None,
        })
        .collect();
    if cb.iter().any(|s| *s > 30) {
        return Err(Error("cbuf slot exceeds Metal binding range".into()));
    }
    let mut s = String::from(
        "#include <metal_stdlib>\nusing namespace metal;\n#pragma clang fp contract(off)\n",
    );
    s.push_str(HELPERS);
    s.push_str("kernel void soju_compute(uint3 local_id [[thread_position_in_threadgroup]], uint3 group_id [[threadgroup_position_in_grid]]");
    for slot in &cb {
        write!(s, ", constant uint* cb{slot} [[buffer({slot})]]").unwrap();
    }
    memory::parameters(program, options, &cb, false, &mut s)?;
    s.push_str(") {\nuint r[255] = {};\nbool p[7] = {};\n");
    if launch.shared_bytes > 0 {
        writeln!(
            s,
            "threadgroup uint shared_mem[{}];",
            launch.shared_bytes / 4
        )
        .unwrap();
    }
    let local = launch.local_positive + launch.local_negative;
    if local > 0 {
        writeln!(s, "uint local_mem[{}] = {{}};", local.div_ceil(4)).unwrap();
    }
    let mut emitter = Emitter {
        header: &header,
        program,
        options,
        local_bytes: local,
        compute: Some(launch),
    };
    emitter.nodes(&program.body, &mut s)?;
    s.push_str("}\n");
    Ok(MetalShader {
        texture_bindings: Vec::new(),
        source: s,
        constant_buffers: cb.into_iter().collect(),
        uses_texture_table: false,
        entry_point: "soju_compute",
    })
}
