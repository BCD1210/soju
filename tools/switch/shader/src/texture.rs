//! Host-resolved texture handles. Native descriptor memory is owned by the caller.
use crate::{Error, Result};
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TextureType {
    D2,
    D1,
    D3,
    Cube,
    D1Array,
    D2Array,
    CubeArray,
    Shadow2D,
    Shadow2DArray,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Handle {
    Bound(u32),
    Bindless(u32),
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Binding {
    pub handle: Handle,
    pub texture_index: u32,
    pub sampler_index: u32,
    pub texture_type: TextureType,
}
/// Deduplicate identical mappings; a handle may never silently change type or resource.
pub fn normalize(bindings: &[Binding], count: u32) -> Result<Vec<Binding>> {
    let mut table = std::collections::BTreeMap::new();
    for &binding in bindings {
        if binding.texture_index >= count || binding.sampler_index >= count {
            return Err(Error(
                "texture or sampler index exceeds resource count".into(),
            ));
        }
        if table
            .insert(binding.handle, binding)
            .is_some_and(|old| old != binding)
        {
            return Err(Error("conflicting texture handle mapping".into()));
        }
    }
    Ok(table.into_values().collect())
}
impl TextureType {
    pub(crate) fn metal(self) -> (&'static str, &'static str, u32) {
        match self {
            Self::D2 => ("texture2d<float>", "images", 0),
            Self::D1 => ("texture1d<float>", "images1d", 1),
            Self::D3 => ("texture3d<float>", "images3d", 2),
            Self::Cube => ("texturecube<float>", "images_cube", 3),
            Self::D1Array => ("texture1d_array<float>", "images1da", 4),
            Self::D2Array => ("texture2d_array<float>", "images2da", 5),
            Self::CubeArray => ("texturecube_array<float>", "images_cubea", 6),
            Self::Shadow2D => ("depth2d<float>", "images_depth", 7),
            Self::Shadow2DArray => ("depth2d_array<float>", "images_deptha", 8),
        }
    }
}
