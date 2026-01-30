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
//! ```no_run
//! use axum::Router;
//! use tower_embed::{Embed, ServeEmbed};
//!
//! #[derive(Embed)]
//! #[embed(folder = "assets")]
//! struct Assets;
//!
//! #[tokio::main]
//! async fn main() {
//!     let assets = ServeEmbed::<Assets>::new();
//!     let router = Router::new().fallback_service(assets);
//!
//!     let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
//!         .await
//!         .unwrap();
//!     axum::serve::serve(listener, router).await.unwrap();
//! }
//! ```
//!
//! Please see the [examples] directory for a working example.
//!
//! [`tower`]: https://crates.io/crates/tower
//! [examples]: https://github.com/mattiapenati/tower-embed/tree/main/examples

#[cfg(not(feature = "tokio"))]
compile_error!("Only tokio runtime is supported, and it is required to use `tower-embed`.");

use std::{
    convert::Infallible,
    marker::PhantomData,
    task::{Context, Poll},
};

use tower::util::BoxCloneSyncService;
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

type NotFoundService =
    tower::util::BoxCloneSyncService<http::Request<()>, http::Response<ResponseBody>, Infallible>;

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E> {
    _embed: PhantomData<E>,
    /// Fallback service for handling 404 Not Found errors.
    not_found_service: Option<NotFoundService>,
}

impl<E> Clone for ServeEmbed<E> {
    fn clone(&self) -> Self {
        Self {
            _embed: PhantomData,
            not_found_service: self.not_found_service.clone(),
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
        ServeEmbedBuilder::new().build::<E>()
    }
}

impl<E, ReqBody> tower::Service<http::Request<ReqBody>> for ServeEmbed<E>
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
        let req = req.map(|_| ());
        ResponseFuture::new::<E>(req, self.not_found_service.clone())
    }
}

/// Builder for [`ServeEmbed`] service.
#[derive(Default)]
pub struct ServeEmbedBuilder {
    not_found_service: Option<NotFoundService>,
}

impl ServeEmbedBuilder {
    /// Create a new [`ServeEmbedBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fallback service.
    pub fn not_found_service<S>(mut self, service: S) -> Self
    where
        S: tower::Service<
                http::Request<()>,
                Response = http::Response<ResponseBody>,
                Error = Infallible,
            > + Send
            + Sync
            + Clone
            + 'static,
        S::Future: Send + 'static,
    {
        self.not_found_service = Some(BoxCloneSyncService::new(service));
        self
    }

    /// Build the [`ServeEmbed`] service.
    pub fn build<E: Embed>(self) -> ServeEmbed<E> {
        ServeEmbed {
            _embed: PhantomData,
            not_found_service: self.not_found_service,
        }
    }
}
