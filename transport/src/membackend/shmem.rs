//! # Examples
//!
//! Demo of shmem lfspmc `ShmemSender` `ShmemReceiver`
//! ```
//! use anyhow::Result;
//!
//! pub fn test_fun(
//! ) -> Result<(),()> {
//!     Ok(())
//! }
//!
//! # Ok::<(), anyhow::Error>(())
//! ```

use crate::membackend::memholder::MemHolder;
use bytemuck::Zeroable;
use libc::{c_int, c_void, off_t};
use libc::{close, ftruncate, mmap, munmap, shm_open, shm_unlink, PROT_READ};
use libc::{MAP_FAILED, MAP_SHARED, O_CREAT, O_RDONLY, O_RDWR, PROT_WRITE, S_IRUSR, S_IWUSR};
use log::{error, warn};
use std::ffi::CString;
use std::marker::PhantomData;

#[derive(Debug)]
enum ShmemHolderRole {
    Owner,
    Client,
}

#[derive(Debug)]
pub struct ShmemHolder<T> {
    role: ShmemHolderRole,
    fd: c_int,
    name: String,
    data: *mut T,
    // For details, see:
    // https://github.com/rust-lang/rfcs/blob/master/text/0769-sound-generic-drop.md#phantom-data
    // just to say, that Self ownes T.
    _marker: PhantomData<T>,
}

unsafe impl<T> Send for ShmemHolder<T> {}

impl<T: Zeroable> ShmemHolder<T> {
    pub fn create(name: &str) -> Self {
        let (fd, data_ptr, length) = unsafe {
            let name_cstr = CString::new(name).expect("no way!");
            let null = std::ptr::null_mut();
            let cname = name_cstr.as_ref().as_ptr();
            // println!("create {} with size = {}", name, std::mem::size_of::<T>());
            let res = shm_unlink(cname);
            if res == 0 {
                warn!(
                    "shm_unlink {} is ok. last start crashed or name collision",
                    name
                );
            }
            let length = std::mem::size_of::<T>();

            let fd = shm_open(cname, O_RDWR | O_CREAT, S_IRUSR | S_IWUSR);
            assert_ne!(fd, -1, "shm_open {} failed", name);

            let res = ftruncate(fd, length as off_t);
            assert_eq!(
                res,
                0,
                "truncate {} to {} failed",
                name,
                std::mem::size_of::<T>()
            );

            let addr = mmap(null, length, PROT_WRITE, MAP_SHARED, fd, 0);
            assert_ne!(addr, MAP_FAILED, "mmap {} failed", name);

            let data_ptr = addr as *mut T;
            std::ptr::write_bytes(data_ptr, 0x0, 1);

            (fd, data_ptr, length)
        };
        assert_eq!(length, Self::LENGTH);

        ShmemHolder {
            role: ShmemHolderRole::Owner,
            fd,
            name: name.to_string(),
            // seqnum: 0,
            data: data_ptr,
            _marker: PhantomData {},
        }
    }

    pub fn connect(name: &str, write_permission: bool) -> Self {
        let (shmem_flag, mmap_flag) = if write_permission {
            (O_RDWR, PROT_WRITE)
        } else {
            (O_RDONLY, PROT_READ)
        };

        let (fd, data_ptr, length) = unsafe {
            let name_cstr = CString::new(name).expect("no way!");
            let null = std::ptr::null_mut();
            let cname = name_cstr.as_ref().as_ptr();

            let length = std::mem::size_of::<T>();
            let fd = shm_open(cname, shmem_flag, S_IRUSR | S_IWUSR);
            assert_ne!(fd, -1, "shm_open {} failed", name);

            let addr = mmap(null, length, mmap_flag, MAP_SHARED, fd, 0);
            assert_ne!(addr, MAP_FAILED, "mmap {} failed", name);

            let data_ptr = addr as *mut T;

            (fd, data_ptr, length)
        };
        assert_eq!(length, Self::LENGTH);

        ShmemHolder {
            role: ShmemHolderRole::Client,
            fd,
            name: name.to_string(),
            data: data_ptr,
            _marker: PhantomData {},
        }
    }
}

impl<T> Drop for ShmemHolder<T> {
    fn drop(&mut self) {
        println!(
            "Drop ShmemHolder {}/{:?}/{} {:p}@{} ",
            self.fd,
            self.role,
            self.name,
            self.data,
            Self::LENGTH
        );

        let rname: &str = &self.name;
        unsafe {
            let name_cstr = CString::new(rname).expect("no way!");
            let cname = name_cstr.as_ref().as_ptr();

            let ret = munmap(self.data as *mut c_void, Self::LENGTH);
            if ret != 0 {
                error!("ShmemSender UNMAP OF {:p} -> {}", self.data, ret);
            }

            let ret = close(self.fd);
            if ret != 0 {
                error!("ShmemSender close err  OF {} -> {}", self.fd, ret);
            }

            if matches!(self.role, ShmemHolderRole::Owner) {
                let ret = shm_unlink(cname);
                if ret != 0 {
                    error!("ShmemSender shm_unlink err  OF {} -> {}", self.name, ret);
                }
            }
        }
    }
}

impl<T> MemHolder<T> for ShmemHolder<T> {
    fn get_mut_ptr(&self) -> *mut T {
        self.data
    }
    fn get_ptr(&self) -> *const T {
        self.data as *const T
    }
}
