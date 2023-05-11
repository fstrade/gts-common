//! trait for MemHolder,
//! It's UB to cast get_mut_ptr or get_ptr to &T or &mut T
//! as soon as underlying objects could mutate.
//!
//! See also https://doc.rust-lang.org/reference/behavior-considered-undefined.html

pub trait MemHolder<T> {
    const LENGTH: usize = std::mem::size_of::<T>();

    fn get_mut_ptr(&self) -> *mut T;
    fn get_ptr(&self) -> *const T;
}
