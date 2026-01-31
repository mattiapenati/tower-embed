//! HTTP body types for requests and responses.

use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_core::Stream;
use http_body_util::BodyExt;

use crate::BoxError;

/// The body used in responses.
#[derive(Debug)]
pub struct Body(BodyInner);

type BodyInner = http_body_util::combinators::UnsyncBoxBody<Bytes, BoxError>;

impl Body {
    /// Create an empty response body.
    pub fn empty() -> Self {
        Body::new(http_body_util::Empty::new())
    }

    /// Create a new response body that contains a single chunk
    pub fn full(data: Bytes) -> Self {
        Body::new(http_body_util::Full::new(data))
    }

    /// Create a response body from a stream of bytes.
    pub fn stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<http_body::Frame<Bytes>, E>> + Send + 'static,
        E: Into<BoxError>,
    {
        Body::new(http_body_util::StreamBody::new(stream))
    }

    fn new<B>(body: B) -> Self
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
    {
        Body(body.map_err(|err| err.into()).boxed_unsync())
    }
}

impl http_body::Body for Body {
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<http_body::Frame<Self::Data>, Self::Error>>> {
        Pin::new(&mut self.0).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.0.is_end_stream()
    }

    fn size_hint(&self) -> http_body::SizeHint {
        self.0.size_hint()
    }
}
