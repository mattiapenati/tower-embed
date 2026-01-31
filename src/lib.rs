//! **tower-embed** is a [`tower`] service that efficiently serves embedded static assets
//! in Rust web applications. It provides a production-ready solution for bundling and
//! serving static files (HTML, CSS, JavaScript, images, etc.) directly within your
//! compiled binary, eliminating the need for external file deployments.
//!
//! ## Features
//!
//! This service includes comprehensive HTTP features for optimal asset delivery:
//!
//! - **HTTP Method Support**: GET and HEAD requests
//! - **Smart Content Detection**: Automatic `Content-Type` header generation based on file MIME type detection
//! - **Efficient Caching**: `ETag` and `Last-Modified` header generation and validation
//! - **Development-Friendly**: In `debug` mode, assets are served directly from the filesystem for rapid iteration
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
