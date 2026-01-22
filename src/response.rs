use std::{
    borrow::Cow,
    convert::Infallible,
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use headers::HeaderMapExt;
use http_body_util::BodyExt;

type BoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Infallible>;

/// The body used in crate responses.
#[derive(Debug)]
pub struct ResponseBody(BoxBody);

impl ResponseBody {
    /// Create an empty response body.
    pub(crate) fn empty() -> Self {
        ResponseBody(http_body_util::Empty::new().boxed_unsync())
    }

    /// Create a new response body that contains a single chunk
    pub(crate) fn full(data: Bytes) -> Self {
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

/// File information
pub(crate) struct File {
    pub content: Cow<'static, [u8]>,
    pub content_type: headers::ContentType,
    pub etag: headers::ETag,
    pub last_modified: Option<headers::LastModified>,
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

    pub(crate) fn file(file: File) -> Self {
        let mut response = http::Response::builder()
            .status(http::StatusCode::OK)
            .body(ResponseBody::full(match file.content {
                Cow::Borrowed(bytes) => Bytes::from_static(bytes),
                Cow::Owned(bytes) => Bytes::from_owner(bytes),
            }))
            .unwrap();

        response.headers_mut().typed_insert(file.content_type);
        response.headers_mut().typed_insert(file.etag);
        if let Some(last_modified) = file.last_modified {
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
