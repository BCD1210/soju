use super::*;

pub(super) struct ComputeTarget {
    pub address: u64,
    pub size: usize,
    pub generation: u64,
    pub identity: u64,
}
impl Backend {
    /// Register scheduling bundles without SPH and a bounded global-memory window.
    pub fn compute_program(&mut self, address: u64, size: usize, options: Options) -> Result<()> {
        require(
            (16..=1024 * 1024).contains(&size) && size.is_multiple_of(8),
            "invalid compute allocation length",
        )?;
        let global = options
            .global_memory
            .ok_or_else(|| Error("compute requires a global-memory window".into()))?;
        require(
            global
                .base_address
                .checked_add(u64::from(global.bytes))
                .is_some()
                && global.buffer < 31
                && global.bytes > 0
                && global.bytes <= 64 * 1024 * 1024
                && global.bytes.is_multiple_of(4)
                && global.base_address & 3 == 0,
            "invalid compute global-memory window",
        )?;
        require(
            options.texture_bindings.is_empty(),
            "compute texture tables unsupported",
        )?;
        self.compute_programs.insert(address, (size, options));
        Ok(())
    }
    /// Capture code/constants now. Global storage stays GPU-resident until readback.
    pub fn prepare_compute(
        &mut self,
        dispatch: &crate::compute::Dispatch,
        memory: &Memory,
    ) -> Result<()> {
        require(
            self.targets.is_empty() && self.depth_targets.is_empty() && self.queries.is_empty(),
            "readback graphics before compute",
        )?;
        let (size, options) = self
            .compute_programs
            .get(&dispatch.program)
            .ok_or_else(|| Error("compute shader extent not registered".into()))?;
        let global = options.global_memory.unwrap();
        let generation = memory.generation(global.base_address, global.bytes as usize)?;
        let key = format!(
            "compute-{}-{:x}-{}",
            memory.identity, global.base_address, global.bytes
        );
        for (other_key, other) in &self.compute_targets {
            require(
                other_key == &key
                    || global.base_address >= other.address + other.size as u64
                    || other.address >= global.base_address + u64::from(global.bytes),
                "overlapping compute windows require readback",
            )?;
            if other_key == &key {
                require(
                    other.generation == generation,
                    "CPU write overlaps pending compute; readback first",
                )?;
            }
            let end = other.address + other.size as u64;
            require(
                dispatch.qmd_address >= end || other.address >= dispatch.qmd_address + 256,
                "compute-written QMD requires readback",
            )?;
            require(
                dispatch.program >= end || other.address >= dispatch.program + *size as u64,
                "compute-written code requires readback",
            )?;
        }
        let shader = soju_switch_shader::compute::translate(
            memory.read(dispatch.program, *size)?,
            &dispatch.launch,
            options,
        )
        .map_err(io)?;
        require(
            !shader.uses_texture_table,
            "compute texture tables unsupported",
        )?;
        let mut constants = Vec::new();
        for &slot in &shader.constant_buffers {
            let cb = dispatch
                .constant(slot as usize)
                .ok_or_else(|| Error(format!("unbound compute cbuf {slot}")))?;
            // Global writes may alias constants, including the current dispatch's
            // writable window. Require an explicit visibility boundary instead.
            for (address, size) in self
                .compute_targets
                .values()
                .map(|t| (t.address, t.size))
                .chain([(global.base_address, global.bytes as usize)])
            {
                require(
                    cb.address >= address + size as u64
                        || address >= cb.address + u64::from(cb.size),
                    "compute global/cbuf alias requires readback and separate storage",
                )?;
            }
            let offset = uniform_offset(
                &mut self.constant_upload,
                &mut self.uniform_offsets,
                ConstantBuffer {
                    address: cb.address,
                    size: cb.size,
                },
                memory,
            )?;
            constants.push(ConstantCommand {
                stage: 0,
                slot: slot as _,
                offset: offset as _,
            });
        }
        require(
            dispatch.program >= global.base_address + u64::from(global.bytes)
                || global.base_address >= dispatch.program + *size as u64,
            "compute code/global alias",
        )?;
        require(
            dispatch.qmd_address >= global.base_address + u64::from(global.bytes)
                || global.base_address >= dispatch.qmd_address + 256,
            "compute QMD/global alias",
        )?;
        let source = self.file(shader.source.into_bytes());
        let file = self.file(
            memory
                .read(global.base_address, global.bytes as usize)?
                .to_vec(),
        );
        let local = dispatch.launch.local_size;
        let groups = dispatch.groups;
        self.prepared = None;
        self.commands.push(Command {
            kind: 4,
            source: source.into(),
            key: key.clone().into(),
            generation: generation as _,
            file: file.into(),
            slot: global.buffer as _,
            local0: local[0] as _,
            local1: local[1] as _,
            local2: local[2] as _,
            groups0: groups[0] as _,
            groups1: groups[1] as _,
            groups2: groups[2] as _,
            constants: constants.into(),
            ..Command::default()
        });
        self.compute_targets.insert(
            key,
            ComputeTarget {
                address: global.base_address,
                size: global.bytes as usize,
                generation,
                identity: memory.identity,
            },
        );
        Ok(())
    }
}
