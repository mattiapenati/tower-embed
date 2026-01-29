use std::{
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_core::Stream;
use http_body::{Body, Frame};
use http_body_util::BodyExt;
use tower_embed_core::{BoxError, Embedded, Metadata};

use crate::core::headers::HeaderMapExt;

type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, BoxError>;

/// The body used in crate responses.
#[derive(Debug)]
pub struct ResponseBody(BoxBody);

impl ResponseBody {
    /// Create an empty response body.
    pub fn empty() -> Self {
        ResponseBody::new(http_body_util::Empty::new())
    }

    /// Create a new response body that contains a single chunk
    pub fn full(data: Bytes) -> Self {
        ResponseBody::new(http_body_util::Full::new(data))
    }

    /// Create a response body from a stream of bytes.
    pub fn stream<S, E>(stream: S) -> Self
    where
        S: Stream<Item = Result<Frame<Bytes>, E>> + Send + 'static,
        E: Into<BoxError>,
    {
        ResponseBody::new(http_body_util::StreamBody::new(stream))
    }

    fn new<B>(body: B) -> Self
    where
        B: Body<Data = Bytes> + Send + 'static,
        B::Error: Into<BoxError>,
    {
        ResponseBody(body.map_err(|err| err.into()).boxed_unsync())
    }
}

impl http_body::Body for ResponseBody {
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

/// Response future of [`ServeEmbed`]
///
/// [`ServeEmbed`]: crate::ServeEmbed
pub struct ResponseFuture {
    inner: ResponseFutureInner,
}

enum ResponseFutureInner {
    Ready(Option<http::Response<ResponseBody>>),
}

impl ResponseFuture {
    pub(crate) fn method_not_allowed() -> Self {
        let response = http::Response::builder()
            .header(
                http::header::ALLOW,
                http::HeaderValue::from_static("GET, HEAD"),
            )
            .status(http::StatusCode::METHOD_NOT_ALLOWED)
            .body(ResponseBody::empty())
            .unwrap();

        Self {
            inner: ResponseFutureInner::Ready(Some(response)),
        }
    }

    pub(crate) fn file_not_found() -> Self {
        let response = http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(ResponseBody::empty())
            .unwrap();

        Self {
            inner: ResponseFutureInner::Ready(Some(response)),
        }
    }

    pub(crate) fn not_modified() -> Self {
        let response = http::Response::builder()
            .status(http::StatusCode::NOT_MODIFIED)
            .body(ResponseBody::empty())
            .unwrap();

        Self {
            inner: ResponseFutureInner::Ready(Some(response)),
        }
    }

    pub(crate) fn internal_server_error() -> Self {
        let response = http::Response::builder()
            .status(http::StatusCode::INTERNAL_SERVER_ERROR)
            .body(ResponseBody::empty())
            .unwrap();

        Self {
            inner: ResponseFutureInner::Ready(Some(response)),
        }
    }

    pub(crate) fn file(embedded: Embedded) -> Self {
        let mut response = http::Response::builder()
            .status(http::StatusCode::OK)
            .body(ResponseBody::stream(embedded.content))
            .unwrap();

        let Metadata {
            content_type,
            etag,
            last_modified,
        } = embedded.metadata;

        response.headers_mut().typed_insert(content_type);
        response.headers_mut().typed_insert(etag);
        if let Some(last_modified) = last_modified {
            response.headers_mut().typed_insert(last_modified);
        }

        Self {
            inner: ResponseFutureInner::Ready(Some(response)),
        }
    }
}

impl Future for ResponseFuture {
    type Output = Result<http::Response<ResponseBody>, std::convert::Infallible>;

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let response = match self.get_mut().inner {
            ResponseFutureInner::Ready(ref mut response) => response
                .take()
                .expect("ResponseFuture polled after completion"),
        };
        Poll::Ready(Ok(response))
    }
}
