use thiserror::Error;

#[derive(Debug, Error)]
pub enum GtsTransportError {
    #[error("logic error (({0})")]
    LogicError(String),

    #[error("common error (({0})")]
    CommonError(String),

    #[error("inconsistent data")]
    Inconsistent,

    #[error("inconsistent data too long (hang)")]
    InconsistentHang,

    #[error("uninitialized")]
    Unitialized,

    #[error("would block")]
    WouldBlock,

    #[error("StdIoError error")]
    StdIoError(#[from] std::io::Error),
}
