//! Page-aligned immutable captures borrowed by Metal until the submission completes.
use std::alloc::{alloc_zeroed, dealloc, handle_alloc_error, Layout};
use std::ops::Deref;
use std::ptr::NonNull;
pub(crate) struct Upload {
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
        Self {
            pointer,
            length: bytes.len(),
            layout,
        }
    }
}
// SAFETY: captures are immutable and transferred to exactly one native submission owner.
unsafe impl Send for Upload {}
impl Deref for Upload {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        // SAFETY: the initialized allocation lives at least as long as this borrow.
        unsafe { std::slice::from_raw_parts(self.pointer.as_ptr(), self.length) }
    }
}
impl Drop for Upload {
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
            source.fill(0);
            assert_eq!(upload.pointer.as_ptr() as usize % 16384, 0);
            assert!(upload.iter().all(|&b| b == 37));
            // SAFETY: layout covers the initialized padding as well as logical bytes.
            let padded = unsafe {
                std::slice::from_raw_parts(upload.pointer.as_ptr(), upload.layout.size())
            };
            assert!(padded[length..].iter().all(|&b| b == 0));
        }
    }
}
