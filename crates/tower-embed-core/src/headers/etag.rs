/// `ETag` header.
#[derive(Clone, Debug)]
pub struct ETag(http::HeaderValue);

impl ETag {
    /// Validates and creates an [`ETag`]` from a HeaderValue.
    fn from_header_value(value: &http::HeaderValue) -> Option<Self> {
        let bytes = value.as_bytes();
        let etag_value = if bytes.starts_with(b"W/") {
            &bytes[2..]
        } else {
            bytes
        };

        let is_quoted = etag_value.starts_with(b"\"") && etag_value.ends_with(b"\"");
        let all_ascii = etag_value.iter().all(|c| c.is_ascii());
        if is_quoted && all_ascii {
            Some(ETag(value.clone()))
        } else {
            None
        }
    }

    /// Creates an [`ETag`] from a string value.
    pub fn new(value: &str) -> Option<Self> {
        if !value.is_ascii() {
            return None;
        }

        let value = format!("\"{value}\"");

        Some(Self(http::HeaderValue::from_str(&value).unwrap()))
    }

    /// Creates a weak [`ETag`] from a string value.
    pub fn weak(value: &str) -> Option<Self> {
        if !value.is_ascii() {
            return None;
        }

        let value = format!("W/\"{value}\"");

        Some(Self(http::HeaderValue::from_str(&value).unwrap()))
    }

    /// Returns true if the ETag is weak.
    pub fn is_weak(&self) -> bool {
        self.0.as_bytes().starts_with(b"W/")
    }

    /// Returns the entity tag value.
    pub fn value(&self) -> &str {
        let bytes = self.0.as_bytes();
        let etag_value = if self.is_weak() { &bytes[2..] } else { bytes };

        let len = etag_value.len();
        let etag_value = &etag_value[1..len - 1]; // remove surrounding quotes

        std::str::from_utf8(etag_value).expect("ETag value is valid ASCII string")
    }

    /// Weak comparison of two ETags.
    pub(crate) fn weak_eq(&self, value: &[u8]) -> bool {
        self.value().as_bytes() == value
    }
}

impl super::Header for ETag {
    fn header_name() -> http::HeaderName {
        http::header::ETAG
    }

    fn decode(value: &http::HeaderValue) -> Option<Self> {
        Self::from_header_value(value)
    }

    fn encode(self) -> http::HeaderValue {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn etag_from_non_ascii_string() {
        assert!(ETag::new("välue").is_none());
    }

    #[test]
    fn invalid_header_value() {
        let header_value = http::HeaderValue::from_static("abc");
        assert!(ETag::from_header_value(&header_value).is_none());
    }

    #[test]
    fn weak_etag_detection() {
        let header_value = http::HeaderValue::from_static("\"xyzzy\"");
        let etag = ETag::from_header_value(&header_value).unwrap();
        assert!(!etag.is_weak());

        let header_value = http::HeaderValue::from_static("W/\"xyzzy\"");
        let etag = ETag::from_header_value(&header_value).unwrap();
        assert!(etag.is_weak());
    }
}
