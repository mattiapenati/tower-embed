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

#[doc(no_inline)]
pub use rust_embed;

use rust_embed::RustEmbed;
use tower_service::Service;

use self::headers::HeaderMapExt;

#[doc(inline)]
pub use self::response::{ResponseBody, ResponseFuture};

mod headers;
mod response;

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E: RustEmbed> {
    _embed: PhantomData<E>,
}

impl<E: RustEmbed> Clone for ServeEmbed<E> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<E: RustEmbed> Copy for ServeEmbed<E> {}

impl<E: RustEmbed> ServeEmbed<E> {
    /// Create a new [`ServeEmbed`] service.
    pub fn new() -> Self {
        Self {
            _embed: PhantomData,
        }
    }
}

impl<E: RustEmbed> Default for ServeEmbed<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E, ReqBody> Service<http::Request<ReqBody>> for ServeEmbed<E>
where
    E: RustEmbed,
{
    type Response = http::Response<ResponseBody>;
    type Error = std::convert::Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        if req.method() != http::Method::GET && req.method() != http::Method::HEAD {
            return ResponseFuture::method_not_allowed();
        }

        let path = get_file_path_from_uri(req.uri());
        let Some(file) = E::get(path.as_ref()) else {
            return ResponseFuture::file_not_found();
        };

        // Get response headers
        let content_type = file.metadata.content_type_header();
        let etag = file.metadata.etag_header();
        let last_modified = file.metadata.last_modified_header();

        // Make the request conditional if an If-None-Match header is present
        if let Some(if_none_match) = req.headers().typed_get::<headers::IfNoneMatch>()
            && !if_none_match.condition_passes(&etag)
        {
            return ResponseFuture::not_modified();
        }

        // Make the request conditional if an If-Modified-Since header is present
        if let Some(if_modified_since) = req.headers().typed_get::<headers::IfModifiedSince>()
            && let Some(last_modified) = last_modified
            && !if_modified_since.condition_passes(&last_modified)
        {
            return ResponseFuture::not_modified();
        }

        ResponseFuture::file(response::File {
            content: file.data.clone(),
            content_type,
            etag,
            last_modified,
        })
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

trait MetadataExt {
    /// Compute the ETag for the asset.
    fn etag_header(&self) -> headers::ETag;

    /// Returns the content type of the asset.
    fn content_type_header(&self) -> headers::ContentType;

    /// Return the last modified time formatted as an HTTP date.
    fn last_modified_header(&self) -> Option<headers::LastModified>;
}

impl MetadataExt for rust_embed::Metadata {
    fn etag_header(&self) -> headers::ETag {
        let etag = self
            .sha256_hash()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        headers::ETag::new(&etag).unwrap()
    }

    fn content_type_header(&self) -> headers::ContentType {
        headers::ContentType::from_str(self.mimetype())
            .unwrap_or_else(headers::ContentType::octet_stream)
    }

    fn last_modified_header(&self) -> Option<headers::LastModified> {
        let unix_timestamp = self.last_modified()?;
        headers::LastModified::from_unix_timestamp(unix_timestamp)
    }
}
