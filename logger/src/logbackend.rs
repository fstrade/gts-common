use crate::error::GtsLoggerError;
pub mod mock;

pub trait LogBackend<T> {
    fn log(&self, event: T) -> Result<(), GtsLoggerError>;
}
