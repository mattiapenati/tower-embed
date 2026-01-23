use crate::headers::ETag;

/// `If-None-Match` header.
pub struct IfNoneMatch(Inner);

enum Inner {
    Any,
    Tags(http::HeaderValue),
}

impl IfNoneMatch {
    /// Validates and creates an [`IfNoneMatch`]` from a HeaderValue.
    fn from_header_value(value: &http::HeaderValue) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes == b"*" {
            return Some(Self::any());
        }

        let etags = bytes.split(|c| *c == b',').map(|etag| etag.trim_ascii());
        let is_valid = etags.clone().all(|etag| {
            let is_quoted = etag.starts_with(b"\"") && etag.ends_with(b"\"");
            let is_ascii = etag.iter().all(|c| c.is_ascii());
            is_quoted && is_ascii
        });
        if !is_valid {
            return None;
        }

        Some(Self(Inner::Tags(value.clone())))
    }

    /// Creates an `If-None-Match` header that matches any ETag.
    pub fn any() -> IfNoneMatch {
        IfNoneMatch(Inner::Any)
    }

    /// Check if the condition passes.
    pub fn condition_passes(&self, etag: &ETag) -> bool {
        match self.etags() {
            None => false,
            Some(mut etags) => etags.all(|x| !etag.weak_eq(x)),
        }
    }

    /// Iterate over the ETags in the `If-None-Match` header.
    fn etags(&self) -> Option<impl Iterator<Item = &'_ [u8]> + '_> {
        match &self.0 {
            Inner::Any => None,
            Inner::Tags(value) => {
                let bytes = value.as_bytes();
                let etags = bytes.split(|c| *c == b',').map(|etag| {
                    let etag = etag.trim_ascii();
                    let len = etag.len();
                    &etag[1..len - 1] // remove surrounding quotes
                });
                Some(etags)
            }
        }
    }
}

impl super::Header for IfNoneMatch {
    fn header_name() -> http::HeaderName {
        http::header::IF_NONE_MATCH
    }

    fn decode(value: &http::HeaderValue) -> Option<Self> {
        Self::from_header_value(value)
    }

    fn encode(self) -> http::HeaderValue {
        match self.0 {
            Inner::Any => http::HeaderValue::from_static("*"),
            Inner::Tags(value) => value,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_if_none_match() {
        let header_value = http::HeaderValue::from_static("*");
        assert!(IfNoneMatch::from_header_value(&header_value).is_some());

        let header_value = http::HeaderValue::from_static(r#""etag""#);
        assert!(IfNoneMatch::from_header_value(&header_value).is_some());

        let header_value = http::HeaderValue::from_static(r#""etag1","etag2""#);
        assert!(IfNoneMatch::from_header_value(&header_value).is_some());

        let header_value = http::HeaderValue::from_static(r#""etag1", "etag2""#);
        assert!(IfNoneMatch::from_header_value(&header_value).is_some());
    }

    #[test]
    fn condition_fails() {
        let etag = ETag::new("etag").unwrap();
        let weak_etag = ETag::weak("etag").unwrap();

        let if_none_match = {
            let header_value = http::HeaderValue::from_static(r#""etag""#);
            IfNoneMatch::from_header_value(&header_value).unwrap()
        };
        assert!(!if_none_match.condition_passes(&etag));
        assert!(!if_none_match.condition_passes(&weak_etag));

        let if_none_match = {
            let header_value = http::HeaderValue::from_static(r#""unmatched","etag""#);
            IfNoneMatch::from_header_value(&header_value).unwrap()
        };
        assert!(!if_none_match.condition_passes(&etag));
        assert!(!if_none_match.condition_passes(&weak_etag));

        let if_none_match = IfNoneMatch::any();
        assert!(!if_none_match.condition_passes(&etag));
    }

    #[test]
    fn condition_passes() {
        let etag = ETag::new("etag").unwrap();
        let weak_etag = ETag::weak("etag").unwrap();

        let if_none_match = {
            let header_value = http::HeaderValue::from_static(r#""unmatched""#);
            IfNoneMatch::from_header_value(&header_value).unwrap()
        };

        assert!(if_none_match.condition_passes(&etag));
        assert!(if_none_match.condition_passes(&weak_etag));
    }
}
