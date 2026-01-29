//! This crate provides a [`tower`] service designed to provide embedded static
//! assets support for web application. This service includes the following HTTP features:
//!
//! - Support for GET and HEAD requests
//! - `Content-Type` header generation based on file MIME type guessed from extension.
//! - `ETag` header generation and validation.
//! - `Last-Modified` header generation and validation.
//!
//! In `debug` mode, assets are served directly from the filesystem to facilitate rapid
//! development. Both `ETag` and `Last-Modified` headers are not generated in this mode.
//!
//! # Usage
//!
//! Please see the [examples] directory for a working example.
//!
//! [`tower`]: https://crates.io/crates/tower
//! [examples]: https://github.com/mattiapenati/tower-embed/tree/main/examples

#[cfg(not(feature = "tokio"))]
compile_error!("Only tokio runtime is supported, and it is required to use `tower-embed`.");

use std::{
    marker::PhantomData,
    task::{Context, Poll},
};

#[doc(inline)]
pub use tower_embed_impl::Embed;

#[doc(inline)]
pub use tower_embed_core as core;

#[doc(inline)]
pub use tower_embed_core::Embed;

#[doc(inline)]
pub use self::response::{ResponseBody, ResponseFuture};

#[doc(hidden)]
pub mod file;

mod response;

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E: Embed> {
    _embed: PhantomData<E>,
}

impl<E: Embed> Clone for ServeEmbed<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: Embed> Copy for ServeEmbed<E> {}

impl<E: Embed> ServeEmbed<E> {
    /// Create a new [`ServeEmbed`] service.
    pub fn new() -> Self {
        Self {
            _embed: PhantomData,
        }
    }
}

impl<E: Embed> Default for ServeEmbed<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E, ReqBody> tower_service::Service<http::Request<ReqBody>> for ServeEmbed<E>
where
    E: Embed,
{
    type Response = http::Response<ResponseBody>;
    type Error = std::convert::Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        ResponseFuture::new::<E, _>(&req)
    }
}
