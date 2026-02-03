//! Standard HTTP responses for common status codes.

use crate::Body;

/// The standard HTTP response for Not Found (404).
pub fn not_found() -> http::Response<Body> {
    http::Response::builder()
        .status(http::StatusCode::NOT_FOUND)
        .body(Body::empty())
        .unwrap()
}

/// The standard HTTP response for Not Modified (304).
pub fn not_modified() -> http::Response<Body> {
    http::Response::builder()
        .status(http::StatusCode::NOT_MODIFIED)
        .body(Body::empty())
        .unwrap()
}

/// The standard HTTP response for Method Not Allowed (405).
pub fn method_not_allowed() -> http::Response<Body> {
    http::Response::builder()
        .header(
            http::header::ALLOW,
            http::HeaderValue::from_static("GET, HEAD"),
        )
        .status(http::StatusCode::METHOD_NOT_ALLOWED)
        .body(Body::empty())
        .unwrap()
}

/// The standard HTTP response for Internal Server Error (500).
pub fn internal_server_error() -> http::Response<Body> {
    http::Response::builder()
        .status(http::StatusCode::INTERNAL_SERVER_ERROR)
        .body(Body::empty())
        .unwrap()
}
