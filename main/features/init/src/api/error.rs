use thiserror::Error;

/// Errors produced by vminit init operations.
#[derive(Debug, Error)]
pub enum VmInitError {
    #[error("config read error: {0}")]
    ConfigRead(#[from] std::io::Error),

    #[error("overlay manifest parse error: {0}")]
    OverlayParse(#[from] crate::api::overlay::ParseError),
}
