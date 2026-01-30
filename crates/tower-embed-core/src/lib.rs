//! Core functionalities of tower-embed.

use std::{
    error::Error,
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use futures_core::{Stream, stream::BoxStream};
use http_body::Frame;

pub mod headers;

/// A trait used to access to binary assets in a directory.
pub trait Embed {
    /// Get an embedded asset by its path.
    fn get(path: &str) -> impl Future<Output = std::io::Result<Embedded>> + Send + 'static;
}

/// An embedded binary asset.
pub struct Embedded {
    /// The content of the embedded asset.
    pub content: Content,
    /// The metadata associated with the embedded asset.
    pub metadata: Metadata,
}

/// Type-erased error type.
pub type BoxError = Box<dyn Error + Send + Sync>;

/// A stream of binary content.
pub struct Content(BoxStream<'static, Result<Bytes, BoxError>>);

impl Content {
    /// Creates a [`Content`] from a static slice of bytes.
    pub fn from_static(bytes: &'static [u8]) -> Self {
        Self(Box::pin(StaticContent::new(bytes)))
    }

    /// Creates a [`Content`] from a stream of frames.
    pub fn from_stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Bytes, E>> + Send + 'static,
        E: Into<BoxError>,
    {
        Self(Box::pin(StreamContent(stream)))
    }
}

impl Stream for Content {
    type Item = Result<Frame<Bytes>, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0.as_mut().poll_next(cx).map_ok(Frame::data)
    }
}

struct StaticContent(Option<&'static [u8]>);

impl StaticContent {
    pub fn new(bytes: &'static [u8]) -> Self {
        Self(Some(bytes))
    }
}

impl Stream for StaticContent {
    type Item = Result<Bytes, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.0
            .take()
            .map(|bytes| Ok(Bytes::from_static(bytes)))
            .into()
    }
}

struct StreamContent<S>(S);

impl<S, E> Stream for StreamContent<S>
where
    S: Stream<Item = Result<Bytes, E>>,
    E: Into<BoxError>,
{
    type Item = Result<Bytes, BoxError>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = unsafe { Pin::new_unchecked(&mut self.get_unchecked_mut().0) };
        match ready!(inner.poll_next(cx)) {
            Some(Ok(bytes)) => Some(Ok(bytes)),
            Some(Err(err)) => Some(Err(err.into())),
            None => None,
        }
        .into()
    }
}

/// Metadata associated with an embedded asset.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// MIME type of the resource.
    pub content_type: headers::ContentType,
    /// File unique identifier, to be used to match with `If-None-Match` header.
    pub etag: Option<headers::ETag>,
    /// The date and time when the resource was modified.
    pub last_modified: Option<headers::LastModified>,
}

/// Returns the last modification time of file.
pub fn last_modified(path: &std::path::Path) -> std::io::Result<headers::LastModified> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(headers::LastModified)
}

/// Returns the MIME type of file.
pub fn content_type(path: &std::path::Path) -> headers::ContentType {
    mime_guess::from_path(path)
        .first()
        .map(headers::ContentType)
        .unwrap_or_else(headers::ContentType::octet_stream)
}

/// Returns the unique identifier tag of the content.
pub fn etag(content: &[u8]) -> headers::ETag {
    use std::hash::Hasher;

    let hash: u64 = {
        let mut hasher = rapidhash::fast::RapidHasher::default_const();
        hasher.write(content);
        hasher.finish()
    };

    let etag = format!("{:016x}", hash);
    headers::ETag::new(&etag).unwrap()
}
