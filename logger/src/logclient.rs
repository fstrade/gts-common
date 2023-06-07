use crate::error::GtsLoggerError;
use crate::logbackend::LogBackend;
use std::marker::PhantomData;

pub struct LogClient<BackendT: LogBackend<EventT>, EventT> {
    backend: BackendT,
    _data: PhantomData<EventT>,
}

pub trait Log {
    type Backend;
    type Event;

    fn new(backend: Self::Backend) -> Self;
    fn log(&self, event: Self::Event) -> Result<(), GtsLoggerError>;
    fn backend(&self) -> &Self::Backend;
}

impl<BackendT: LogBackend<EventT>, EventT> Log for LogClient<BackendT, EventT> {
    type Backend = BackendT;
    type Event = EventT;

    fn new(backend: Self::Backend) -> Self {
        Self {
            backend: backend,
            _data: PhantomData {},
        }
    }

    fn log(&self, event: Self::Event) -> Result<(), GtsLoggerError> {
        self.backend.log(event)
    }

    fn backend(&self) -> &Self::Backend {
        &self.backend
    }
}
