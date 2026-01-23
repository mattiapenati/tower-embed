/// `Content-Type` header.
pub struct ContentType(mime::Mime);

impl ContentType {
    /// Create a `Content-Type` header for `application/octet-stream`.
    pub fn octet_stream() -> Self {
        ContentType(mime::APPLICATION_OCTET_STREAM)
    }

    pub fn from_str(mime: &str) -> Option<Self> {
        let mime_type = mime.parse().ok()?;
        Some(ContentType(mime_type))
    }
}

impl super::Header for ContentType {
    fn header_name() -> http::HeaderName {
        http::header::CONTENT_TYPE
    }

    fn decode(value: &http::HeaderValue) -> Option<Self> {
        let value_str = value.to_str().ok()?;
        let mime_type: mime::Mime = value_str.parse().ok()?;
        Some(ContentType(mime_type))
    }

    fn encode(self) -> http::HeaderValue {
        let value_string = self.0.to_string();
        http::HeaderValue::from_str(&value_string).unwrap()
    }
}
