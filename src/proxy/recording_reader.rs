use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, ReadBuf};

/// An `AsyncRead` wrapper that records all bytes read through it.
pub struct RecordingReader<R> {
    inner: R,
    buffer: Vec<u8>,
}

impl<R> RecordingReader<R> {
    pub fn new(inner: R) -> Self {
        Self {
            inner,
            buffer: Vec::new(),
        }
    }

    /// Take and clear the recorded bytes.
    pub fn take_recorded(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.buffer)
    }
}

impl<R: AsyncRead + Unpin> AsyncRead for RecordingReader<R> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let before = buf.filled().len();
        let result = Pin::new(&mut self.inner).poll_read(cx, buf);
        if let Poll::Ready(Ok(())) = &result {
            let new_bytes = &buf.filled()[before..];
            self.buffer.extend_from_slice(new_bytes);
        }
        result
    }
}
