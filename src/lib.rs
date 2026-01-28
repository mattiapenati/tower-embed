//! This crate provides a [`tower`] service designed to provide embedded static
//! assets support for web application. This service includes the following HTTP features:
//!
//! - Support for GET and HEAD requests
//! - Content-Type header generation based on file MIME types
//! - ETag header generation and validation
//! - Last-Modified header generation and validation
//!
//! # Usage
//!
//! Please see the [examples] directory for a working example.
//!
//! [`tower`]: https://crates.io/crates/tower
//! [examples]: https://github.com/mattiapenati/tower-embed/tree/main/examples

use std::{
    borrow::Cow,
    marker::PhantomData,
    task::{Context, Poll},
};

#[doc(hidden)]
pub use tower_embed_core;

#[doc(inline)]
pub use tower_embed_impl::Embed;

#[doc(inline)]
pub use tower_embed_core::{Embed, Embedded, Metadata, headers};

#[doc(inline)]
pub use self::response::{ResponseBody, ResponseFuture};

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
        use self::headers::HeaderMapExt;

        if req.method() != http::Method::GET && req.method() != http::Method::HEAD {
            return ResponseFuture::method_not_allowed();
        }

        let path = get_file_path_from_uri(req.uri());
        let embedded = match E::get(path.as_ref()) {
            Err(ref err) if err.kind() == std::io::ErrorKind::NotFound => {
                return ResponseFuture::file_not_found();
            }
            Err(_) => {
                return ResponseFuture::internal_server_error();
            }
            Ok(embedded) => embedded,
        };

        // Make the request conditional if an If-None-Match header is present
        if let Some(if_none_match) = req.headers().typed_get::<headers::IfNoneMatch>()
            && !if_none_match.condition_passes(&embedded.metadata.etag)
        {
            return ResponseFuture::not_modified();
        }

        // Make the request conditional if an If-Modified-Since header is present
        if let Some(if_modified_since) = req.headers().typed_get::<headers::IfModifiedSince>()
            && let Some(last_modified) = embedded.metadata.last_modified
            && !if_modified_since.condition_passes(&last_modified)
        {
            return ResponseFuture::not_modified();
        }

        ResponseFuture::file(embedded)
    }
}

fn get_file_path_from_uri(uri: &http::Uri) -> Cow<'_, str> {
    let path = uri.path();
    if path.ends_with("/") {
        Cow::Owned(format!("{}index.html", path.trim_start_matches('/')))
    } else {
        Cow::Borrowed(path.trim_start_matches('/'))
    }
}
