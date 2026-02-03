//! Tower services for serving embedded assets.

use std::{
    convert::Infallible,
    marker::PhantomData,
    sync::Arc,
    task::{Context, Poll},
};

use crate::{Body, ResponseFuture};

/// The trait used to access to embedded assets.
pub trait Embed {
    /// Forward an HTTP request to the embedded asset service.
    fn forward(
        req: http::Request<()>,
    ) -> impl Future<Output = http::Response<Body>> + Send + 'static;
}

/// Extension trait for [`Embed`].
pub trait EmbedExt: Embed + Sized {
    /// Returns a service that serves a custom not found page.
    fn not_found_page(path: &str) -> NotFoundPage<Self> {
        NotFoundPage::new(path)
    }
}

impl<T> EmbedExt for T where T: Embed + Sized {}

type NotFoundService = tower::util::BoxCloneSyncService<(), http::Response<Body>, Infallible>;

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E = ()> {
    _embed: PhantomData<E>,
    /// Fallback service for handling 404 Not Found errors.
    not_found_service: Option<NotFoundService>,
}

impl<E> Clone for ServeEmbed<E> {
    fn clone(&self) -> Self {
        Self {
            _embed: PhantomData,
            not_found_service: self.not_found_service.clone(),
        }
    }
}

impl<E: Embed> Default for ServeEmbed<E> {
    fn default() -> Self {
        Self::new()
    }
}

impl<E: Embed> ServeEmbed<E> {
    /// Create a new [`ServeEmbed`] service.
    pub fn new() -> Self {
        Self {
            _embed: PhantomData,
            not_found_service: None,
        }
    }

    /// Set the fallback service for not found pages.
    pub fn with_not_found<S>(mut self, service: S) -> Self
    where
        S: tower::Service<(), Response = http::Response<Body>, Error = Infallible>
            + Send
            + Sync
            + Clone
            + 'static,
        S::Future: Send + 'static,
    {
        self.not_found_service = Some(tower::util::BoxCloneSyncService::new(service));
        self
    }
}

impl<E, ReqBody> tower::Service<http::Request<ReqBody>> for ServeEmbed<E>
where
    E: Embed + Send + 'static,
{
    type Response = http::Response<Body>;
    type Error = std::convert::Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<ReqBody>) -> Self::Future {
        let req = req.map(|_| ());
        let mut not_found_service = self.not_found_service.clone();

        ResponseFuture::new(async move {
            use tower::ServiceExt;

            let response =
                if req.method() != http::Method::GET && req.method() != http::Method::HEAD {
                    crate::response::method_not_allowed()
                } else {
                    let mut response = E::forward(req).await;
                    if let Some(not_found_service) = not_found_service.take()
                        && response.status() == http::StatusCode::NOT_FOUND
                    {
                        let service = not_found_service.ready_oneshot().await.unwrap();
                        response = service.oneshot(()).await.unwrap()
                    }

                    response
                };
            Ok(response)
        })
    }
}

/// A service that serves a custom not found page.
pub struct NotFoundPage<E>(Arc<NotFoundPageInner<E>>);

impl<E> Clone for NotFoundPage<E> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

struct NotFoundPageInner<E> {
    _embed: PhantomData<E>,
    page: String,
}

impl<E> NotFoundPage<E> {
    pub(crate) fn new(page: &str) -> Self {
        let page = if page.starts_with('/') {
            page.to_string()
        } else {
            format!("/{}", page)
        };

        Self(Arc::new(NotFoundPageInner {
            _embed: PhantomData,
            page,
        }))
    }
}

impl<E> tower::Service<()> for NotFoundPage<E>
where
    E: Embed,
{
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _: ()) -> Self::Future {
        let req = http::Request::builder()
            .method(http::Method::GET)
            .uri(&self.0.page)
            .body(())
            .unwrap();
        ResponseFuture::new(async move {
            let mut response = E::forward(req).await;
            response.headers_mut().remove(http::header::ETAG);
            response.headers_mut().remove(http::header::LAST_MODIFIED);

            Ok(response)
        })
    }
}
