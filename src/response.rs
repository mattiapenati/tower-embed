use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use futures_core::{Stream, future::BoxFuture};
use http_body::{Body, Frame};
use http_body_util::BodyExt;
use tower::ServiceExt;
use tower_embed_core::{BoxError, Embed, Embedded, headers};

use crate::{NotFoundService, core::headers::HeaderMapExt};

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
    /// A response that is ready to be returned
    Ready(Option<http::Response<ResponseBody>>),
    /// Yield the not found service when it is ready to be called
    NotFoundReady(
        tower::util::ReadyOneshot<NotFoundService, http::Request<()>>,
        Option<http::Request<()>>,
    ),
    /// Consume the not found service by calling it
    NotFoundCall(tower::util::Oneshot<NotFoundService, http::Request<()>>),
    PollEmbedded(PollEmbedded),
}

struct PollEmbedded {
    future: BoxFuture<'static, std::io::Result<Embedded>>,
    request: Option<http::Request<()>>,
    not_found_service: Option<NotFoundService>,
}

impl ResponseFuture {
    pub(crate) fn new<E>(
        request: http::Request<()>,
        not_found_service: Option<NotFoundService>,
    ) -> Self
    where
        E: Embed,
    {
        if request.method() != http::Method::GET && request.method() != http::Method::HEAD {
            return Self::method_not_allowed();
        }

        let path = request.uri().path().trim_start_matches('/');

        let poll_embedded = PollEmbedded {
            future: Box::pin(E::get(path)),
            request: Some(request),
            not_found_service,
        };
        let inner = ResponseFutureInner::PollEmbedded(poll_embedded);
        Self { inner }
    }

    pub(crate) fn poll_embedded<F>(future: F, request: http::Request<()>) -> Self
    where
        F: Future<Output = std::io::Result<Embedded>> + Send + 'static,
    {
        let poll_embedded = PollEmbedded {
            future: Box::pin(future),
            request: Some(request),
            not_found_service: None,
        };
        let inner = ResponseFutureInner::PollEmbedded(poll_embedded);
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
        loop {
            break match inner {
                ResponseFutureInner::Ready(response) => Poll::Ready(Ok(response
                    .take()
                    .expect("ResponseFuture polled after completion"))),
                ResponseFutureInner::NotFoundReady(future, request) => {
                    let service = ready!(Pin::new(future).poll(cx)).unwrap();
                    let request = request.take().unwrap();
                    *inner = ResponseFutureInner::NotFoundCall(service.oneshot(request));
                    continue;
                }
                ResponseFutureInner::NotFoundCall(future) => {
                    let response = ready!(Pin::new(future).poll(cx));
                    *inner = ResponseFutureInner::Ready(None);
                    Poll::Ready(response)
                }
                ResponseFutureInner::PollEmbedded(waiting) => {
                    match ready!(Pin::new(&mut waiting.future).poll(cx)) {
                        Err(err)
                            if err.kind() == std::io::ErrorKind::NotFound
                                || err.kind() == std::io::ErrorKind::NotADirectory =>
                        {
                            match waiting.not_found_service.take() {
                                Some(not_found_service) => {
                                    *inner = ResponseFutureInner::NotFoundReady(
                                        not_found_service.ready_oneshot(),
                                        waiting.request.take(),
                                    );
                                    continue;
                                }
                                None => {
                                    *inner = ResponseFutureInner::Ready(None);
                                    Poll::Ready(Ok(default_not_found_response()))
                                }
                            }
                        }
                        Err(err) => {
                            let response = server_error_response(err);
                            *inner = ResponseFutureInner::Ready(None);
                            Poll::Ready(Ok(response))
                        }
                        Ok(Embedded { content, metadata }) => {
                            let request = waiting.request.as_ref().unwrap();

                            let if_none_match =
                                request.headers().typed_get::<headers::IfNoneMatch>();
                            if let Some(if_none_match) = if_none_match
                                && let Some(etag) = &metadata.etag
                                && !if_none_match.condition_passes(etag)
                            {
                                *inner = ResponseFutureInner::Ready(None);
                                return Poll::Ready(Ok(not_modified_response()));
                            }

                            let if_modified_since =
                                request.headers().typed_get::<headers::IfModifiedSince>();
                            if let Some(if_modified_since) = if_modified_since
                                && let Some(last_modified) = &metadata.last_modified
                                && !if_modified_since.condition_passes(last_modified)
                            {
                                *inner = ResponseFutureInner::Ready(None);
                                return Poll::Ready(Ok(not_modified_response()));
                            }

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

                            *inner = ResponseFutureInner::Ready(None);
                            Poll::Ready(Ok(response))
                        }
                    }
                }
            };
        }
    }
}

pub(crate) fn default_not_found_response() -> http::Response<ResponseBody> {
    http::Response::builder()
        .status(http::StatusCode::NOT_FOUND)
        .body(ResponseBody::empty())
        .unwrap()
}

pub(crate) fn server_error_response(_err: std::io::Error) -> http::Response<ResponseBody> {
    http::Response::builder()
        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
        .body(ResponseBody::empty())
        .unwrap()
}

fn not_modified_response() -> http::Response<ResponseBody> {
    http::Response::builder()
        .status(http::StatusCode::NOT_MODIFIED)
        .body(ResponseBody::empty())
        .unwrap()
}
