//! Lock free single producer multiple consumer primitive, which holds only last value.
//! Works like atomic on any struct https://doc.rust-lang.org/std/sync/atomic/
//! [`SpMcReceiver::try_recv()`] return Ok(&T) only if recieved new value from queue,
//! use [`SpMcReceiver::try_recv_or_cached()`] to get new or last one from localcopy
//! (without overhead)
//! try_recv_or_cached will hang for MAX_ITER_TILL_HANG iters if senders hang with while sending
//!
//! # Examples
//!
//! ```
//! use anyhow::Result;
//! use gts_transport::error::GtsTransportError;
//! use gts_transport::membackend::memchunk::MemChunkHolder;
//! use gts_transport::sync::lfspmc::spmc_pair;
//!
//! #[derive(Copy, Debug, Clone, Default)]
//! struct TestData {
//!     timestamp: u64,
//! }
//! let (mut tx1, mut rx1) = spmc_pair::<TestData, _>(MemChunkHolder::zeroed());
//! let res = rx1.try_recv();
//! assert!(matches!(res, Err(GtsTransportError::Unitialized)));
//!
//! let res = rx1.try_recv();
//! assert!(matches!(res, Err(GtsTransportError::Unitialized)));
//!
//! let to_send = TestData { timestamp: 222 };
//! tx1.send(&to_send).unwrap();
//! let res = rx1.try_recv();
//! assert!(res.is_ok());
//! assert_eq!(res.unwrap().timestamp, 222);
//!
//! let res = rx1.try_recv();
//! assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
//!
//! tx1.send(&to_send).unwrap();
//! let res = rx1.try_recv();
//! assert!(res.is_ok());
//! assert_eq!(res.unwrap().timestamp, 222);
//!
//! let res = rx1.try_recv();
//! assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
//! ```

use crate::error::GtsTransportError;
use crate::membackend::memholder::MemHolder;
use bytemuck::Zeroable;
use log::debug;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};

const VALUE_BITS: u32 = 1 << 24;
const GOOD_BIT: u32 = 1 << 24;

#[repr(C)]
pub struct SpMcData<T: Copy> {
    begin: AtomicU32,
    data: MaybeUninit<T>,
    end: AtomicU32,
}

unsafe impl<T: Copy> Zeroable for SpMcData<T> {}

pub struct SpMcSender<T: Copy, BackT: MemHolder<SpMcData<T>>> {
    seqnum: u32,
    back: BackT,
    _owns_t: std::marker::PhantomData<T>,
}

impl<T: Copy, BackT: MemHolder<SpMcData<T>>> SpMcSender<T, BackT> {
    pub fn new(backend: BackT) -> Self {
        Self {
            seqnum: 0,
            back: backend,
            _owns_t: std::marker::PhantomData::<T> {},
        }
    }

    pub fn send(&mut self, new_data: &T) -> Result<(), ()> {
        // SAFETY:
        // only one producer is allowed per backend.
        // we write
        // 1. atomic begin.
        // 2. chunk of data to pdata.data
        // 3. atomic end.
        // to make reader get proper data from pdata.data.
        let pdata = self.back.get_mut_ptr();

        self.seqnum = (self.seqnum + 1) % VALUE_BITS;
        let seqnum_to_store = self.seqnum | GOOD_BIT;
        // use std::intrinsics::volatile_copy_nonoverlapping_memory;
        unsafe {
            (*pdata).begin.store(seqnum_to_store, Ordering::Release);
            // write volatile is more correct, but has performance issue.
            // probably write_volatile doesn't make forget as write does.
            // TODO: investigate this.
            // std::ptr::write_volatile((*self.data).data.as_mut_ptr(), *new_data);
            // std::ptr::write((*self.data).data.as_mut_ptr(), *new_data);

            // added checks from ptr::read to construction.
            // TODO: replace with https://doc.rust-lang.org/std/intrinsics/fn.volatile_copy_nonoverlapping_memory.html
            std::ptr::copy_nonoverlapping(new_data as *const _, (*pdata).data.as_mut_ptr(), 1);
            (*pdata).end.store(seqnum_to_store, Ordering::Release);
        }

        Ok(())
    }
}

pub struct SpMcReceiver<T: Copy, BackT: MemHolder<SpMcData<T>>> {
    back: BackT,
    last_read_success: Option<u32>,
    lastcopy: MaybeUninit<T>,
}

impl<T: Copy, BackT: MemHolder<SpMcData<T>>> SpMcReceiver<T, BackT> {
    const MAX_ITER_TILL_HANG: usize = 1000;

    pub fn new(backend: BackT) -> Self {
        Self {
            back: backend,
            last_read_success: None,
            lastcopy: MaybeUninit::<_>::uninit(),
        }
    }

    // TODO: refactor to Option<(u32, MaybeUninit<T>)> without overhead?
    //       or Option<(u32, &T)> etc. to remove unsafe & ugly code here
    pub fn get_last_value(&self) -> Option<&T> {
        // SAFETY: lastcopy is only valid last_read_success != None;
        // upheld by the caller.
        // see recv() for details
        match self.last_read_success {
            Some(_) => Some(unsafe { self.lastcopy.assume_init_ref() }),
            None => None,
        }
    }

    pub fn try_recv_or_cached(&mut self) -> Result<&T, GtsTransportError> {
        for _ in 0..Self::MAX_ITER_TILL_HANG {
            match self.try_recv() {
                Ok(_) => return Ok(self.get_last_value().unwrap()),
                Err(err) => match err {
                    GtsTransportError::Inconsistent => continue,
                    _ => return Err(err),
                },
            };
        }
        debug!("try_recv_or_cached reach MAX_ITER_TILL_HANG, seriously bug in runtime");
        Err(GtsTransportError::InconsistentHang)
    }

    pub fn try_recv(&mut self) -> Result<&T, GtsTransportError> {
        // SAFETY: we read
        // 1. atomic end
        // 2. chunk of data to pdata.data
        // 3. atomic begin.
        // IFF begin == end, we could guarantee, that we read exactly the same bytes as writer
        // writed to pdata.data.
        let pdata = self.back.get_ptr();

        let (begin, end) = unsafe {
            let end = (*pdata).end.load(Ordering::Acquire);
            std::ptr::copy_nonoverlapping(&(*pdata).data, &mut self.lastcopy as *mut _, 1);
            let begin = (*pdata).begin.load(Ordering::Acquire);
            (begin, end)
        };

        // SAFETY: lastcopy is only valid iff begin == end;
        // upheld by the caller.
        // store to self.last_read_success

        if begin != end {
            self.last_read_success = None;
            return Err(GtsTransportError::Inconsistent);
        }

        let seqnum = begin;
        if seqnum & GOOD_BIT == 0 {
            return Err(GtsTransportError::Unitialized);
        }

        // same as self.get_last_value().unwrap();
        let ref_data = unsafe { self.lastcopy.assume_init_ref() };

        if Some(seqnum) == self.last_read_success {
            return Err(GtsTransportError::WouldBlock);
        }

        self.last_read_success = Some(seqnum);

        Ok(ref_data)
    }
}

pub fn spmc_pair_def<T, BackT>() -> (SpMcSender<T, BackT>, SpMcReceiver<T, BackT>)
where
    T: Copy,
    BackT: Clone + Default + MemHolder<SpMcData<T>>,
{
    let backend: BackT = Default::default();
    (SpMcSender::new(backend.clone()), SpMcReceiver::new(backend))
}

pub fn spmc_pair<T, BackT>(backend: BackT) -> (SpMcSender<T, BackT>, SpMcReceiver<T, BackT>)
where
    T: Copy,
    BackT: Clone + MemHolder<SpMcData<T>>,
{
    (SpMcSender::new(backend.clone()), SpMcReceiver::new(backend))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::membackend::memchunk::MemChunkHolder;
    use crate::membackend::shmem::ShmemHolder;

    #[derive(Copy, Debug, Clone, Default)]
    struct TestData {
        timestamp: u64,
    }

    #[test]
    fn test_simple_ping() {
        let shmem_name = "testtx1simple";
        let mut tx1 = SpMcSender::<TestData, _>::new(ShmemHolder::create(shmem_name));
        let mut rx1 = SpMcReceiver::<TestData, _>::new(ShmemHolder::connect_ro(shmem_name));

        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_simple_ping_with_unhang() {
        let shmem_name = "testtx1simpleunhang";
        let mut tx1 = SpMcSender::<TestData, _>::new(ShmemHolder::create(shmem_name));
        let mut rx1 = SpMcReceiver::<TestData, _>::new(ShmemHolder::connect_ro(shmem_name));

        let res = rx1.try_recv_or_cached();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_or_cached();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv_or_cached();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_or_cached();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv_or_cached();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_or_cached();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_simple_ping_threads() {
        let (mut tx1, mut rx1) = spmc_pair::<TestData, _>(MemChunkHolder::zeroed());

        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send(&to_send).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_heavy_pingpong() {
        //        let mut rng = rand::thread_rng();
        // The first call will take some time for calibartion

        let test_shmem1 = "testtx1heavy";
        let test_shmem2 = "testtx2heavy";
        let mut tx1 = SpMcSender::<TestData, _>::new(ShmemHolder::create(test_shmem1));
        let mut rx1 = SpMcReceiver::<TestData, _>::new(ShmemHolder::connect_ro(test_shmem1));
        let mut tx2 = SpMcSender::<TestData, _>::new(ShmemHolder::create(test_shmem2));
        let mut rx2 = SpMcReceiver::<TestData, _>::new(ShmemHolder::connect_ro(test_shmem2));

        let server = std::thread::spawn(move || {
            let mut last_val;
            loop {
                let next_val = loop {
                    let res = rx1.try_recv();
                    match res {
                        Ok(next_val) => break next_val,
                        Err(_err) => continue,
                    }
                };
                tx2.send(next_val).unwrap();
                last_val = next_val;
                if last_val.timestamp == 0 {
                    //println!("QUIT");
                    break;
                }
            }
        });
        let client = std::thread::spawn(move || {
            let mut send_data = TestData::default();

            let anc = minstant::Anchor::new();
            let total_iters = 1_000_000;
            let max_wait_iters = 100_000_000;
            let mut wait_iter = 0;
            for _cur_iter in 0..total_iters {
                let start = minstant::Instant::now();

                let timestamp = start.as_unix_nanos(&anc);
                //send_data.timestamp = timestamp;
                send_data.timestamp = timestamp;
                tx1.send(&send_data).unwrap();
                let _start = minstant::Instant::now();

                let _next_val = loop {
                    let ret = rx2.try_recv();
                    match ret {
                        Ok(next_val) => {
                            assert_eq!(next_val.timestamp, timestamp);
                            break next_val;
                        }
                        Err(err) => {
                            wait_iter += 1;
                            assert!(wait_iter < max_wait_iters);
                            match err {
                                GtsTransportError::Inconsistent => {}
                                GtsTransportError::WouldBlock => {}
                                GtsTransportError::Unitialized => {}
                                _ => {}
                            }
                            continue;
                        }
                    }
                };
            }
            send_data.timestamp = 0;
            tx1.send(&send_data).unwrap();
        });
        client.join().unwrap();
        server.join().expect("join failed");
    }
}
