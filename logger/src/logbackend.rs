use crate::error::GtsLoggerError;
pub mod consolelogger;
pub mod mock;
pub mod threadmock;

pub trait LogBackend<T> {
    fn log(&self, event: T) -> Result<(), GtsLoggerError>;
}
