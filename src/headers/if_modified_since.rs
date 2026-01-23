use std::time::SystemTime;

use crate::headers::LastModified;

/// `Last-Modified` header.
pub struct IfModifiedSince(SystemTime);

impl IfModifiedSince {
    pub fn condition_passes(&self, last_modified: &LastModified) -> bool {
        last_modified.0 > self.0
    }
}

impl super::Header for IfModifiedSince {
    fn header_name() -> http::HeaderName {
        http::header::IF_MODIFIED_SINCE
    }

    fn decode(value: &http::HeaderValue) -> Option<Self> {
        let value_str = value.to_str().ok()?;
        let http_date = httpdate::parse_http_date(value_str).ok()?;
        Some(IfModifiedSince(http_date))
    }

    fn encode(self) -> http::HeaderValue {
        let value_string = httpdate::fmt_http_date(self.0);
        http::HeaderValue::from_str(&value_string).unwrap()
    }
}
