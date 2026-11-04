//! Owned C-layout command values. The worker borrows these allocations until GPU completion.
use std::ops::Deref;
#[repr(C)]
pub(crate) struct Array<T> {
    pointer: *mut T,
    length: usize,
    capacity: usize,
}
impl<T> Default for Array<T> {
    fn default() -> Self {
        Vec::new().into()
    }
}
impl<T> From<Vec<T>> for Array<T> {
    fn from(mut v: Vec<T>) -> Self {
        let result = Self {
            pointer: v.as_mut_ptr(),
            length: v.len(),
            capacity: v.capacity(),
        };
        std::mem::forget(v);
        result
    }
}
impl<T> Deref for Array<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        // Allocations originate from Vec; this owner keeps them alive and immutable.
        unsafe { std::slice::from_raw_parts(self.pointer, self.length) }
    }
}
impl<T> Drop for Array<T> {
    fn drop(&mut self) {
        // Exactly one owner reconstructs the original Vec, including empty allocations.
        unsafe {
            drop(Vec::from_raw_parts(
                self.pointer,
                self.length,
                self.capacity,
            ));
        }
    }
}
impl<T: Clone> Clone for Array<T> {
    fn clone(&self) -> Self {
        self.to_vec().into()
    }
}
// Ownership moves across the queue; native code borrows only on the consuming thread.
unsafe impl<T: Send> Send for Array<T> {}
unsafe impl<T: Sync> Sync for Array<T> {}
impl From<String> for Array<u8> {
    fn from(s: String) -> Self {
        s.into_bytes().into()
    }
}
impl From<&str> for Array<u8> {
    fn from(s: &str) -> Self {
        s.as_bytes().to_vec().into()
    }
}
pub(crate) type Text = Array<u8>;
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct Reg {
    pub address: i64,
    pub value: i64,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct TargetCommand {
    pub width: i64,
    pub height: i64,
    pub samples: i64,
    pub generation: i64,
    pub metal: i64,
    pub bytes: i64,
    pub key: Text,
    pub file: Text,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct DepthCommand {
    pub format: i64,
    pub generation: i64,
    pub key: Text,
    pub zfile: Text,
    pub sfile: Text,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct ConstantCommand {
    pub stage: i64,
    pub slot: i64,
    pub offset: i64,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct AttributeCommand {
    pub slot: i64,
    pub stream: i64,
    pub offset: i64,
    pub format: i64,
    pub stride: i64,
    pub instanced: i64,
    pub rate: i64,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct BufferCommand {
    pub slot: i64,
    pub file: Text,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct SegmentCommand {
    pub start: i64,
    pub count: i64,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct TextureCommand {
    pub stage: i64,
    pub count: i64,
    pub image: i64,
    pub sampler: i64,
    pub width: i64,
    pub height: i64,
    pub layers: i64,
    pub array: i64,
    pub metal: i64,
    pub row: i64,
    pub rows: i64,
    pub u: i64,
    pub v: i64,
    pub min: i64,
    pub mag: i64,
    pub anisotropy: i64,
    pub compare: i64,
    pub swizzle0: i64,
    pub swizzle1: i64,
    pub swizzle2: i64,
    pub swizzle3: i64,
    pub key: Text,
    pub file: Text,
}
#[repr(C)]
#[derive(Clone, Default)]
pub(crate) struct Command {
    pub reference: i64,
    pub constant_vertex: [i64; 32],
    pub constant_fragment: [i64; 32],
    pub kind: i64,
    pub repeats: i64,
    pub clear: i64,
    pub topology: i64,
    pub first: i64,
    pub count: i64,
    pub index_size: i64,
    pub base_vertex: i64,
    pub instance: i64,
    pub generation: i64,
    pub slot: i64,
    pub local0: i64,
    pub local1: i64,
    pub local2: i64,
    pub groups0: i64,
    pub groups1: i64,
    pub groups2: i64,
    pub pipeline: Text,
    pub vertex: Text,
    pub fragment: Text,
    pub indices: Text,
    pub key: Text,
    pub source: Text,
    pub file: Text,
    pub registers: Array<Reg>,
    pub targets: Array<TargetCommand>,
    pub depth: Array<DepthCommand>,
    pub constants: Array<ConstantCommand>,
    pub attributes: Array<AttributeCommand>,
    pub buffers: Array<BufferCommand>,
    pub segments: Array<SegmentCommand>,
    pub textures: Array<TextureCommand>,
}
#[repr(C)]
pub(crate) struct View<'a, T> {
    pointer: *const T,
    length: usize,
    capacity: usize,
    lifetime: std::marker::PhantomData<&'a [T]>,
}
impl<T> Array<T> {
    pub fn borrowed(slice: &[T]) -> View<'_, T> {
        View {
            pointer: slice.as_ptr(),
            length: slice.len(),
            capacity: 0,
            lifetime: std::marker::PhantomData,
        }
    }
}
