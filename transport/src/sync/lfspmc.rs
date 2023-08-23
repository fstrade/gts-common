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
//! let (mut tx1, mut rx1) = spmc_pair::<TestData, TestData, _, 1>(MemChunkHolder::zeroed());
//! let res = rx1.try_recv_info();
//! assert!(matches!(res, Err(GtsTransportError::Unitialized)));
//!
//! let res = rx1.try_recv_info();
//! assert!(matches!(res, Err(GtsTransportError::Unitialized)));
//!
//! let to_send = TestData { timestamp: 222 };
//! tx1.send_info(&to_send).unwrap();
//! let res = rx1.try_recv_info();
//! assert!(res.is_ok());
//! assert_eq!(res.unwrap().timestamp, 222);
//!
//! let res = rx1.try_recv_info();
//! assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
//!
//! tx1.send_info(&to_send).unwrap();
//! let res = rx1.try_recv_info();
//! assert!(res.is_ok());
//! assert_eq!(res.unwrap().timestamp, 222);
//!
//! let res = rx1.try_recv_info();
//! assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
//! ```

use crate::error::GtsTransportError;
use crate::membackend::memholder::MemHolder;
use bytemuck::Zeroable;
use log::debug;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};

//TODO: add cargo cfg param for this constant
// const CACHE_LINE_SIZE: usize = 64;

const VALUE_BITS: u32 = 1 << 24;
const GOOD_BIT: u32 = 1 << 24;

#[repr(C)]
pub struct SpMcData2<T: Copy> {
    begin: AtomicU32,
    data: MaybeUninit<T>,
    end: AtomicU32,
}

#[repr(C)]
pub struct SubSpMcData<T: Copy> {
    begin: AtomicU32,
    data: MaybeUninit<T>,
    end: AtomicU32,
}

#[repr(C)]
pub struct SpMcData<InfoT: Copy, T: Copy, const NT: usize> {
    info: SubSpMcData<InfoT>,
    slots: [SubSpMcData<T>; NT],
}

unsafe impl<InfoT: Copy, T: Copy, const NT: usize> Zeroable for SpMcData<InfoT, T, NT> {}

//pub struct SpMcSender<T: Copy, BackT: MemHolder<SpMcData<T>>> {
pub struct SpMcSender<
    InfoT: Copy,
    T: Copy,
    BackT: MemHolder<SpMcData<InfoT, T, NT>>,
    const NT: usize,
> {
    seqnum: u32,
    seqnum_slot: [u32; NT],
    back: BackT,
    _owns_t: std::marker::PhantomData<T>,
    _owns_it: std::marker::PhantomData<InfoT>,
}

impl<InfoT: Copy, T: Copy, BackT: MemHolder<SpMcData<InfoT, T, NT>>, const NT: usize>
    SpMcSender<InfoT, T, BackT, NT>
{
    pub const SIZE: usize = NT;

    pub fn new(backend: BackT) -> Self {
        // if std::mem::size_of::<SubSpMcData<T>>() % CACHE_LINE_SIZE != 0 {
        //     println!(
        //         "== WARN ==: std::mem::size_of<SpMcData<T>> % CACHE_LINE_SIZE doesn't align: {} {}",
        //         std::mem::size_of::<SubSpMcData<T>>(),
        //         CACHE_LINE_SIZE
        //     );
        // }
        //
        // if std::mem::size_of::<SubSpMcData<InfoT>>() % CACHE_LINE_SIZE != 0 {
        //     println!(
        //         "== WARN ==: std::mem::size_of<SpMcData<T>> % CACHE_LINE_SIZE doesn't align: {} {}",
        //         std::mem::size_of::<SubSpMcData<InfoT>>(),
        //         CACHE_LINE_SIZE
        //     );
        // }
        //
        // let mc_data = backend.get_ptr();
        //
        // unsafe {
        //     let pinfo = &(*mc_data).info as *const _;
        //     println!("pinfo = {:p} align: {}", pinfo, (pinfo as usize) % CACHE_LINE_SIZE);
        //     for idx in 0..NT {
        //         let pdata = &(*mc_data).slots[idx] as *const _;
        //         println!(
        //             "== WARN == pdata[{}] = {:p} align: {}",
        //             idx,
        //             pdata,
        //             (pdata as usize) % CACHE_LINE_SIZE
        //         );
        //     }
        // }

        Self {
            seqnum: 0,
            seqnum_slot: [0; NT],
            back: backend,
            _owns_t: std::marker::PhantomData {},
            _owns_it: std::marker::PhantomData {},
        }
    }

    pub fn send_info(&mut self, new_data: &InfoT) -> Result<(), GtsTransportError> {
        let pdata = self.back.get_mut_ptr();
        let pslot = unsafe { &mut (*pdata).info as *mut _ };
        Self::send_int(pslot, new_data, &mut self.seqnum)
    }

    pub fn send_slot(&mut self, idx: usize, new_data: &T) -> Result<(), GtsTransportError> {
        assert!(idx < NT);
        let pdata = self.back.get_mut_ptr();
        let pslot = unsafe { &mut (*pdata).slots[idx] as *mut _ };
        Self::send_int(pslot, new_data, &mut self.seqnum_slot[idx])
    }

    fn send_int<TT: Copy>(
        pdata: *mut SubSpMcData<TT>,
        new_data: &TT,
        seqnum: &mut u32,
    ) -> Result<(), GtsTransportError> {
        // SAFETY:
        // only one producer is allowed per backend.
        // we write
        // 1. atomic begin.
        // 2. chunk of data to pdata.data
        // 3. atomic end.
        // to make reader get proper data from pdata.data.

        *seqnum = (*seqnum + 1) % VALUE_BITS;
        let seqnum_to_store = *seqnum | GOOD_BIT;
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

pub struct SpMcReceiver<
    InfoT: Copy,
    T: Copy,
    BackT: MemHolder<SpMcData<InfoT, T, NT>>,
    const NT: usize,
> {
    back: BackT,
    last_read_success_info: Option<u32>,
    last_read_success_slot: [Option<u32>; NT],
    lastcopy_slot: [MaybeUninit<T>; NT],
    lastcopy_info: MaybeUninit<InfoT>,
}

impl<InfoT: Copy, T: Copy, BackT: MemHolder<SpMcData<InfoT, T, NT>> + Clone, const NT: usize> Clone
    for SpMcReceiver<InfoT, T, BackT, NT>
{
    fn clone(&self) -> Self {
        Self {
            back: self.back.clone(),
            last_read_success_info: None,
            last_read_success_slot: [None; NT],
            lastcopy_info: MaybeUninit::<_>::uninit(),
            lastcopy_slot: unsafe { MaybeUninit::<[MaybeUninit<_>; NT]>::uninit().assume_init() },
        }
    }
}

//impl<T: Copy, BackT: MemHolder<SpMcData<T>>> SpMcReceiver<T, BackT> {
impl<InfoT: Copy, T: Copy, BackT: MemHolder<SpMcData<InfoT, T, NT>>, const NT: usize>
    SpMcReceiver<InfoT, T, BackT, NT>
{
    pub const MAX_ITER_TILL_HANG: usize = 1000;
    pub const SIZE: usize = NT;

    pub fn new(backend: BackT) -> Self {
        // SAFETY: An uninitialized `[MaybeUninit<_>; LEN]` is valid.
        // see [MaybeUninit::uninit_array]
        Self {
            back: backend,
            last_read_success_info: None,
            last_read_success_slot: [None; NT],
            lastcopy_info: MaybeUninit::<_>::uninit(),
            lastcopy_slot: unsafe { MaybeUninit::<[MaybeUninit<_>; NT]>::uninit().assume_init() },
        }
    }

    // TODO: refactor to _smth like_ Option<(u32, MaybeUninit<T>)> without overhead?
    //       or Option<(u32, &T)> etc. to remove unsafe & ugly code here
    //       N.B. we can't just store &T in self.last_read_* as soon as try_recv could change it
    //       store of &T/&InfoT by caller prevents from calling try_recv here.
    //       even for another slot.
    pub fn get_last_info(&self) -> Option<&InfoT> {
        // SAFETY: lastcopy is only valid last_read_success != None;
        // upheld by the caller.
        // see recv() for details
        match self.last_read_success_info {
            Some(_) => Some(unsafe { self.lastcopy_info.assume_init_ref() }),
            None => None,
        }
    }

    pub fn get_last_slot(&self, idx: usize) -> Option<&T> {
        // SAFETY: lastcopy is only valid last_read_success != None;
        // upheld by the caller.
        // see recv() for details
        match self.last_read_success_slot[idx] {
            Some(_) => Some(unsafe { self.lastcopy_slot[idx].assume_init_ref() }),
            None => None,
        }
    }

    pub fn try_recv_info_multi(&mut self) -> Result<&InfoT, GtsTransportError> {
        for _ in 0..Self::MAX_ITER_TILL_HANG {
            match self.try_recv_info() {
                Ok(_) => return Ok(self.get_last_info().unwrap()),
                Err(err) => match err {
                    GtsTransportError::Inconsistent => continue,
                    _ => return Err(err),
                },
            };
        }
        debug!("try_recv_or_cached reach MAX_ITER_TILL_HANG, seriously bug in runtime");
        Err(GtsTransportError::InconsistentHang)
    }

    pub fn try_recv_slot_multi(&mut self, idx: usize) -> Result<&T, GtsTransportError> {
        for _ in 0..Self::MAX_ITER_TILL_HANG {
            match self.try_recv_slot(idx) {
                Ok(_) => return Ok(self.get_last_slot(idx).unwrap()),
                Err(err) => match err {
                    GtsTransportError::Inconsistent => continue,
                    _ => return Err(err),
                },
            };
        }
        debug!("try_recv_or_cached reach MAX_ITER_TILL_HANG, seriously bug in runtime");
        Err(GtsTransportError::InconsistentHang)
    }

    pub fn try_recv_info(&mut self) -> Result<&InfoT, GtsTransportError> {
        let pdata = self.back.get_ptr();
        let pslot = unsafe { &(*pdata).info as *const _ };
        Self::try_recv_int(
            pslot,
            &mut self.lastcopy_info,
            &mut self.last_read_success_info,
        )
    }

    pub fn try_recv_slot(&mut self, idx: usize) -> Result<&T, GtsTransportError> {
        assert!(idx < NT);
        let pdata = self.back.get_ptr();
        let pslot = unsafe { &(*pdata).slots[idx] as *const _ };

        Self::try_recv_int(
            pslot,
            &mut self.lastcopy_slot[idx],
            &mut self.last_read_success_slot[idx],
        )
    }

    fn try_recv_int<'a, TT: Copy>(
        pdata: *const SubSpMcData<TT>,
        localcopy: &'a mut MaybeUninit<TT>,
        last_success_read: &'a mut Option<u32>,
    ) -> Result<&'a TT, GtsTransportError> {
        // SAFETY: we read
        // 1. atomic end
        // 2. chunk of data to pdata.data
        // 3. atomic begin.
        // IFF begin == end, we could guarantee, that we read exactly the same bytes as writer
        // writed to pdata.data.

        let (begin, end) = unsafe {
            let end = (*pdata).end.load(Ordering::Acquire);
            std::ptr::copy_nonoverlapping(&(*pdata).data, localcopy as *mut _, 1);
            let begin = (*pdata).begin.load(Ordering::Acquire);
            (begin, end)
        };

        // SAFETY: lastcopy is only valid iff begin == end;
        // upheld by the caller.
        // store to self.last_read_success

        if begin != end {
            *last_success_read = None;
            return Err(GtsTransportError::Inconsistent);
        }

        let seqnum = begin;
        if seqnum & GOOD_BIT == 0 {
            return Err(GtsTransportError::Unitialized);
        }

        if Some(seqnum) == *last_success_read {
            return Err(GtsTransportError::WouldBlock);
        }

        let ref_data = unsafe { localcopy.assume_init_ref() };
        *last_success_read = Some(seqnum);

        Ok(ref_data)
    }
}

pub fn spmc_pair_def<InfoT, T, BackT, const NT: usize>() -> (
    SpMcSender<InfoT, T, BackT, NT>,
    SpMcReceiver<InfoT, T, BackT, NT>,
)
where
    InfoT: Copy,
    T: Copy,
    BackT: Clone + Default + MemHolder<SpMcData<InfoT, T, NT>>,
{
    let backend: BackT = Default::default();
    (SpMcSender::new(backend.clone()), SpMcReceiver::new(backend))
}

pub fn spmc_pair<InfoT, T, BackT, const NT: usize>(
    backend: BackT,
) -> (
    SpMcSender<InfoT, T, BackT, NT>,
    SpMcReceiver<InfoT, T, BackT, NT>,
)
where
    InfoT: Copy,
    T: Copy,
    BackT: Clone + MemHolder<SpMcData<InfoT, T, NT>>,
{
    (SpMcSender::new(backend.clone()), SpMcReceiver::new(backend))
}

pub fn spmc_sender<InfoT, T, BackT, const NT: usize>(
    backend: BackT,
) -> SpMcSender<InfoT, T, BackT, NT>
where
    InfoT: Copy,
    T: Copy,
    BackT: MemHolder<SpMcData<InfoT, T, NT>>,
{
    SpMcSender::new(backend)
}

pub fn spmc_receiver<InfoT, T, BackT, const NT: usize>(
    backend: BackT,
) -> SpMcReceiver<InfoT, T, BackT, NT>
where
    InfoT: Copy,
    T: Copy,
    BackT: MemHolder<SpMcData<InfoT, T, NT>>,
{
    SpMcReceiver::new(backend)
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
        let mut tx1 =
            SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(shmem_name).unwrap());
        let mut rx1 = SpMcReceiver::<TestData, TestData, _, 1>::new(
            ShmemHolder::connect_ro(shmem_name).unwrap(),
        );

        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_simple_ping_with_unhang() {
        let shmem_name = "testtx1simpleunhang";
        let mut tx1 =
            SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(shmem_name).unwrap());
        let mut rx1 = SpMcReceiver::<TestData, TestData, _, 1>::new(
            ShmemHolder::connect_ro(shmem_name).unwrap(),
        );

        let res = rx1.try_recv_info_multi();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_info_multi();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info_multi();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_info_multi();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info_multi();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_info_multi();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_change_of_reference() {
        let (mut tx1, mut rx1) = spmc_pair::<TestData, TestData, _, 2>(MemChunkHolder::zeroed());
        let mut rx2 = rx1.clone();
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let rr = rx1.get_last_info();
        assert!(rr.is_none());

        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        // will not work

        let to_send = TestData { timestamp: 222 };
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);

        let res = rx2.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);

        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        let res = rx2.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.get_last_info();
        assert!(res.is_some());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx2.get_last_info();

        // TODO: add check for next line fail to compile (smth like compile time try_borrow)
        // this will not compile. it's fine
        // let _res2 = rx2.try_recv_slot(1);

        assert!(res.is_some());
        assert_eq!(res.unwrap().timestamp, 222);
    }

    #[test]
    fn test_simple_ping_threads() {
        let (mut tx1, mut rx1) = spmc_pair::<TestData, TestData, _, 2>(MemChunkHolder::zeroed());
        let mut rx2 = rx1.clone();
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx2.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));

        let to_send = TestData { timestamp: 222 };
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);

        let res = rx2.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);

        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send_info(&to_send).unwrap();
        let res = rx1.try_recv_info();
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_info();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.try_recv_slot(0);
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_slot(0);
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 222 };
        tx1.send_slot(0, &to_send).unwrap();
        let res = rx1.try_recv_slot(0);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_slot(0);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send_slot(0, &to_send).unwrap();
        let res = rx1.try_recv_slot(0);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 222);
        let res = rx1.try_recv_slot(0);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.try_recv_slot(1);
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let res = rx1.try_recv_slot(1);
        assert!(matches!(res, Err(GtsTransportError::Unitialized)));
        let to_send = TestData { timestamp: 333 };
        tx1.send_slot(1, &to_send).unwrap();
        let res = rx1.try_recv_slot(1);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 333);
        let res = rx1.try_recv_slot(1);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send_slot(1, &to_send).unwrap();

        let res = rx1.try_recv_slot(0);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.try_recv_slot(1);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 333);

        let res = rx2.try_recv_slot(1);
        assert!(res.is_ok());
        assert_eq!(res.unwrap().timestamp, 333);

        let res = rx1.try_recv_slot(1);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx2.try_recv_slot(1);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }

    #[test]
    fn test_heavy_pingpong() {
        //        let mut rng = rand::thread_rng();
        // The first call will take some time for calibartion

        let test_shmem1 = "testtx1heavy";
        let test_shmem2 = "testtx2heavy";
        let mut tx1 =
            SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(test_shmem1).unwrap());
        let mut rx1 = SpMcReceiver::<TestData, TestData, _, 1>::new(
            ShmemHolder::connect_ro(test_shmem1).unwrap(),
        );
        let mut tx2 =
            SpMcSender::<TestData, TestData, _, 1>::new(ShmemHolder::create(test_shmem2).unwrap());
        let mut rx2 = SpMcReceiver::<TestData, TestData, _, 1>::new(
            ShmemHolder::connect_ro(test_shmem2).unwrap(),
        );

        let server = std::thread::spawn(move || {
            let mut last_val;
            loop {
                let next_val = loop {
                    let res = rx1.try_recv_info();
                    match res {
                        Ok(next_val) => break next_val,
                        Err(_err) => continue,
                    }
                };
                tx2.send_info(next_val).unwrap();
                last_val = next_val;
                if last_val.timestamp == 0 {
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
                tx1.send_info(&send_data).unwrap();
                let _start = minstant::Instant::now();

                let _next_val = loop {
                    let ret = rx2.try_recv_info();
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
            tx1.send_info(&send_data).unwrap();
        });
        client.join().unwrap();
        server.join().expect("join failed");
    }
}
