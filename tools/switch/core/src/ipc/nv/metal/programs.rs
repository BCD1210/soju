//! Bounded straight-line shader discovery inside the submitted GPU mapping.
//! Control-flow shaders require explicit extent discovery rather than scanning past EXIT.
use super::*;
use soju_switch_shader::{
    decoder::{Opcode, decode_instruction},
    msl::Options,
};

pub(super) fn register(
    registers: &soju_switch_gpu::engine::Registers,
    memory: &Memory,
    mappings: &[(u64, u64, Vec<u8>)],
    backend: &mut Backend,
) -> Result<(), Error> {
    for stage in [0x2044, 0x2144] {
        let address = registers
            .address(0x1608)
            .checked_add(u64::from(registers.get(stage)))
            .ok_or(Error::Malformed("shader address overflow"))?;
        let (base, _, allocation) = mappings
            .iter()
            .find(|(base, _, bytes)| address >= *base && address - *base < bytes.len() as u64)
            .ok_or(Error::Malformed("shader outside GPU mappings"))?;
        let available = (allocation.len() - (address - base) as usize).min(1024 * 1024);
        let bytes = memory.read(address, available).map_err(gpu_error)?;
        let size = straight_line_extent(bytes)?;
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
        backend
            .program(
                address,
                size,
                Options {
                    texture_count: 16,
                    ..Options::default()
                },
            )
            .map_err(gpu_error)?;
    }
    Ok(())
}

fn straight_line_extent(bytes: &[u8]) -> Result<usize, Error> {
    soju_switch_shader::header::Header::parse(bytes).map_err(|e| Error::Message(e.to_string()))?;
    for (slot, word) in bytes[80..].chunks_exact(8).enumerate() {
        if slot % 4 == 0 {
            continue;
        }
        let instruction = decode_instruction(
            (slot * 8) as u32,
            u64::from_le_bytes(word.try_into().unwrap()),
        )
        .map_err(|e| Error::Message(format!("shader extent: {e}")))?;
        if matches!(
            instruction.opcode,
            Opcode::BRA | Opcode::SSY | Opcode::SYNC | Opcode::PBK | Opcode::BRK
        ) {
            return Err(Error::Malformed(
                "control-flow shader extent discovery unsupported",
            ));
        }
        if instruction.opcode == Opcode::EXIT
            && instruction.predicate.is_always()
            && instruction.raw & 31 == 15
        {
            return Ok(80 + (slot + 1) * 8);
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
        assert_eq!(straight_line_extent(&bytes).unwrap(), 96);
        assert!(straight_line_extent(&bytes[..95]).is_err());
        assert!(straight_line_extent(&program(&[0, 0xe24000000007000f])).is_err());
        assert!(straight_line_extent(&program(&[0, 0xe30000000000000f])).is_err());
        assert!(straight_line_extent(&program(&[0, u64::MAX])).is_err());
    }
}
