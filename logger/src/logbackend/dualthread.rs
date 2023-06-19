use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use crate::logclient::{LogClient, LogEventTs};
use gts_transport::error::GtsTransportError;
use gts_transport::membackend::memchunk::MemChunkHolder;
use gts_transport::sync::lfringspsc::{spsc_ring_pair, SpScRingData, SpScRingSender};
use log::{debug, error, info};
use minstant::Instant;
use serde::Serialize;
use std::cell::{Cell, UnsafeCell};
use std::fmt::Debug;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

pub struct DualThreadLogBacked<const RSIZE: usize, T>
where
    T: Copy + Send,
{
    // queue_rx: Receiver<T>,
    run_flag: Arc<AtomicBool>,
    join_handle_alpha: Option<std::thread::JoinHandle<()>>,
    join_handle_beta: Option<std::thread::JoinHandle<()>>,
    log_tx: UnsafeCell<SpScRingSender<RSIZE, T, MemChunkHolder<SpScRingData<RSIZE, T>>>>,
}

impl<T, const RSIZE: usize> DualThreadLogBacked<RSIZE, LogEventTs<T>>
where
    T: Copy + Send + 'static + Debug,
{
    pub fn new(fname: &str) -> Self {
        let running_flag_alpha = Arc::new(AtomicBool::new(true));
        let running_flag_beta = Arc::new(AtomicBool::new(true));
        // let queue = Arc::new(Mutex::new(VecDeque::<T>::new()));

        let running_flag_alpha_clone = running_flag_alpha.clone();
        let running_flag_beta_clone = running_flag_beta.clone();
        // let queue_clone = queue.clone();
        let (log_tx, mut log_rx) =
            spsc_ring_pair::<RSIZE, LogEventTs<T>, _>(MemChunkHolder::zeroed());

        let (queue_tx, queue_rx) = channel();

        let fname = fname.to_owned();
        let join_handle_alpha = Some(std::thread::spawn(move || {
            //let mut logs = Vec::with_capacity(3000);
            while running_flag_alpha_clone.load(Ordering::Relaxed) {
                let mut counter = 0;
                loop {
                    //while logs.len() < logs.capacity() {
                    match log_rx.try_recv() {
                        Ok(res) => {
                            //queue_tx.send(*res).unwrap();
                            queue_tx.send(*res).unwrap();
                            counter += 1;
                        }
                        Err(GtsTransportError::WouldBlock) => {
                            break;
                        }
                        _ => unreachable!(),
                    }
                }
                if counter > 0 {
                    println!("READ {} items", counter);
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            running_flag_beta.store(false, Ordering::Relaxed);
            println!("logthread-alpha closed");
        }));

        let join_handle_beta = Some(std::thread::spawn(move || {
            let mut last_send = minstant::Instant::now();

            let mut logs = Vec::with_capacity(3000);
            while running_flag_beta_clone.load(Ordering::Relaxed) {
                loop {
                    //while logs.len() < logs.capacity() {
                    match queue_rx.try_recv() {
                        Ok(res) => {
                            logs.push(res);
                            // //queue_tx.send(*res).unwrap();
                            // println!("LOG: {:?}", res);
                        }
                        Err(_) => {
                            // either empty or closed, need to break
                            break;
                        }
                    }
                }
                if !logs.is_empty()
                    && (logs.len() >= 5000 || last_send.elapsed() > Duration::from_millis(5000))
                {
                    // println!("SENDING [LOG]: {:?}", logs);
                    last_send = Instant::now();
                    let start = minstant::Instant::now();
                    let log_size = logs.len();
                    let duration = start.elapsed();

                    // if sent is good
                }
                std::thread::sleep(Duration::from_millis(500));
            }
            println!("logthread-beta closed");
        }));

        DualThreadLogBacked {
            run_flag: running_flag_alpha,
            join_handle_alpha,
            join_handle_beta,
            log_tx: log_tx.into(),
        }
    }
}

impl<T, const RSIZE: usize> Drop for DualThreadLogBacked<RSIZE, T>
where
    T: Copy + Send,
{
    fn drop(&mut self) {
        self.run_flag.store(false, Ordering::Relaxed);
        self.join_handle_alpha.take().unwrap().join().unwrap();
        self.join_handle_beta.take().unwrap().join().unwrap();
    }
}

impl<T, const RSIZE: usize> LogBackend<T> for DualThreadLogBacked<RSIZE, T>
where
    T: Copy + Send,
{
    fn log(&self, event: T) -> Result<(), GtsLoggerError> {
        // SAFETY: Self is !Sync, only this function uses log_tx,
        // no reentrancy in this function.
        // but need verify reentrancy (by signal e.g.)
        // anyway refcell doesn't check signal-reentrancy either.
        let log_tx = unsafe { &mut *self.log_tx.get() };
        log_tx.send(&event)?;
        Ok(())
    }
}
