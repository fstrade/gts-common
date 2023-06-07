use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use core::fmt::Debug;
use gts_transport::error::GtsTransportError;
use gts_transport::membackend::memchunk::MemChunkHolder;
use gts_transport::sync::lfringspsc::{spsc_ring_pair, SpScRingData, SpScRingSender};
use std::cell::UnsafeCell;
use std::sync::mpsc::Receiver;
use std::sync::Mutex;
use std::sync::{mpsc, Arc};
use std::time::Duration;

pub struct LogContext {}

pub struct MockThreadLogBacked<const RSIZE: usize, T>
where
    T: Copy + Send,
{
    queue_rx: Receiver<T>,
    run_flag: Arc<Mutex<bool>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    log_tx: UnsafeCell<SpScRingSender<RSIZE, T, MemChunkHolder<SpScRingData<RSIZE, T>>>>,
}

impl<T, const RSIZE: usize> MockThreadLogBacked<RSIZE, T>
where
    T: Copy + Send + 'static + Debug,
{
    pub fn new() -> Self {
        let flag = Arc::new(Mutex::new(false));
        // let queue = Arc::new(Mutex::new(VecDeque::<T>::new()));

        let flag_clone = flag.clone();
        // let queue_clone = queue.clone();
        let (queue_tx, queue_rx) = mpsc::channel();
        let (log_tx, mut log_rx) = spsc_ring_pair::<RSIZE, T, _>(MemChunkHolder::zeroed());

        let join_handle = Some(std::thread::spawn(move || {
            let queue_tx = queue_tx;
            while !*flag_clone.lock().unwrap() {
                match log_rx.try_recv() {
                    Ok(res) => {
                        queue_tx.send(*res).unwrap();
                    }
                    Err(GtsTransportError::WouldBlock) => {}
                    _ => unreachable!(),
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }));

        MockThreadLogBacked {
            queue_rx,
            run_flag: flag,
            join_handle,
            log_tx: log_tx.into(),
        }
    }

    pub fn pop_front(&self) -> Option<T> {
        self.queue_rx.try_recv().ok()
    }
}
impl<T, const RSIZE: usize> Drop for MockThreadLogBacked<RSIZE, T>
where
    T: Copy + Send,
{
    fn drop(&mut self) {
        *self.run_flag.lock().unwrap() = true;
        self.join_handle.take().unwrap().join().unwrap();
    }
}

impl<T, const RSIZE: usize> LogBackend<T> for MockThreadLogBacked<RSIZE, T>
where
    T: Copy + Send,
{
    fn log(&self, event: T) -> Result<(), GtsLoggerError> {
        // SAFETY: Self is !Sync, only this function uses log_tx,
        // no reentrancy in this function.
        // but need verify reentrancy (by signal e.g.)
        // anyway refcell doesn't check signal-reentrancy either.
        let log_tx = unsafe { &mut *self.log_tx.get() };
        log_tx.send(&event).unwrap();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use crate::logbackend::threadmock::MockThreadLogBacked;
    use crate::logclient::{Log, LogClient};
    use arrayvec::ArrayString;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;

    #[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
    pub struct LogOneStruct {
        some_num: u64,
        some_other_num: u64,
        some_string: ArrayString<16>,
    }

    #[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
    pub struct LogTwoStruct {
        some_string: ArrayString<16>,
    }

    #[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
    #[serde(tag = "t", content = "c")]
    pub enum LogEvent {
        LogOneOne(LogOneStruct),
        LogTwo(LogTwoStruct),
    }

    #[test]
    fn create_logger() {
        let event = LogEvent::LogOneOne(LogOneStruct {
            some_num: 5,
            some_other_num: 7,
            some_string: ArrayString::from("333").unwrap(),
        });

        let copy_event = event;

        let log_client = LogClient::<_, LogEvent>::new(MockThreadLogBacked::<3000, _>::new());

        log_client.log(event).unwrap();

        std::thread::sleep(Duration::from_millis(200));
        let rr = log_client.backend().pop_front();
        assert!(matches!(rr, Some(ev) if ev == copy_event));

        std::thread::sleep(Duration::from_millis(200));
        let rr = log_client.backend().pop_front();
        assert!(rr.is_none());

        log_client.log(event).unwrap();
        std::thread::sleep(Duration::from_millis(200));
        let rr = log_client.backend().pop_front();
        assert!(matches!(rr, Some(ev) if ev == copy_event));

        std::thread::sleep(Duration::from_millis(200));
        let rr = log_client.backend().pop_front();
        assert!(rr.is_none());
    }
}
