use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::marker::PhantomData;

#[derive(Debug, Serialize, Deserialize, Copy, Clone, PartialEq)]
pub struct LogEventTs<T> {
    pub timestamp: u64,
    pub seqid: u32,
    pub data: T,
}

impl<T> LogEventTs<T> {
    pub fn new(timestamp: u64, seqid: u32, data: T) -> Self {
        LogEventTs {
            timestamp,
            seqid,
            data,
        }
    }
}

/// LogClient is simple client with timestamp.
pub struct LogClient<BackendT: LogBackend<LogEventTs<EventT>>, EventT> {
    backend: BackendT,
    _data: PhantomData<EventT>,
    anc: minstant::Anchor,
    last_ts: Cell<(u64, u32)>,
}

impl<BackendT: LogBackend<LogEventTs<EventT>>, EventT> LogClient<BackendT, EventT> {
    pub fn new(backend: BackendT) -> Self {
        Self {
            backend: backend,
            _data: PhantomData {},
            anc: minstant::Anchor::new(),
            last_ts: (0, 0).into(),
        }
    }

    pub fn log_same(&self, event: EventT) -> Result<(), GtsLoggerError> {
        let (timestamp, mut seqid) = self.last_ts.get();
        seqid += 1;
        self.last_ts.set((timestamp, seqid));

        self.backend.log(LogEventTs {
            timestamp,
            seqid,
            data: event,
        })
    }

    pub fn log(&self, event: EventT) -> Result<(), GtsLoggerError> {
        let ts = minstant::Instant::now();
        let timestamp = ts.as_unix_nanos(&self.anc);
        self.last_ts.set((timestamp, 0));

        self.backend.log(LogEventTs {
            timestamp,
            seqid: 0,
            data: event,
        })
    }

    pub fn backend(&self) -> &BackendT {
        &self.backend
    }
}
