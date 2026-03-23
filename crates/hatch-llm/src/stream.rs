use std::pin::Pin;
use std::task::{Context, Poll};

use futures_util::Stream;
use hatch_core::Result;

/// Stream of completion text chunks from an LLM.
pub struct CompletionStream {
    inner: Pin<Box<dyn Stream<Item = Result<String>> + Send>>,
}

impl CompletionStream {
    /// Wraps a stream of textual chunks.
    pub fn new<S>(stream: S) -> Self
    where
        S: Stream<Item = Result<String>> + Send + 'static,
    {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Stream for CompletionStream {
    type Item = Result<String>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
