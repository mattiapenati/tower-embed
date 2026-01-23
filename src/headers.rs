pub use self::{
    content_type::ContentType, etag::ETag, if_modified_since::IfModifiedSince,
    if_none_match::IfNoneMatch, last_modified::LastModified,
};

mod content_type;
mod etag;
mod if_modified_since;
mod if_none_match;
mod last_modified;

/// A trait for any type which represents an HTTP header.
pub trait Header: Sized {
    /// The name of the header.
    fn header_name() -> http::HeaderName;

    /// Decode the header from a `HeaderValue`.
    fn decode(value: &http::HeaderValue) -> Option<Self>;

    /// Encode the header into a `HeaderValue`.
    fn encode(self) -> http::HeaderValue;
}

/// An extension trait adding convenience methods to use typed headers.
pub trait HeaderMapExt {
    /// Tries to get a typed header from the header map.
    fn typed_get<H: Header>(&self) -> Option<H>;

    /// Inserts a typed header into the header map.
    fn typed_insert<H: Header>(&mut self, header: H);
}

impl HeaderMapExt for http::HeaderMap {
    fn typed_get<H: Header>(&self) -> Option<H> {
        self.get(H::header_name())
            .and_then(|value| H::decode(value))
    }

    fn typed_insert<H: Header>(&mut self, header: H) {
        self.insert(H::header_name(), header.encode());
    }
}
