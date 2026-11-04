//! Metal source backend. A decoded opcode is not a promise of semantic support.
use crate::decoder::{bits, Instruction, Opcode, Operand, Predicate, Space};
use crate::header::{Header, Interpolation, Stage};
use crate::ir::{Node, Program};
use crate::{Error, Result};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobalMemory {
    pub buffer: u8,
    pub base_address: u64,
    pub bytes: u32,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// Explicit host-resolved handles; empty retains the legacy paired-index ABI.
    pub texture_bindings: Vec<crate::texture::Binding>,
    pub global_memory: Option<GlobalMemory>,
    /// Metal argument buffer binding. Must not collide with a used cbuf slot.
    pub texture_buffer: u8,
    /// Host provides a normalized descriptor table; see README for the ABI.
    pub texture_count: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            texture_bindings: Vec::new(),
            global_memory: None,
            texture_buffer: 30,
            texture_count: 128,
        }
    }
}
#[derive(Debug, Clone)]
pub struct MetalShader {
    pub texture_bindings: Vec<crate::texture::Binding>,
    pub source: String,
    pub constant_buffers: Vec<u8>,
    pub uses_texture_table: bool,
    pub entry_point: &'static str,
}
pub fn generate(header: &Header, program: &Program, options: &Options) -> Result<MetalShader> {
    if !matches!(header.stage, Stage::Vertex | Stage::Fragment) {
        return Err(Error(
            "only standalone Vertex B and Fragment stages have a Metal backend".into(),
        ));
    }
    if header.depth_output {
        return Err(Error(
            "fragment depth register ABI is not implemented".into(),
        ));
    }
    if header.stage == Stage::Fragment && (header.raw[14] != 0 || header.raw[17] != 0) {
        return Err(Error("special fragment inputs are not implemented".into()));
    }
    if options.texture_count == 0 || options.texture_count > 4096 || options.texture_buffer > 30 {
        return Err(Error("invalid Metal argument table configuration".into()));
    }
    let mut cb = BTreeSet::new();
    let mut textures = false;
    for x in reachability::instructions(program) {
        for o in &x.operands {
            if let Operand::Constant { slot, .. } = o {
                cb.insert(*slot);
            }
        }
        textures |= matches!(
            x.opcode,
            Opcode::TEX | Opcode::TEXS | Opcode::TLD | Opcode::TLDS | Opcode::TLD4 | Opcode::TXQ
        );
    }
    if cb
        .iter()
        .any(|s| *s > 30 || (textures && *s == options.texture_buffer))
    {
        return Err(Error(
            "cbuf slot exceeds Metal binding range or collides with texture table".into(),
        ));
    }
    let vertex = header.stage == Stage::Vertex;
    let mut s = String::from(
        "#include <metal_stdlib>\nusing namespace metal;\n#pragma clang fp contract(off)\n",
    );
    s.push_str(HELPERS);
    s.push_str("struct StageIn {\n");
    if vertex {
        let slots: BTreeSet<_> = header.inputs.iter().map(|a| a.address / 16).collect();
        for slot in slots {
            if !(8..39).contains(&slot) {
                return Err(Error(format!(
                    "unsupported vertex input address 0x{:x}",
                    slot * 16
                )));
            }
            writeln!(s, "float4 a{} [[attribute({})]];", slot - 8, slot - 8).unwrap();
        }
    } else {
        if header
            .inputs
            .iter()
            .any(|a| a.address >= 0x70 && a.address < 0x80)
            || reachability::instructions(program)
                .into_iter()
                .any(|x| x.opcode == Opcode::IPA && bits(x.raw, 54, 2) == 1)
        {
            s.push_str("float4 position [[position]];\n");
        }
        for a in &header.inputs {
            if (0x70..0x80).contains(&a.address) {
                continue;
            }
            if !(0x80..0x280).contains(&a.address) {
                return Err(Error(format!(
                    "unsupported fragment input 0x{:x}",
                    a.address
                )));
            }
            let mode = match a.interpolation {
                Some(Interpolation::Flat) => "flat",
                Some(Interpolation::Linear) => "center_no_perspective",
                _ => "center_perspective",
            };
            writeln!(
                s,
                "float v{} [[user(locn{}), {mode}]];",
                a.address / 4,
                a.address / 4
            )
            .unwrap();
        }
    }
    s.push_str("};\nstruct StageOut {\n");
    if vertex {
        s.push_str("float4 position [[position]];\n");
        if header.outputs.iter().any(|a| a.address == 0x6c) {
            s.push_str("float point_size [[point_size]];\n");
        }
        if header
            .outputs
            .iter()
            .any(|a| (0x2c0..0x2e0).contains(&a.address))
        {
            s.push_str("float clip_distance [[clip_distance]] [8];\n");
        }
        for a in &header.outputs {
            if (0x6c..0x80).contains(&a.address) || (0x2c0..0x2e0).contains(&a.address) {
                continue;
            }
            if !(0x80..0x280).contains(&a.address) {
                return Err(Error(format!(
                    "unsupported vertex output 0x{:x}",
                    a.address
                )));
            }
            writeln!(
                s,
                "float v{} [[user(locn{})]];",
                a.address / 4,
                a.address / 4
            )
            .unwrap();
        }
    } else {
        for (i, m) in header.color_masks.iter().enumerate() {
            if *m != 0 {
                writeln!(s, "float4 color{i} [[color({i})]];").unwrap();
            }
        }
    }
    s.push_str("};\n");
    if textures {
        s.push_str(&textures::declarations(program, options)?);
    }
    let entry = if vertex {
        "soju_vertex"
    } else {
        "soju_fragment"
    };
    write!(
        s,
        "{} StageOut {entry}(StageIn in [[stage_in]]",
        if vertex { "vertex" } else { "fragment" }
    )
    .unwrap();
    for slot in &cb {
        write!(s, ", constant uint* cb{slot} [[buffer({slot})]]").unwrap();
    }
    if textures {
        write!(
            s,
            ", constant TextureTable& resources [[buffer({})]]",
            options.texture_buffer
        )
        .unwrap();
    }
    memory::parameters(program, options, &cb, textures, &mut s)?;
    s.push_str(") {\nuint r[255] = {};\nbool p[7] = {};\nStageOut out = {};\n");
    let local = header
        .local_positive
        .checked_add(header.local_negative)
        .ok_or_else(|| Error("local allocation overflow".into()))?;
    if local > 64 * 1024 {
        return Err(Error(
            "local memory allocation exceeds backend limit 64 KiB".into(),
        ));
    }
    if local > 0 {
        writeln!(s, "uint local_mem[{}] = {{}};", local.div_ceil(4)).unwrap();
    }
    let mut emitter = Emitter {
        header,
        program,
        options,
        local_bytes: local,
        compute: None,
    };
    emitter.nodes(&program.body, &mut s)?;
    s.push_str(&emitter.finish());
    s.push_str("}\n");
    Ok(MetalShader {
        texture_bindings: textures::bindings(program, options)?,
        source: s,
        constant_buffers: cb.into_iter().collect(),
        uses_texture_table: textures,
        entry_point: entry,
    })
}
struct Emitter<'a> {
    header: &'a Header,
    program: &'a Program,
    options: &'a Options,
    local_bytes: u32,
    compute: Option<&'a crate::compute::Launch>,
}

mod flow;
mod helpers;
mod instructions;
mod operands;
mod predicates;
mod resources;
mod validation;
use helpers::HELPERS;
use operands::*;
#[cfg(test)]
mod tests;

mod compute;
mod memory;
pub use compute::generate_compute;

mod textures;

mod reachability;
