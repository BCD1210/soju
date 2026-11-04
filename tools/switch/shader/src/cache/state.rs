use super::*;
use crate::{
    msl::Options,
    texture::{Handle, TextureType},
};
impl Key {
    /// Includes every translator option, canonical resource mappings and caller render state.
    /// The caller still supplies target/compiler identities and any external pipeline layout
    /// in `render_state`; these cannot be inferred from shader words.
    pub fn for_shader(inputs: Inputs<'_>, options: &Options) -> Result<Self> {
        let mut state = b"Soju MSL ABI 2\0metal3.0\0-fno-fast-math\0-ffp-contract=off\0".to_vec();
        state.push(options.texture_buffer);
        state.extend(options.texture_count.to_le_bytes());
        match options.global_memory {
            Some(g) => {
                state.push(1);
                state.push(g.buffer);
                state.extend(g.base_address.to_le_bytes());
                state.extend(g.bytes.to_le_bytes());
            }
            None => state.push(0),
        }
        let bindings = crate::texture::normalize(&options.texture_bindings, options.texture_count)?;
        state.extend((bindings.len() as u64).to_le_bytes());
        for b in bindings {
            let (tag, raw) = match b.handle {
                Handle::Bound(v) => (0, v),
                Handle::Bindless(v) => (1, v),
            };
            state.push(tag);
            state.extend(raw.to_le_bytes());
            state.extend(b.texture_index.to_le_bytes());
            state.extend(b.sampler_index.to_le_bytes());
            state.push(match b.texture_type {
                TextureType::D1 => 1,
                TextureType::D2 => 2,
                TextureType::D3 => 3,
                TextureType::Cube => 4,
                TextureType::D1Array => 5,
                TextureType::D2Array => 6,
                TextureType::CubeArray => 7,
                TextureType::Shadow2D => 8,
                TextureType::Shadow2DArray => 9,
            });
        }
        state.extend((inputs.render_state.len() as u64).to_le_bytes());
        state.extend(inputs.render_state);
        Ok(Self::new(Inputs {
            render_state: &state,
            ..inputs
        }))
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{msl::GlobalMemory, texture::Binding};
    #[test]
    fn key_includes_options_resources_target_and_compiler_state() {
        let inputs = Inputs {
            program: b"words",
            header: b"SPH",
            render_state: b"gpu-family;compiler-build;vertex-layout",
        };
        let original = Options::default();
        let key = Key::for_shader(inputs, &original).unwrap();
        let mut changed = original.clone();
        changed.texture_buffer = 29;
        assert_ne!(key, Key::for_shader(inputs, &changed).unwrap());
        changed = original.clone();
        changed.texture_count = 64;
        assert_ne!(key, Key::for_shader(inputs, &changed).unwrap());
        changed = original.clone();
        changed.global_memory = Some(GlobalMemory {
            buffer: 0,
            base_address: 0x1000,
            bytes: 128,
        });
        assert_ne!(key, Key::for_shader(inputs, &changed).unwrap());
        changed = original.clone();
        changed.texture_bindings = vec![Binding {
            handle: Handle::Bound(7),
            texture_index: 1,
            sampler_index: 2,
            texture_type: TextureType::D2,
        }];
        let binding_key = Key::for_shader(inputs, &changed).unwrap();
        assert_ne!(key, binding_key);
        changed.texture_bindings[0].sampler_index = 3;
        assert_ne!(binding_key, Key::for_shader(inputs, &changed).unwrap());
        changed.texture_bindings[0].sampler_index = 2;
        changed.texture_bindings.push(changed.texture_bindings[0]);
        assert_eq!(binding_key, Key::for_shader(inputs, &changed).unwrap());
        assert_ne!(
            key,
            Key::for_shader(
                Inputs {
                    render_state: b"other target",
                    ..inputs
                },
                &original
            )
            .unwrap()
        );
    }
}
