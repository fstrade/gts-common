use thiserror::Error;

#[derive(Debug, Error)]
pub enum GtsLoggerError {
    #[error("common error (({0})")]
    CommonError(String),
}
