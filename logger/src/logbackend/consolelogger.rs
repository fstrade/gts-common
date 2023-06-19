use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use crate::logclient::LogEventTs;
use core::fmt::Debug;
use gts_transport::error::GtsTransportError;
use gts_transport::membackend::memchunk::MemChunkHolder;
use gts_transport::sync::lfringspsc::{spsc_ring_pair, SpScRingData, SpScRingSender};
use log::info;
use std::cell::UnsafeCell;
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;

pub struct ConsoleThreadLogBacked<const RSIZE: usize, T>
where
    T: Copy + Send,
{
    run_flag: Arc<Mutex<bool>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    log_tx: UnsafeCell<SpScRingSender<RSIZE, T, MemChunkHolder<SpScRingData<RSIZE, T>>>>,
}

impl<T, const RSIZE: usize> ConsoleThreadLogBacked<RSIZE, LogEventTs<T>>
where
    T: Copy + Send + 'static + Debug,
{
    pub fn new(core_id: Option<usize>) -> Self {
        let flag = Arc::new(Mutex::new(false));
        // let queue = Arc::new(Mutex::new(VecDeque::<T>::new()));

        let flag_clone = flag.clone();
        // let queue_clone = queue.clone();
        let (log_tx, mut log_rx) =
            spsc_ring_pair::<RSIZE, LogEventTs<T>, _>(MemChunkHolder::zeroed());

        let join_handle = Some(std::thread::spawn(move || {
            if let Some(core_id) = core_id {
                assert!(core_affinity::set_for_current(core_affinity::CoreId {
                    id: core_id
                }));
            }
            let mut last_ts = None;
            while !*flag_clone.lock().unwrap() {
                match log_rx.try_recv() {
                    Ok(res) => {
                        let diff = last_ts.map(|val| res.timestamp - val);
                        match diff {
                            None => {
                                info!("[LOG] @{} (-) {:?}", res.timestamp, res.data);
                            }
                            Some(diff) => {
                                info!("[LOG] @{} (+{} ns) {:?}", res.timestamp, diff, res.data);
                            }
                        }
                        last_ts = Some(res.timestamp);
                    }
                    Err(GtsTransportError::WouldBlock) => {}
                    _ => unreachable!(),
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }));

        ConsoleThreadLogBacked {
            run_flag: flag,
            join_handle,
            log_tx: log_tx.into(),
        }
    }
}

impl<T, const RSIZE: usize> Drop for ConsoleThreadLogBacked<RSIZE, T>
where
    T: Copy + Send,
{
    fn drop(&mut self) {
        *self.run_flag.lock().unwrap() = true;
        self.join_handle.take().unwrap().join().unwrap();
    }
}

impl<T, const RSIZE: usize> LogBackend<T> for ConsoleThreadLogBacked<RSIZE, T>
where
    T: Copy + Send + Debug,
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
    use crate::logbackend::consolelogger::ConsoleThreadLogBacked;
    use crate::logclient::LogClient;
    use arrayvec::ArrayString;
    use serde::{Deserialize, Serialize};

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

        let log_client =
            LogClient::<_, LogEvent>::new(ConsoleThreadLogBacked::<3000, _>::new(None));

        log_client.log(event).unwrap();
        log_client.log_same(event).unwrap();
    }
}
