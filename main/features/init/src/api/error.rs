use alloc::string::String;

/// Errors produced by vminit init operations.
#[derive(Debug)]
pub enum VmInitError {
    ConfigRead(String),
    OverlayParse(crate::api::overlay::ParseError),
}

impl core::fmt::Display for VmInitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            VmInitError::ConfigRead(s) => write!(f, "config read error: {s}"),
            VmInitError::OverlayParse(e) => write!(f, "overlay manifest parse error: {e}"),
        }
    }
}

impl From<crate::api::overlay::ParseError> for VmInitError {
    fn from(e: crate::api::overlay::ParseError) -> Self {
        VmInitError::OverlayParse(e)
    }
}
