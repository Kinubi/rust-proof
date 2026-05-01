use std::io;

use futures::future::poll_fn;
use futures::io::{ AsyncRead, AsyncWrite };
use log::info;
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
}

pub struct YamuxSession<M> {
    pub muxer: M,
}

impl<S> YamuxMuxer<S> where S: AsyncRead + AsyncWrite + Unpin {
    pub async fn open_substream(&mut self) -> Result<Stream, RuntimeError> {
        info!(target: TAG, "opening outbound substream");
        let stream = poll_fn(|cx| self.connection.poll_new_outbound(cx)).await.map_err(
            map_connection_error
        )?;
        info!(target: TAG, "outbound substream opened");
        Ok(stream)
    }

    pub async fn accept_substream(&mut self) -> Result<Option<Stream>, RuntimeError> {
        info!(target: TAG, "waiting for inbound substream");
        let result = poll_fn(|cx| self.connection.poll_next_inbound(cx)).await
            .transpose()
            .map_err(map_connection_error)?;
        info!(target: TAG, "inbound substream: {:?}", result.is_some());
        Ok(result)
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
