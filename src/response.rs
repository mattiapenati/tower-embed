use std::{
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use http_body_util::BodyExt;

type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Infallible>;

/// The body used in crate responses.
pub struct ResponseBody(BoxBody);

impl ResponseBody {
    /// Create an empty response body.
    pub fn empty() -> Self {
        ResponseBody(http_body_util::Empty::new().boxed_unsync())
    }

    /// Create a new response body that contains a single chunk
    pub fn full(data: Bytes) -> Self {
        ResponseBody(http_body_util::Full::new(data).boxed_unsync())
    }
}

impl http_body::Body for ResponseBody {
    type Data = Bytes;
    type Error = Infallible;

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
