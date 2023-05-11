use crate::error::GtsTransportError;
use crate::membackend::memholder::MemHolder;
use bytemuck::Zeroable;
use std::mem::MaybeUninit;
use std::sync::atomic::{AtomicU32, Ordering};

//TODO: use some lib like
//   https://github.com/lovesegfault/cache-size/blob/master/src/x86.rs

const CACHE_LINE_SIZE: usize = 64;

/// SpScRingData have 2 sections:
///     1) read_done_seqnum for writes of reciever, read of sender
///     2) write_done_seqnum+data for writes of sender, read of reciever
///
/// to eliminate cache coherence, we must put this data to separate cache lines,
/// In this scenario, we have only 1 core which will write to each cacheline and
/// this cacheline on this core is always up to date, so there is no invalidate penalty
/// (by modifying read_done_seqnum) for write to it.
#[repr(C)]
pub struct SpScRingData<const RSIZE: usize, T: Copy> {
    pub read_done_seqnum: AtomicU32,
    _padding_one: [u8; CACHE_LINE_SIZE - { std::mem::size_of::<AtomicU32>() }],
    pub write_done_seqnum: AtomicU32,
    // pub data: [MaybeUninit<T>; RSIZE + 1],
    pub data: [MaybeUninit<T>; RSIZE],
}

unsafe impl<const RSIZE: usize, T: Copy> Zeroable for SpScRingData<RSIZE, T> {}

pub struct SpScRingSender<const RSIZE: usize, T: Copy, BackT: MemHolder<SpScRingData<RSIZE, T>>> {
    last_send_seqnum: u32,
    back: BackT,
    _owns_t: std::marker::PhantomData<T>,
}

impl<const RSIZE: usize, T: Copy, BackT: MemHolder<SpScRingData<RSIZE, T>>>
    SpScRingSender<RSIZE, T, BackT>
{
    const RING_SIZE: u32 = RSIZE as u32;

    pub fn new(backend: BackT) -> Self {
        Self {
            last_send_seqnum: 0,
            back: backend,
            _owns_t: std::marker::PhantomData::<T> {},
        }
    }

    pub fn send(&mut self, new_data: &T) -> Result<(), GtsTransportError> {
        // SAFETY:
        // only one producer is allowed per backend.
        // we write
        // 1. check advance(send_seqnum) != recv_seqnum
        // 2. write data to send_seqnum+1
        // 3. advance send_seqnum
        // to make reader get proper data from pdata.data.

        // let pdata = unsafe { self.back.get_mut_ptr() };

        let pdata = self.back.get_mut_ptr();

        let next_seqnum = (self.last_send_seqnum + 1) % Self::RING_SIZE;
        let read_seqnum = unsafe { (*pdata).read_done_seqnum.load(Ordering::Acquire) };

        if read_seqnum == next_seqnum {
            return Err(GtsTransportError::WouldBlock);
        }

        // println!("send send_seqnum = {}", next_seqnum);
        // println!("send read_seqnum = {}", read_seqnum);

        self.last_send_seqnum = next_seqnum;
        unsafe {
            std::ptr::copy_nonoverlapping(
                new_data as *const _,
                (*pdata).data[next_seqnum as usize].as_mut_ptr(),
                1,
            );
            (*pdata)
                .write_done_seqnum
                .store(self.last_send_seqnum, Ordering::Release);
        }

        Ok(())
    }
}

pub struct SpScRingReceiver<const RSIZE: usize, T: Copy, BackT: MemHolder<SpScRingData<RSIZE, T>>> {
    back: BackT,
    last_read_seqnum: Option<u32>,
    last_copy: MaybeUninit<T>,
}

impl<const RSIZE: usize, T: Copy, BackT: MemHolder<SpScRingData<RSIZE, T>>>
    SpScRingReceiver<RSIZE, T, BackT>
{
    const RING_SIZE: u32 = RSIZE as u32;

    pub fn new(backend: BackT) -> Self {
        SpScRingReceiver {
            back: backend,
            last_read_seqnum: None,
            last_copy: MaybeUninit::uninit(),
        }
    }

    // TODO: refactor to Option<(u32, MaybeUninit<T>)> without overhead?
    //       or Option<(u32, &T)> etc. to remove unsafe & ugly code here
    pub fn get_last_value(&self) -> Option<&T> {
        match self.last_read_seqnum {
            Some(_) => Some(unsafe { self.last_copy.assume_init_ref() }),
            None => None,
        }
    }

    pub fn try_recv(&mut self) -> Result<&T, GtsTransportError> {
        // SAFETY: we read
        // 1) check read_seqnum != write_seqnum, otherwise return GtsTransportError::WouldBlock
        // 2) read(copy) data from data[write_seqnum]
        // 2) advance read_seqnum
        //let pdata = unsafe { self.back.get_ref() };
        let pdata = self.back.get_mut_ptr();

        let (send_seqnum, read_seqnum) = unsafe {
            let send_seqnum = (*pdata).write_done_seqnum.load(Ordering::Acquire);
            let read_seqnum = (*pdata).read_done_seqnum.load(Ordering::Acquire);
            (send_seqnum, read_seqnum)
        };

        // println!("try_recv send_seqnum = {}", send_seqnum);
        // println!("try_recv read_seqnum = {}", read_seqnum);

        if send_seqnum == read_seqnum {
            return Err(GtsTransportError::WouldBlock);
        }
        let next_read = (read_seqnum + 1) % Self::RING_SIZE;
        // println!("try_recv next_read = {}", next_read);

        unsafe {
            std::ptr::copy_nonoverlapping(
                &(*pdata).data[next_read as usize],
                &mut self.last_copy as *mut _,
                1,
            );

            (*pdata)
                .read_done_seqnum
                .store(next_read as u32, Ordering::Release);
        }

        let ref_data = unsafe { self.last_copy.assume_init_ref() };
        Ok(ref_data)
    }
}

pub fn spsc_ring_pair<const RSIZE: usize, T, BackT>(
    backend: BackT,
) -> (
    SpScRingSender<RSIZE, T, BackT>,
    SpScRingReceiver<RSIZE, T, BackT>,
)
where
    T: Copy,
    BackT: Clone + MemHolder<SpScRingData<RSIZE, T>>,
{
    (
        SpScRingSender::new(backend.clone()),
        SpScRingReceiver::new(backend),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::membackend::memchunk::MemChunkHolder;

    #[derive(Copy, Clone, Debug, Default)]
    pub struct TestData {
        timestamp: u64,
        _timestamp2: u64,
        _timestamp3: u64,
        _timestamp4: u64,
    }
    #[derive(Copy, Clone, Debug)]
    pub enum TestDataEnum {
        TestData(TestData),
    }

    #[test]
    pub fn test_sizes() {
        let test_data = SpScRingData::<10, TestDataEnum>::zeroed();
        let addr_of_read_done = std::ptr::addr_of!(test_data.read_done_seqnum);
        let addr_of_write_done = std::ptr::addr_of!(test_data.write_done_seqnum);
        let addr_of_data_done = std::ptr::addr_of!(test_data.data);

        assert!((addr_of_write_done as usize) == (addr_of_read_done as usize + CACHE_LINE_SIZE));
        assert!((addr_of_data_done as usize) > (addr_of_read_done as usize + CACHE_LINE_SIZE));
    }

    #[test]
    pub fn test_simple_with_threads() {
        let (mut tx1, mut rx1) = spsc_ring_pair::<3, TestDataEnum, _>(MemChunkHolder::zeroed());

        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        let to_send1 = TestDataEnum::TestData(TestData {
            timestamp: 111,
            ..Default::default()
        });
        let to_send2 = TestDataEnum::TestData(TestData {
            timestamp: 222,
            ..Default::default()
        });
        let to_send3 = TestDataEnum::TestData(TestData {
            timestamp: 333,
            ..Default::default()
        });
        let to_send4 = TestDataEnum::TestData(TestData {
            timestamp: 444,
            ..Default::default()
        });
        tx1.send(&to_send1).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 111));
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
        tx1.send(&to_send2).unwrap();
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 222));
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        // overflow.
        let res = tx1.send(&to_send1);
        assert!(matches!(res, Ok(())));
        let res = tx1.send(&to_send2);
        assert!(matches!(res, Ok(())));

        let res = tx1.send(&to_send3);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 111));

        // now there is 1 free room.
        let res = tx1.send(&to_send3);
        assert!(matches!(res, Ok(())));

        let res = tx1.send(&to_send4);
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));

        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 222));

        let res = tx1.send(&to_send4);
        assert!(matches!(res, Ok(())));

        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 333));
        let res = rx1.try_recv();
        assert!(res.is_ok());
        assert!(matches!(res.unwrap(), TestDataEnum::TestData(td) if td.timestamp == 444));

        // no data left
        let res = rx1.try_recv();
        assert!(matches!(res, Err(GtsTransportError::WouldBlock)));
    }
}
