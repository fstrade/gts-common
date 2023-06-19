use gts_transport::error::GtsTransportError;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GtsLoggerError {
    #[error("common error (({0})")]
    CommonError(String),

    #[error("GtsTransportError")]
    TransportWouldBlock(#[from] GtsTransportError),
}
