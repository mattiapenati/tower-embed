use std::{
    borrow::Cow,
    marker::PhantomData,
    task::{Context, Poll},
};

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

        ResponseFuture::file(file.data.clone())
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
