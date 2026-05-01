use std::{ collections::VecDeque, io, io::ErrorKind, pin::Pin, task::{ Context, Poll } };

use futures::future::poll_fn;
use futures::io::{ AsyncRead, AsyncWrite };
use log::debug;
use yamux::{ Connection, Mode, Stream };

use crate::{
    network::transport::multistream::{ dialer_select, listener_select },
    runtime::errors::RuntimeError,
};

const TAG: &str = "yamux";
pub const YAMUX_PROTOCOL: &str = "/yamux/1.0.0";
const DEFAULT_MAX_SUBSTREAMS: usize = 32;

pub struct YamuxMuxer<S> {
    connection: Connection<S>,
    pending_inbound: VecDeque<Stream>,
}

pub struct YamuxSession<M> {
    pub muxer: M,
}

pub struct YamuxStreamIo<'a, S> {
    muxer: &'a mut YamuxMuxer<S>,
    stream: &'a mut Stream,
}

impl<S> YamuxMuxer<S> where S: AsyncRead + AsyncWrite + Unpin {
    pub async fn open_substream(&mut self) -> Result<Stream, RuntimeError> {
        debug!(target: TAG, "opening outbound substream");
        let stream = poll_fn(|cx| self.connection.poll_new_outbound(cx)).await.map_err(
            map_connection_error
        )?;
        debug!(target: TAG, "outbound substream opened");
        Ok(stream)
    }

    pub async fn accept_substream(&mut self) -> Result<Option<Stream>, RuntimeError> {
        if let Some(stream) = self.pending_inbound.pop_front() {
            debug!(target: TAG, "returning queued inbound substream");
            return Ok(Some(stream));
        }

        debug!(target: TAG, "waiting for inbound substream");
        let result = poll_fn(|cx| {
            if let Some(stream) = self.pending_inbound.pop_front() {
                return Poll::Ready(Ok(Some(stream)));
            }

            match self.connection.poll_next_inbound(cx) {
                Poll::Ready(Some(Ok(stream))) => Poll::Ready(Ok(Some(stream))),
                Poll::Ready(Some(Err(error))) => Poll::Ready(Err(map_connection_error(error))),
                Poll::Ready(None) => Poll::Ready(Ok(None)),
                Poll::Pending => Poll::Pending,
            }
        }).await?;
        debug!(target: TAG, "inbound substream: {:?}", result.is_some());
        Ok(result)
    }

    pub fn io<'a>(&'a mut self, stream: &'a mut Stream) -> YamuxStreamIo<'a, S> {
        YamuxStreamIo { muxer: self, stream }
    }

    // `yamux::Stream` does not advance the underlying connection on its own, so reads and
    // writes on one substream must continue polling the shared connection and queue any
    // incidental inbound streams that arrive in the meantime.
    fn poll_drive_connection(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut queued_stream = false;

        loop {
            match self.connection.poll_next_inbound(cx) {
                Poll::Ready(Some(Ok(stream))) => {
                    debug!(target: TAG, "queued incidental inbound substream");
                    self.pending_inbound.push_back(stream);
                    queued_stream = true;
                }
                Poll::Ready(Some(Err(error))) => {
                    return Poll::Ready(Err(connection_error_to_io(error)));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(
                        Err(io::Error::new(ErrorKind::UnexpectedEof, "yamux connection closed"))
                    );
                }
                Poll::Pending => {
                    return if queued_stream { Poll::Ready(Ok(())) } else { Poll::Pending };
                }
            }
        }
    }

    pub async fn close(&mut self) -> Result<(), RuntimeError> {
        poll_fn(|cx| self.connection.poll_close(cx)).await.map_err(map_connection_error)
    }

    pub fn connection_mut(&mut self) -> &mut Connection<S> {
        &mut self.connection
    }
}

pub async fn upgrade_outbound<S>(mut stream: S) -> Result<YamuxSession<YamuxMuxer<S>>, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    dialer_select(&mut stream, YAMUX_PROTOCOL).await?;

    Ok(YamuxSession {
        muxer: YamuxMuxer {
            connection: Connection::new(stream, yamux_config(), Mode::Client),
            pending_inbound: VecDeque::new(),
        },
    })
}

pub async fn upgrade_inbound<S>(mut stream: S) -> Result<YamuxSession<YamuxMuxer<S>>, RuntimeError>
    where S: AsyncRead + AsyncWrite + Unpin
{
    listener_select(&mut stream, &[YAMUX_PROTOCOL]).await?;

    Ok(YamuxSession {
        muxer: YamuxMuxer {
            connection: Connection::new(stream, yamux_config(), Mode::Server),
            pending_inbound: VecDeque::new(),
        },
    })
}

fn yamux_config() -> yamux::Config {
    let mut config = yamux::Config::default();
    config.set_max_num_streams(DEFAULT_MAX_SUBSTREAMS);
    config.set_max_connection_receive_window(
        Some(DEFAULT_MAX_SUBSTREAMS * (yamux::DEFAULT_CREDIT as usize))
    );
    config
}

fn map_connection_error(error: yamux::ConnectionError) -> RuntimeError {
    RuntimeError::NetworkError(io::Error::other(format!("yamux connection error: {error}")))
}

fn connection_error_to_io(error: yamux::ConnectionError) -> io::Error {
    io::Error::other(format!("yamux connection error: {error}"))
}

impl<S> AsyncRead for YamuxStreamIo<'_, S> where S: AsyncRead + AsyncWrite + Unpin {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut [u8]
    ) -> Poll<io::Result<usize>> {
        loop {
            match Pin::new(&mut *self.stream).poll_read(cx, buf) {
                Poll::Ready(result) => {
                    return Poll::Ready(result);
                }
                Poll::Pending => {}
            }

            match self.muxer.poll_drive_connection(cx) {
                Poll::Ready(Ok(())) => {
                    continue;
                }
                Poll::Ready(Err(error)) => {
                    return Poll::Ready(Err(error));
                }
                Poll::Pending => {
                    return Poll::Pending;
                }
            }
        }
    }
}

impl<S> AsyncWrite for YamuxStreamIo<'_, S> where S: AsyncRead + AsyncWrite + Unpin {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8]
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut *self.stream).poll_write(cx, buf) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending =>
                match self.muxer.poll_drive_connection(cx) {
                    Poll::Ready(Ok(())) | Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                }
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut *self.stream).poll_flush(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending =>
                match self.muxer.poll_drive_connection(cx) {
                    Poll::Ready(Ok(())) | Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                }
        }
    }

    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut *self.stream).poll_close(cx) {
            Poll::Ready(result) => Poll::Ready(result),
            Poll::Pending =>
                match self.muxer.poll_drive_connection(cx) {
                    Poll::Ready(Ok(())) | Poll::Pending => Poll::Pending,
                    Poll::Ready(Err(error)) => Poll::Ready(Err(error)),
                }
        }
    }
}
