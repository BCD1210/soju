//! Bounded shader discovery inside the submitted GPU mapping.
use super::*;
use soju_switch_shader::{
    decoder::{Opcode, decode_instruction},
    msl::Options,
};

#[derive(Default)]
pub(super) struct Cache(
    BTreeMap<
        u64,
        (
            u64,
            usize,
            Vec<soju_switch_shader::decoder::ScheduledInstruction>,
        ),
    >,
);

pub(super) fn register(
    snapshot: &soju_switch_gpu::engine::Snapshot,
    memory: &Memory,
    mappings: &[(u64, u64, usize)],
    backend: &mut Backend,
    cache: &mut Cache,
) -> Result<(), Error> {
    let _span = soju_switch_gpu::profile::span(soju_switch_gpu::profile::Stage::ShaderDiscovery);
    let registers = &snapshot.registers;
    for (stage, group_register) in [(0x2044, 0x2050), (0x2144, 0x2150)] {
        let address = registers
            .address(0x1608)
            .checked_add(u64::from(registers.get(stage)))
            .ok_or(Error::Malformed("shader address overflow"))?;
        let (base, _, allocation) = mappings
            .iter()
            .find(|(base, _, bytes)| address >= *base && address - *base < *bytes as u64)
            .ok_or(Error::Malformed("shader outside GPU mappings"))?;
        let available = (*allocation - (address - base) as usize).min(1024 * 1024);
        let bytes = memory.read(address, available).map_err(gpu_error)?;
        let valid = cache.0.get(&address).is_some_and(|(generation, size, _)| {
            *size <= available && memory.generation(address, *size).ok() == Some(*generation)
        });
        if !valid {
            let size = shader_extent(bytes)?;
            let decoded = soju_switch_shader::decoder::decode(&bytes[80..size])
                .map_err(|e| Error::Message(e.to_string()))?;
            if cache.0.len() >= 256 {
                cache.0.clear();
            }
            cache.0.insert(
                address,
                (
                    memory.generation(address, size).map_err(gpu_error)?,
                    size,
                    decoded,
                ),
            );
        }
        let (_, size, decoded) = &cache.0[&address];
        let size = *size;
        if std::env::var_os("SOJU_SWITCH_TRACE_GPU").is_some() {
            eprintln!(
                "gpu-shader|{address:#x}|{size}|{:?}",
                soju_switch_shader::header::Header::parse(bytes)
            );
            for item in soju_switch_shader::decoder::decode(&bytes[80..size])
                .map_err(|e| Error::Message(e.to_string()))?
            {
                eprintln!("gpu-instruction|{:?}", item.instruction);
            }
        }
        let mut texture_bindings = Vec::new();
        for decoded in decoded {
            let instruction = &decoded.instruction;
            if !matches!(instruction.opcode, Opcode::TEX | Opcode::TEXS | Opcode::TLD | Opcode::TLDS | Opcode::TLD4 | Opcode::TXQ) {
                continue;
            }
            let (handle, texture_type) = soju_switch_shader::msl::texture_descriptor(instruction)
                .map_err(|e| Error::Message(e.to_string()))?
                .ok_or(Error::Malformed("dynamic texture handle discovery unsupported"))?;
            let group = registers.get(group_register) as usize;
            let slot = registers.get(0x2608) as usize;
            let binding = snapshot
                .constants
                .get(group)
                .and_then(|g| g.get(slot))
                .copied()
                .flatten()
                .ok_or(Error::Malformed("unbound texture handle cbuf"))?;
            let offset = handle
                .checked_mul(4)
                .ok_or(Error::Malformed("texture handle offset"))?;
            if offset.checked_add(4).is_none_or(|end| end > binding.size) {
                return Err(Error::Malformed("texture handle exceeds cbuf"));
            }
            let packed = u32::from_le_bytes(
                memory
                    .read(binding.address + u64::from(offset), 4)
                    .map_err(gpu_error)?
                    .try_into()
                    .unwrap(),
            );
            texture_bindings.push(soju_switch_shader::texture::Binding {
                handle: soju_switch_shader::texture::Handle::Bound(handle),
                texture_index: packed & 0xfffff,
                sampler_index: packed >> 20,
                texture_type,
            });
        }
        let options = Options {
            constant_buffer_bytes: snapshot
                .constants
                .get(registers.get(group_register) as usize)
                .map(|group| group.map(|binding| binding.map_or(0, |b| b.size)))
                .ok_or(Error::Malformed("constant group"))?,
            texture_count: 16,
            texture_bindings,
            ..Options::default()
        };
        if std::env::var_os("SOJU_SWITCH_AUDIT_SHADERS").is_some() {
            use sha2::{Digest, Sha256};
            let hash = Sha256::digest(&bytes[..size]);
            let decoded = soju_switch_shader::decoder::decode(&bytes[80..size])
                .map_err(|e| Error::Message(e.to_string()))?;
            let outcome = match soju_switch_shader::translate(&bytes[..size], &options) {
                Ok(_) => "translated".to_owned(),
                Err(error) => error.to_string(),
            };
            eprintln!(
                "shader-audit|{hash:x}|stage={stage:#x}|instructions={}|{outcome}",
                decoded.len()
            );
        }
        backend.program(address, size, options).map_err(gpu_error)?;
    }
    Ok(())
}

fn shader_extent(bytes: &[u8]) -> Result<usize, Error> {
    soju_switch_shader::header::Header::parse(bytes).map_err(|e| Error::Message(e.to_string()))?;
    Ok(80 + code_extent(&bytes[80..])?)
}

pub(super) fn code_extent(bytes: &[u8]) -> Result<usize, Error> {
    let mut furthest_target = 0;
    for (slot, word) in bytes.chunks_exact(8).enumerate() {
        if slot % 4 == 0 {
            continue;
        }
        let instruction = decode_instruction(
            (slot * 8) as u32,
            u64::from_le_bytes(word.try_into().unwrap()),
        )
        .map_err(|e| Error::Message(format!("shader extent: {e}")))?;
        if matches!(instruction.opcode, Opcode::BRA | Opcode::SSY | Opcode::PBK) {
            let target = instruction
                .target()
                .ok_or(Error::Malformed("indirect shader target"))?;
            let target = if target.is_multiple_of(32) {
                target
                    .checked_add(8)
                    .ok_or(Error::Malformed("shader target overflow"))?
            } else {
                target
            };
            if !target.is_multiple_of(8) || u64::from(target) + 8 > bytes.len() as u64 {
                return Err(Error::Malformed("shader target outside mapped bound"));
            }
            furthest_target = furthest_target.max(target);
        }
        if instruction.opcode == Opcode::EXIT
            && instruction.predicate.is_always()
            && instruction.raw & 31 == 15
            && instruction.address >= furthest_target
        {
            let size = (slot + 1) * 8;
            let decoded = soju_switch_shader::decoder::decode(&bytes[..size])
                .map_err(|e| Error::Message(e.to_string()))?;
            soju_switch_shader::ir::Program::build(&decoded)
                .map_err(|e| Error::Message(format!("shader extent: {e}")))?;
            return Ok(size);
        }
    }
    Err(Error::Malformed(
        "shader has no unconditional EXIT within mapped bound",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn program(words: &[u64]) -> Vec<u8> {
        let mut bytes = vec![0; 80];
        bytes[..4].copy_from_slice(&0x401u32.to_le_bytes());
        for word in words {
            bytes.extend(word.to_le_bytes());
        }
        bytes
    }
    #[test]
    fn extent_stops_at_exit_and_never_scans_padding() {
        let bytes = program(&[0, 0xe30000000007000f, u64::MAX]);
        assert_eq!(shader_extent(&bytes).unwrap(), 96);
        assert!(shader_extent(&bytes[..95]).is_err());
        assert!(shader_extent(&program(&[0, 0xe24000000007000f])).is_err());
        assert!(shader_extent(&program(&[0, 0xe30000000000000f])).is_err());
        assert!(shader_extent(&program(&[0, u64::MAX])).is_err());
    }

    #[test]
    fn extent_includes_forward_target_beyond_early_exit() {
        // @P0 BRA from 8 to 24; the EXIT at 16 is only the other path.
        let bytes = program(&[
            0,
            0xe24000000080000f,
            0xe30000000007000f,
            0xe30000000007000f,
            u64::MAX,
        ]);
        assert_eq!(shader_extent(&bytes).unwrap(), 112);
        assert!(shader_extent(&bytes[..104]).is_err());
    }

    #[test]
    fn extent_validates_reconvergent_loop_before_padding() {
        let bytes = program(&[
            0,
            0xe290000002800000,
            0x50b0000000070f00,
            0xe2400fffff00000f,
            0,
            0xf0f800000007000f,
            0x50b0000000070f00,
            0xe30000000007000f,
            u64::MAX,
        ]);
        assert_eq!(shader_extent(&bytes).unwrap(), 144);
        assert!(shader_extent(&program(&[0, 0xf0f800000007000f, 0xe30000000007000f])).is_err());
    }
}
