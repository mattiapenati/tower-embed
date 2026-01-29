use std::{
    pin::Pin,
    task::{Context, Poll, ready},
};

use bytes::Bytes;
use futures_core::Stream;
use http_body::Frame;
use tokio_util::io::ReaderStream;

use crate::core::BoxError;

/// An opened file handle.
pub struct File(ReaderStream<tokio::fs::File>);

impl File {
    /// Tries to open a file in read-only mode.
    pub async fn open(path: &std::path::Path) -> std::io::Result<Self> {
        let file = tokio::fs::File::open(path).await?;
        Ok(Self(ReaderStream::new(file)))
    }
}

impl Stream for File {
    type Item = Result<Frame<Bytes>, BoxError>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let inner = Pin::new(&mut self.0);
        match ready!(inner.poll_next(cx)) {
            Some(Ok(bytes)) => Some(Ok(Frame::data(bytes))),
            Some(Err(err)) => Some(Err(err.into())),
            None => None,
        }
        .into()
    }
}
