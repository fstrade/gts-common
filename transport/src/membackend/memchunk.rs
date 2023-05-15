//! Memchunk create a mem chunk, which could use as shared mem inside one process
//! This is alternative for shared mem chunk for multithread.
//!
//! As soon as mutate Arc::as_ptr(&data).get() to *mut T is UB, we must use
//! UnsafeCell to prevent UB.
//!
//! See also https://doc.rust-lang.org/reference/behavior-considered-undefined.html

use crate::membackend::memholder::MemHolder;
use bytemuck::Zeroable;
use std::cell::UnsafeCell;
use std::sync::Arc;

#[derive(Debug)]
pub struct MemChunkHolder<T> {
    _data_holder: Arc<UnsafeCell<T>>,
    data: *mut T,
}

impl<T> Clone for MemChunkHolder<T> {
    fn clone(&self) -> Self {
        MemChunkHolder {
            _data_holder: self._data_holder.clone(),
            data: self.data,
        }
    }
}

impl<T> MemChunkHolder<T> {
    /// Safety: T must be Zeroed.
    pub unsafe fn init_zeroed() -> Self {
        // SAFETY: T must be Zeroed.
        let data = Arc::new(UnsafeCell::new(unsafe { std::mem::zeroed() }));
        let ptr = data.get();
        Self {
            _data_holder: data,
            data: ptr,
        }
    }
}

impl<T: Zeroable> MemChunkHolder<T> {
    pub fn zeroed() -> Self {
        let data = Arc::new(UnsafeCell::new(Zeroable::zeroed()));
        let ptr = data.get();
        Self {
            _data_holder: data,
            data: ptr,
        }
    }
}

impl<T: Default> Default for MemChunkHolder<T> {
    fn default() -> Self {
        let data = Arc::new(UnsafeCell::new(Default::default()));
        let ptr = data.get() as *mut T;

        Self {
            _data_holder: data,
            data: ptr,
        }
    }
}

impl<T> MemHolder<T> for MemChunkHolder<T> {
    fn get_mut_ptr(&self) -> *mut T {
        self.data
    }
    fn get_ptr(&self) -> *const T {
        self.data as *const T
    }
}
