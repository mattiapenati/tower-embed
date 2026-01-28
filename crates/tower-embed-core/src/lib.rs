//! Core functionalities of tower-embed.

use std::borrow::Cow;

pub mod headers;

/// A trait used to access to binary assets in a directory.
pub trait Embed {
    /// Get an embedded asset by its path.
    fn get(path: &str) -> std::io::Result<Embedded>;
}

/// An embedded binary asset.
#[derive(Clone)]
pub struct Embedded {
    /// The content of the embedded asset.
    pub content: Cow<'static, [u8]>,
    /// The metadata associated with the embedded asset.
    pub metadata: Metadata,
}

impl Embedded {
    pub fn read(path: &std::path::Path) -> std::io::Result<Self> {
        let content = std::fs::read(path)?;

        let content_type = content_type(path);
        let etag = etag(&content);
        let last_modified = Some(last_modified(path)?);

        Ok(Self {
            content: Cow::Owned(content),
            metadata: Metadata {
                content_type,
                etag,
                last_modified,
            },
        })
    }
}

/// Metadata associated with an embedded asset.
#[derive(Clone, Debug)]
pub struct Metadata {
    /// MIME type of the resource.
    pub content_type: headers::ContentType,
    /// File unique identifier, to be used to match with `If-None-Match` header.
    pub etag: headers::ETag,
    /// The date and time when the resource was modified.
    pub last_modified: Option<headers::LastModified>,
}

/// Returns the last modification time of file.
pub fn last_modified(path: &std::path::Path) -> std::io::Result<headers::LastModified> {
    std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .map(headers::LastModified)
}

/// Returns the MIME type of file.
pub fn content_type(path: &std::path::Path) -> headers::ContentType {
    mime_guess::from_path(path)
        .first()
        .map(headers::ContentType)
        .unwrap_or_else(headers::ContentType::octet_stream)
}

/// Returns the unique identifier tag of the content.
pub fn etag(content: &[u8]) -> headers::ETag {
    let hash: [u8; 32] = {
        use sha2::Digest;

        let mut hasher = sha2::Sha256::new();
        hasher.update(content);
        hasher.finalize().into()
    };

    let etag = hash.iter().map(|b| format!("{b:02x}")).collect::<String>();
    headers::ETag::new(&etag).unwrap()
}
