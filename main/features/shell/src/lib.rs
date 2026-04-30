#![cfg_attr(not(feature = "std"), no_std)]
#[cfg(not(feature = "std"))]
extern crate alloc;

pub(crate) mod api;
mod core;
mod spi;
mod saf;

pub use saf::*;
