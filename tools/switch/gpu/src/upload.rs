//! Page-aligned immutable captures borrowed by Metal until the submission completes.
use std::alloc::{Layout, alloc_zeroed, dealloc, handle_alloc_error};
use std::ops::Deref;
use std::ptr::NonNull;
#[derive(Clone)]
pub(crate) struct Upload(std::sync::Arc<Storage>);
struct Storage {
    pointer: NonNull<u8>,
    length: usize,
    layout: Layout,
}
impl Upload {
    pub fn copy(bytes: &[u8]) -> Self {
        let capacity = bytes.len().max(1).next_multiple_of(16384);
        let layout = Layout::from_size_align(capacity, 16384).expect("upload layout");
        // SAFETY: layout is nonzero; this object exclusively owns the allocation.
        let pointer = NonNull::new(unsafe { alloc_zeroed(layout) })
            .unwrap_or_else(|| handle_alloc_error(layout));
        // SAFETY: capacity covers bytes and the source cannot alias this new allocation.
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer.as_ptr(), bytes.len());
        }
        Self(std::sync::Arc::new(Storage {
            pointer,
            length: bytes.len(),
            layout,
        }))
    }
}
// SAFETY: storage is immutable; owners retain it until every native use completes.
unsafe impl Send for Storage {}
unsafe impl Sync for Storage {}
impl Deref for Upload {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY: the initialized allocation lives at least as long as this borrow.
        unsafe { std::slice::from_raw_parts(self.0.pointer.as_ptr(), self.0.length) }
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        // SAFETY: worker waits for GPU completion before releasing the batch owner.
        unsafe {
            dealloc(self.pointer.as_ptr(), self.layout);
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn captured_upload_is_aligned_owned_and_zero_padded() {
        for length in [0, 1, 16383, 16384, 16385] {
            let mut source = vec![37; length];
            let upload = Upload::copy(&source);
            let upload = {
                let retained = upload.clone();
                drop(upload);
                retained
            };
            source.fill(0);
            assert_eq!(upload.0.pointer.as_ptr() as usize % 16384, 0);
            assert!(upload.iter().all(|&b| b == 37));
            // SAFETY: layout covers the initialized padding as well as logical bytes.
            let padded = unsafe {
                std::slice::from_raw_parts(upload.0.pointer.as_ptr(), upload.0.layout.size())
            };
            assert!(padded[length..].iter().all(|&b| b == 0));
        }
    }
}
