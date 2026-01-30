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
    sync::Arc,
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

type Handler404 = Box<dyn Fn() -> http::Response<ResponseBody> + Send + Sync>;
type Handler500 = Box<dyn Fn(std::io::Error) -> http::Response<ResponseBody> + Send + Sync>;
struct Handlers {
    e404: Handler404,
    e500: Handler500,
}

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E: Embed> {
    _embed: PhantomData<E>,
    /// Custom error handlers.
    handlers: Arc<Handlers>,
}

impl<E: Embed> Clone for ServeEmbed<E> {
    fn clone(&self) -> Self {
        Self {
            _embed: PhantomData,
            handlers: Arc::clone(&self.handlers),
        }
    }
}

impl<E: Embed> Default for ServeEmbed<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Embed> ServeEmbed<E> {
    /// Create a new [`ServeEmbed`] service.
    pub fn new() -> Self {
        Self::builder().build::<E>()
    }

    /// Create a new [`ServeEmbedBuilder`] to customize the service.
    pub fn builder() -> ServeEmbedBuilder {
        ServeEmbedBuilder::new()
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
        ResponseFuture::new::<E, _>(&req, Arc::clone(&self.handlers))
    }
}

/// Builder for [`ServeEmbed`] service.
pub struct ServeEmbedBuilder {
    handlers: Handlers,
}

impl Default for ServeEmbedBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ServeEmbedBuilder {
    /// Create a new [`ServeEmbedBuilder`].
    pub fn new() -> Self {
        Self {
            handlers: Handlers {
                e404: Box::new(response::default_404_response),
                e500: Box::new(response::default_500_response),
            },
        }
    }

    /// Set a custom 404 error handler.
    pub fn handle_404<F>(mut self, f: F) -> Self
    where
        F: Fn() -> http::Response<ResponseBody> + Send + Sync + 'static,
    {
        self.handlers.e404 = Box::new(f);
        self
    }

    /// Set a custom 500 error handler.
    pub fn handle_500<F>(mut self, f: F) -> Self
    where
        F: Fn(std::io::Error) -> http::Response<ResponseBody> + Send + Sync + 'static,
    {
        self.handlers.e500 = Box::new(f);
        self
    }

    /// Build the [`ServeEmbed`] service.
    pub fn build<E: Embed>(self) -> ServeEmbed<E> {
        ServeEmbed {
            _embed: PhantomData,
            handlers: Arc::new(self.handlers),
        }
    }
}
