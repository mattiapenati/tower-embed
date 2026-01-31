//! This crate provides a [`tower`] service designed to provide embedded static
//! assets support for web application. This service includes the following HTTP features:
//!
//! - Support for GET and HEAD requests
//! - `Content-Type` header generation based on file MIME type guessed from extension.
//! - `ETag` header generation and validation.
//! - `Last-Modified` header generation and validation.
//! - Customizable 404 page.
//!
//! In `debug` mode, assets are served directly from the filesystem to facilitate rapid
//! development. Both `ETag` and `Last-Modified` headers are not generated in this mode.
//!
//! # Usage
//!
//! ```no_run
//! use axum::Router;
//! use tower_embed::{Embed, EmbedExt, ServeEmbed};
//!
//! #[derive(Embed)]
//! #[embed(folder = "assets")]
//! struct Assets;
//!
//! #[tokio::main]
//! async fn main() {
//!     let assets = ServeEmbed::builder()
//!         .not_found_service(Assets::not_found_page("404.html"))
//!         .build::<Assets>();
//!     let router = Router::new().fallback_service(assets);
//!
//!     let listener = tokio::net::TcpListener::bind("127.0.0.1:8080")
//!         .await
//!         .unwrap();
//!     axum::serve::serve(listener, router).await.unwrap();
//! }
//! ```
//!
//! Please see the [examples] directory for working examples.
//!
//! [`tower`]: https://crates.io/crates/tower
//! [examples]: https://github.com/mattiapenati/tower-embed/tree/main/examples

#[cfg(not(feature = "tokio"))]
compile_error!("Only tokio runtime is supported, and it is required to use `tower-embed`.");

use std::{
    convert::Infallible,
    marker::PhantomData,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

#[doc(inline)]
pub use tower_embed_impl::Embed;

#[doc(inline)]
pub use tower_embed_core as core;

#[doc(inline)]
pub use tower_embed_core::{Embed, http::Body};

#[doc(hidden)]
pub mod file;

/// Response future of [`ServeEmbed`]
pub struct ResponseFuture(ResponseFutureInner);

type ResponseFutureInner =
    Pin<Box<dyn Future<Output = Result<http::Response<Body>, Infallible>> + Send>>;

impl ResponseFuture {
    fn new<F>(future: F) -> Self
    where
        F: Future<Output = Result<http::Response<Body>, Infallible>> + Send + 'static,
    {
        ResponseFuture(Box::pin(future))
    }
}

impl Future for ResponseFuture {
    type Output = Result<http::Response<Body>, Infallible>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.0.as_mut().poll(cx)
    }
}

/// Service that serves files from embedded assets.
pub struct ServeEmbed<E = ()> {
    _embed: PhantomData<E>,
    /// Fallback service for handling 404 Not Found errors.
    not_found_service: Option<NotFoundService>,
}

type NotFoundService =
    tower::util::BoxCloneSyncService<http::Request<()>, http::Response<Body>, Infallible>;

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
        ServeEmbedBuilder::new().build::<E>()
    }
}

impl ServeEmbed<()> {
    /// Create a new [`ServeEmbedBuilder`] to customize a new service instance.
    pub fn builder() -> ServeEmbedBuilder {
        ServeEmbedBuilder::new()
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
        let not_found_service = self.not_found_service.clone();
        ResponseFuture::new(async move {
            let response =
                if req.method() != http::Method::GET && req.method() != http::Method::HEAD {
                    method_not_allowed()
                } else {
                    let path = req.uri().path().trim_start_matches('/');
                    tracing::trace!("Serving embedded resource '{path}'");
                    handle_request(E::get(path), req, not_found_service).await
                };
            Ok(response)
        })
    }
}

/// Builder for [`ServeEmbed`] service.
#[derive(Default)]
pub struct ServeEmbedBuilder {
    not_found_service: Option<NotFoundService>,
}

impl ServeEmbedBuilder {
    /// Create a new [`ServeEmbedBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the fallback service.
    pub fn not_found_service<S>(mut self, service: S) -> Self
    where
        S: tower::Service<http::Request<()>, Response = http::Response<Body>, Error = Infallible>
            + Send
            + Sync
            + Clone
            + 'static,
        S::Future: Send + 'static,
    {
        self.not_found_service = Some(tower::util::BoxCloneSyncService::new(service));
        self
    }

    /// Build the [`ServeEmbed`] service.
    pub fn build<E: Embed>(self) -> ServeEmbed<E> {
        ServeEmbed {
            _embed: PhantomData,
            not_found_service: self.not_found_service,
        }
    }
}

/// Extension trait for [`Embed`].
pub trait EmbedExt: Embed + Sized {
    /// Returns a service that serves a custom not found page.
    fn not_found_page(path: &str) -> NotFoundPage<Self> {
        NotFoundPage::new(path.to_string())
    }
}

impl<T> EmbedExt for T where T: Embed + Sized {}

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
    fn new(page: String) -> Self {
        Self(Arc::new(NotFoundPageInner {
            _embed: PhantomData,
            page,
        }))
    }
}

impl<E> tower::Service<http::Request<()>> for NotFoundPage<E>
where
    E: Embed,
{
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = ResponseFuture;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<()>) -> Self::Future {
        let embedded = E::get(&self.0.page);
        ResponseFuture::new(async move { Ok(handle_request(embedded, req, None).await) })
    }
}

async fn handle_request<F>(
    embedded: F,
    request: http::Request<()>,
    not_found_service: Option<NotFoundService>,
) -> http::Response<Body>
where
    F: Future<Output = std::io::Result<core::Embedded>> + Send,
{
    use core::headers::{self, HeaderMapExt};

    let path = request.uri().path().trim_start_matches('/');
    let core::Embedded { content, metadata } = match embedded.await {
        Ok(embedded) => embedded,
        Err(err)
            if err.kind() == std::io::ErrorKind::NotFound
                || err.kind() == std::io::ErrorKind::NotADirectory =>
        {
            tracing::trace!("Embedded resource not found: '{path}'");
            return not_found_response(request, not_found_service).await;
        }
        Err(err) => {
            tracing::error!("Failed to get embedded resource '{path}': {err}");
            return server_error_response(err);
        }
    };

    let if_none_match = request.headers().typed_get::<headers::IfNoneMatch>();
    if let Some(if_none_match) = if_none_match
        && let Some(etag) = &metadata.etag
        && !if_none_match.condition_passes(etag)
    {
        tracing::trace!("ETag match for embedded resource '{path}'");
        return not_modified_response();
    }

    let if_modified_since = request.headers().typed_get::<headers::IfModifiedSince>();
    if let Some(if_modified_since) = if_modified_since
        && let Some(last_modified) = &metadata.last_modified
        && !if_modified_since.condition_passes(last_modified)
    {
        tracing::trace!("Last-Modified match for embedded resource '{path}'");
        return not_modified_response();
    }

    let mut response = http::Response::builder()
        .status(http::StatusCode::OK)
        .body(Body::stream(content))
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

async fn not_found_response(
    request: http::Request<()>,
    mut not_found_service: Option<NotFoundService>,
) -> http::Response<Body> {
    use tower::ServiceExt;

    let mut response = match not_found_service.take() {
        Some(service) => {
            let service = service.ready_oneshot().await.unwrap();
            service.oneshot(request).await.unwrap()
        }
        None => http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .body(Body::empty())
            .unwrap(),
    };
    response.headers_mut().insert(
        http::header::CACHE_CONTROL,
        http::HeaderValue::from_static("no-store"),
    );
    response
}

fn not_modified_response() -> http::Response<Body> {
    http::Response::builder()
        .status(http::StatusCode::NOT_MODIFIED)
        .body(Body::empty())
        .unwrap()
}

fn method_not_allowed() -> http::Response<Body> {
    http::Response::builder()
        .header(
            http::header::ALLOW,
            http::HeaderValue::from_static("GET, HEAD"),
        )
        .status(http::StatusCode::METHOD_NOT_ALLOWED)
        .body(Body::empty())
        .unwrap()
}

fn server_error_response(_err: std::io::Error) -> http::Response<Body> {
    http::Response::builder()
        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
        .header(http::header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap()
}
