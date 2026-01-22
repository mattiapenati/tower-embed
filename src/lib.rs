use std::{
    borrow::Cow,
    marker::PhantomData,
    str::FromStr,
    task::{Context, Poll},
};

use headers::HeaderMapExt;

#[doc(no_inline)]
pub use rust_embed;

#[doc(inline)]
pub use self::response::{ResponseBody, ResponseFuture};

use rust_embed::RustEmbed;
use tower_service::Service;

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
            && !if_none_match.precondition_passes(&etag)
        {
            return ResponseFuture::not_modified();
        }

        // Make the request conditional if an If-Modified-Since header is present
        if let Some(if_modified_since) = req.headers().typed_get::<headers::IfModifiedSince>()
            && let Some(last_modified) = last_modified
            && !if_modified_since.is_modified(last_modified.into())
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
        let bytes = self
            .sha256_hash()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        let etag = format!("\"{bytes}\"");
        headers::ETag::from_str(&etag).unwrap()
    }

    fn content_type_header(&self) -> headers::ContentType {
        headers::ContentType::from_str(self.mimetype())
            .unwrap_or_else(|_| headers::ContentType::octet_stream())
    }

    fn last_modified_header(&self) -> Option<headers::LastModified> {
        let unix_timestamp = self.last_modified()?;
        let system_time = std::time::SystemTime::UNIX_EPOCH
            .checked_add(std::time::Duration::from_secs(unix_timestamp))?;

        Some(headers::LastModified::from(system_time))
    }
}
