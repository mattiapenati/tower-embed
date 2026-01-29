use std::{
    borrow::Cow,
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use futures_core::{Stream, future::BoxFuture};
use http_body::{Body, Frame};
use http_body_util::BodyExt;
use tower_embed_core::{BoxError, Embed, Embedded, headers};

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
    WaitingEmbedded {
        fut: BoxFuture<'static, std::io::Result<Embedded>>,
        if_none_match: Option<headers::IfNoneMatch>,
        if_modified_since: Option<headers::IfModifiedSince>,
    },
}

impl ResponseFuture {
    pub(crate) fn new<E, B>(req: &http::Request<B>) -> Self
    where
        E: Embed,
    {
        if req.method() != http::Method::GET && req.method() != http::Method::HEAD {
            return Self::method_not_allowed();
        }

        let path = get_file_path_from_uri(req.uri());
        let embedded = E::get(path.as_ref());

        let if_none_match = req.headers().typed_get::<headers::IfNoneMatch>();
        let if_modified_since = req.headers().typed_get::<headers::IfModifiedSince>();

        let inner = ResponseFutureInner::WaitingEmbedded {
            fut: Box::pin(embedded),
            if_none_match,
            if_modified_since,
        };
        Self { inner }
    }

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
}

impl Future for ResponseFuture {
    type Output = Result<http::Response<ResponseBody>, std::convert::Infallible>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let inner = &mut self.get_mut().inner;

        let response = match inner {
            ResponseFutureInner::Ready(response) => response
                .take()
                .expect("ResponseFuture polled after completion"),
            ResponseFutureInner::WaitingEmbedded {
                fut,
                if_none_match,
                if_modified_since,
            } => match ready!(Pin::new(fut).poll(cx)) {
                Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                    *inner = ResponseFutureInner::Ready(None);
                    http::Response::builder()
                        .status(http::StatusCode::NOT_FOUND)
                        .body(ResponseBody::empty())
                        .unwrap()
                }
                Err(_) => {
                    *inner = ResponseFutureInner::Ready(None);
                    http::Response::builder()
                        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
                        .body(ResponseBody::empty())
                        .unwrap()
                }
                Ok(embedded) => {
                    // Make the request conditional if an If-None-Match header is present
                    if let Some(if_none_match) = if_none_match
                        && let Some(etag) = &embedded.metadata.etag
                        && !if_none_match.condition_passes(etag)
                    {
                        return Poll::Ready(Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_MODIFIED)
                            .body(ResponseBody::empty())
                            .unwrap()));
                    }

                    // Make the request conditional if an If-Modified-Since header is present
                    if let Some(if_modified_since) = if_modified_since
                        && let Some(last_modified) = embedded.metadata.last_modified
                        && !if_modified_since.condition_passes(&last_modified)
                    {
                        return Poll::Ready(Ok(http::Response::builder()
                            .status(http::StatusCode::NOT_MODIFIED)
                            .body(ResponseBody::empty())
                            .unwrap()));
                    }

                    let Embedded { content, metadata } = embedded;
                    let mut response = http::Response::builder()
                        .status(http::StatusCode::OK)
                        .body(ResponseBody::stream(content))
                        .unwrap();

                    response.headers_mut().typed_insert(metadata.content_type);
                    if let Some(etag) = metadata.etag {
                        response.headers_mut().typed_insert(etag);
                    }
                    if let Some(last_modified) = metadata.last_modified {
                        response.headers_mut().typed_insert(last_modified);
                    }

                    response
                }
            },
        };
        Poll::Ready(Ok(response))
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
