//! Core functionalities of tower-embed.

#[cfg(not(feature = "tokio"))]
compile_error!("Only tokio runtime is supported, and it is required to use `tower-embed`.");

use std::error::Error;

#[doc(hidden)]
pub use http;

pub use self::{body::*, embedded::*, service::*};

mod body;
mod embedded;
mod service;

pub mod headers;
pub mod response;

#[cfg(feature = "tokio")]
pub mod file;

/// Type-erased error type.
pub type BoxError = Box<dyn Error + Send + Sync>;
