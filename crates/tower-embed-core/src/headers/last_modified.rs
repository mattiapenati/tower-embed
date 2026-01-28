use std::time::{Duration, SystemTime};

/// `Last-Modified` header.
#[derive(Clone, Copy, Debug)]
pub struct LastModified(pub SystemTime);

impl LastModified {
    /// Creates a new [`LastModified`] from a UNIX timestamp.
    pub fn from_unix_timestamp(seconds: u64) -> Option<Self> {
        SystemTime::UNIX_EPOCH
            .checked_add(Duration::from_secs(seconds))
            .map(Self)
    }
}

impl super::Header for LastModified {
    fn header_name() -> http::HeaderName {
        http::header::LAST_MODIFIED
    }

    fn decode(value: &http::HeaderValue) -> Option<Self> {
        let value_str = value.to_str().ok()?;
        let http_date = httpdate::parse_http_date(value_str).ok()?;
        Some(LastModified(http_date))
    }

    fn encode(self) -> http::HeaderValue {
        let value_string = httpdate::fmt_http_date(self.0);
        http::HeaderValue::from_str(&value_string).unwrap()
    }
}
