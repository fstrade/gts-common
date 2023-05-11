use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use std::marker::PhantomData;

pub struct LogClient<BackendT: LogBackend<EventT>, EventT> {
    backend: BackendT,
    _data: PhantomData<EventT>,
}

impl<BackendT: LogBackend<EventT>, EventT> LogClient<BackendT, EventT> {
    pub fn new(backend: BackendT) -> Self {
        Self {
            backend: backend,
            _data: PhantomData {},
        }
    }

    pub fn log(&self, event: EventT) -> Result<(), GtsLoggerError> {
        self.backend.log(event)
    }

    pub fn backend(&self) -> &BackendT {
        &self.backend
    }
}
