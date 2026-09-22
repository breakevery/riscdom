//! The minimal bridge between a tokio stream and hyper's IO traits.
//!
//! hyper 1.x serves a connection over any type implementing `hyper::rt::Read` +
//! `hyper::rt::Write`; the usual adapter (`hyper-util`'s `TokioIo`) is **not**
//! among this crate's authorised dependencies, so the ~40 lines it amounts to
//! live here instead. Adding `hyper-util` would also have pulled hyper's `server`
//! feature anyway, so it buys nothing this crate does not already have.
//!
//! No `unsafe`: `ReadBufCursor::put_slice` panics unless the source fits in
//! `remaining()`, so the read is capped to that first.

use hyper::rt::{Read, ReadBufCursor, Write};
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};

/// Wraps a tokio IO type so hyper can drive it.
pub struct TokioIo<T> {
    inner: T,
}

impl<T> TokioIo<T> {
    pub fn new(inner: T) -> Self {
        Self { inner }
    }
}

/// One stack buffer per read call. hyper hands us the destination cursor; copying
/// through a fixed buffer keeps the copy safe and bounds each syscall.
const READ_CHUNK: usize = 16 * 1024;

impl<T: AsyncRead + Unpin> Read for TokioIo<T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let cap = buf.remaining().min(READ_CHUNK);
        if cap == 0 {
            return Poll::Ready(Ok(()));
        }
        let mut stack = [0u8; READ_CHUNK];
        let filled = {
            let mut read_buf = ReadBuf::new(&mut stack[..cap]);
            match Pin::new(&mut this.inner).poll_read(cx, &mut read_buf) {
                Poll::Ready(Ok(())) => read_buf.filled().len(),
                Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                Poll::Pending => return Poll::Pending,
            }
        };
        buf.put_slice(&stack[..filled]);
        Poll::Ready(Ok(()))
    }
}

impl<T: AsyncWrite + Unpin> Write for TokioIo<T> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.get_mut().inner).poll_write(cx, buf)
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_flush(cx)
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.get_mut().inner).poll_shutdown(cx)
    }
}
