//! Low-level WebSocket plumbing.
//!
//! This module is internal, and may incompatibly change without warning.

#[cfg(not(feature = "web"))]
use std::mem;
use std::sync::Arc;
#[cfg(not(feature = "web"))]
use std::time::Duration;

use bytes::Bytes;
#[cfg(not(feature = "web"))]
use futures::TryStreamExt;
use futures::{SinkExt, StreamExt as _};
use futures_channel::mpsc;
use http::uri::{InvalidUri, Scheme, Uri};
use spacetimedb_client_api_messages::websocket::{BsatnFormat, Compression, BIN_PROTOCOL};
use spacetimedb_client_api_messages::websocket::{ClientMessage, ServerMessage};
use spacetimedb_lib::{bsatn, ConnectionId};
use thiserror::Error;

#[cfg(not(feature = "web"))]
use tokio::task::JoinHandle;
#[cfg(not(feature = "web"))]
use tokio::time::Instant;
#[cfg(not(feature = "web"))]
use tokio::{net::TcpStream, runtime};
#[cfg(not(feature = "web"))]
use tokio_tungstenite::{
    connect_async_with_config,
    tungstenite::client::IntoClientRequest,
    tungstenite::protocol::{Message as WebSocketMessage, WebSocketConfig},
    MaybeTlsStream, WebSocketStream,
};

#[cfg(feature = "web")]
use tokio_tungstenite_wasm::{Message as WebSocketMessage, WebSocketStream};

use crate::compression::decompress_server_message;
use crate::metrics::CLIENT_METRICS;

#[cfg(not(feature = "web"))]
type TungsteniteError = tokio_tungstenite::tungstenite::Error;
#[cfg(feature = "web")]
type TungsteniteError = tokio_tungstenite_wasm::Error;

#[derive(Error, Debug, Clone)]
pub enum UriError {
    #[error("Unknown URI scheme {scheme}, expected http, https, ws or wss")]
    UnknownUriScheme { scheme: String },

    #[error("Expected a URI without a query part, but found {query}")]
    UnexpectedQuery { query: String },

    #[error(transparent)]
    InvalidUri { source: Arc<http::uri::InvalidUri> },

    #[error(transparent)]
    InvalidUriParts {
        source: Arc<http::uri::InvalidUriParts>,
    },
}

#[derive(Error, Debug, Clone)]
pub enum WsError {
    #[error(transparent)]
    UriError(#[from] UriError),

    #[error("Error in WebSocket connection with {uri}: {source}")]
    Tungstenite {
        uri: Uri,
        #[source]
        source: Arc<TungsteniteError>,
    },

    #[error("Received empty raw message, but valid messages always start with a one-byte compression flag")]
    EmptyMessage,

    #[error("Failed to deserialize WebSocket message: {source}")]
    DeserializeMessage {
        #[source]
        source: bsatn::DecodeError,
    },

    #[error("Failed to decompress WebSocket message with {scheme}: {source}")]
    Decompress {
        scheme: &'static str,
        #[source]
        source: Arc<std::io::Error>,
    },

    #[error("Unrecognized compression scheme: {scheme:#x}")]
    UnknownCompressionScheme { scheme: u8 },

    #[cfg(feature = "web")]
    #[error("Token verification error: {0}")]
    #[allow(dead_code)]
    TokenVerification(String),
}

pub(crate) struct WsConnection {
    db_name: Box<str>,
    #[cfg(not(feature = "web"))]
    sock: WebSocketStream<MaybeTlsStream<TcpStream>>,
    #[cfg(feature = "web")]
    sock: WebSocketStream,
}

fn parse_scheme(scheme: Option<Scheme>) -> Result<Scheme, UriError> {
    Ok(match scheme {
        Some(s) => match s.as_str() {
            "ws" | "wss" => s,
            "http" => "ws".parse().unwrap(),
            "https" => "wss".parse().unwrap(),
            unknown_scheme => {
                return Err(UriError::UnknownUriScheme {
                    scheme: unknown_scheme.into(),
                })
            }
        },
        None => "ws".parse().unwrap(),
    })
}

#[derive(Clone, Copy, Default)]
pub(crate) struct WsParams {
    pub compression: Compression,
    pub light: bool,
    pub confirmed: Option<bool>,
}

fn make_uri(
    host: Uri,
    db_name: &str,
    connection_id: Option<ConnectionId>,
    params: WsParams,
    #[cfg(feature = "web")] token: Option<&str>,
) -> Result<Uri, UriError> {
    let mut parts = host.into_parts();
    let scheme = parse_scheme(parts.scheme.take())?;
    parts.scheme = Some(scheme);
    let mut path = if let Some(path_and_query) = parts.path_and_query {
        if let Some(query) = path_and_query.query() {
            return Err(UriError::UnexpectedQuery {
                query: query.into(),
            });
        }
        path_and_query.path().to_string()
    } else {
        "/".to_string()
    };

    if !path.ends_with('/') {
        path.push('/');
    }

    path.push_str("v1/database/");
    path.push_str(db_name);
    path.push_str("/subscribe");

    match params.compression {
        Compression::None => path.push_str("?compression=None"),
        Compression::Gzip => path.push_str("?compression=Gzip"),
        Compression::Brotli => path.push_str("?compression=Brotli"),
    };

    if let Some(cid) = connection_id {
        path.push_str("&connection_id=");
        path.push_str(&cid.to_hex());
    }

    if params.light {
        path.push_str("&light=true");
    }

    if let Some(confirmed) = params.confirmed {
        path.push_str("&confirmed=");
        path.push_str(if confirmed { "true" } else { "false" });
    }

    // On WASM, embed the token in the URL since we can't set headers on WebSocket
    #[cfg(feature = "web")]
    if let Some(token) = token {
        path.push_str("&token=");
        path.push_str(token);
    }

    parts.path_and_query =
        Some(
            path.parse()
                .map_err(|source: InvalidUri| UriError::InvalidUri {
                    source: Arc::new(source),
                })?,
        );
    Uri::from_parts(parts).map_err(|source| UriError::InvalidUriParts {
        source: Arc::new(source),
    })
}

#[cfg(not(feature = "web"))]
fn make_request(
    host: Uri,
    db_name: &str,
    token: Option<&str>,
    connection_id: Option<ConnectionId>,
    params: WsParams,
) -> Result<http::Request<()>, WsError> {
    let uri = make_uri(host, db_name, connection_id, params)?;
    let mut req = IntoClientRequest::into_client_request(uri.clone()).map_err(|source| {
        WsError::Tungstenite {
            uri,
            source: Arc::new(source),
        }
    })?;
    req.headers_mut().insert(
        http::header::SEC_WEBSOCKET_PROTOCOL,
        const { http::HeaderValue::from_static(BIN_PROTOCOL) },
    );
    if let Some(token) = token {
        let auth = ["Bearer ", token].concat().try_into().unwrap();
        req.headers_mut().insert(http::header::AUTHORIZATION, auth);
    }
    Ok(req)
}

/// If `res` evaluates to `Err(e)`, log a warning in the form `"{}: {:?}", $cause, e`.
macro_rules! maybe_log_error {
    ($cause:expr, $res:expr) => {
        if let Err(e) = $res {
            log::warn!("{}: {:?}", $cause, e);
        }
    };
}

impl WsConnection {
    #[cfg(not(feature = "web"))]
    pub(crate) async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: Option<ConnectionId>,
        params: WsParams,
    ) -> Result<Self, WsError> {
        let req = make_request(host, db_name, token, connection_id, params)?;
        let uri = req.uri().clone();

        let (sock, _): (WebSocketStream<MaybeTlsStream<TcpStream>>, _) = connect_async_with_config(
            req,
            Some(
                WebSocketConfig::default()
                    .max_frame_size(None)
                    .max_message_size(None),
            ),
            false,
        )
        .await
        .map_err(|source| WsError::Tungstenite {
            uri,
            source: Arc::new(source),
        })?;
        Ok(WsConnection {
            db_name: db_name.into(),
            sock,
        })
    }

    #[cfg(feature = "web")]
    pub(crate) async fn connect(
        host: Uri,
        db_name: &str,
        token: Option<&str>,
        connection_id: Option<ConnectionId>,
        params: WsParams,
    ) -> Result<Self, WsError> {
        let uri = make_uri(host, db_name, connection_id, params, token)?;
        let uri_string = uri.to_string();

        log::info!("WASM WsConnection::connect opening WebSocket to {uri_string}");
        let sock = tokio_tungstenite_wasm::connect_with_protocols(uri_string, &[BIN_PROTOCOL])
            .await
            .map_err(|source| {
                log::error!("WASM WsConnection::connect failed: {source:?}");
                WsError::Tungstenite {
                    uri,
                    source: Arc::new(source),
                }
            })?;
        log::info!("WASM WsConnection::connect WebSocket open");

        Ok(WsConnection {
            db_name: db_name.into(),
            sock,
        })
    }

    pub(crate) fn parse_response(bytes: &[u8]) -> Result<ServerMessage<BsatnFormat>, WsError> {
        let bytes = &*decompress_server_message(bytes)?;
        bsatn::from_slice(bytes).map_err(|source| WsError::DeserializeMessage { source })
    }

    pub(crate) fn encode_message(msg: ClientMessage<Bytes>) -> WebSocketMessage {
        WebSocketMessage::Binary(bsatn::to_vec(&msg).unwrap().into())
    }

    // === Native message loop ===

    #[cfg(not(feature = "web"))]
    async fn message_loop(
        mut self,
        incoming_messages: mpsc::UnboundedSender<ServerMessage<BsatnFormat>>,
        outgoing_messages: mpsc::UnboundedReceiver<ClientMessage<Bytes>>,
    ) {
        let websocket_received = CLIENT_METRICS
            .websocket_received
            .with_label_values(&self.db_name);
        let websocket_received_msg_size = CLIENT_METRICS
            .websocket_received_msg_size
            .with_label_values(&self.db_name);
        let record_metrics = |msg_size: usize| {
            websocket_received.inc();
            websocket_received_msg_size.observe(msg_size as f64);
        };

        const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
        let mut idle_timeout_interval =
            tokio::time::interval_at(Instant::now() + IDLE_TIMEOUT, IDLE_TIMEOUT);

        let mut idle = true;
        let mut want_pong = false;

        let mut outgoing_messages = Some(outgoing_messages);
        loop {
            tokio::select! {
                incoming = self.sock.try_next() => match incoming {
                    Err(tokio_tungstenite::tungstenite::error::Error::ConnectionClosed) | Ok(None) => {
                        log::info!("Connection closed");
                        break;
                    },

                    Err(e) => {
                        maybe_log_error!(
                            "Error reading message from read WebSocket stream",
                            Result::<(), _>::Err(e)
                        );
                        break;
                    },

                    Ok(Some(WebSocketMessage::Binary(bytes))) => {
                        idle = false;
                        record_metrics(bytes.len());
                        match Self::parse_response(&bytes) {
                            Err(e) => maybe_log_error!(
                                "Error decoding WebSocketMessage::Binary payload",
                                Result::<(), _>::Err(e)
                            ),
                            Ok(msg) => maybe_log_error!(
                                "Error sending decoded message to incoming_messages queue",
                                incoming_messages.unbounded_send(msg)
                            ),
                        }
                    }

                    Ok(Some(WebSocketMessage::Ping(payload))) => {
                        log::trace!("received ping");
                        idle = false;
                        record_metrics(payload.len());
                    },

                    Ok(Some(WebSocketMessage::Pong(payload))) => {
                        log::trace!("received pong");
                        idle = false;
                        want_pong = false;
                        record_metrics(payload.len());
                    },

                    Ok(Some(other)) => {
                        log::warn!("Unexpected WebSocket message {other:?}");
                        idle = false;
                        record_metrics(other.len());
                    },
                },

                _ = idle_timeout_interval.tick() => {
                    if mem::replace(&mut idle, true) {
                        if want_pong {
                            log::warn!("Connection timed out");
                            break;
                        }

                        log::trace!("sending client ping");
                        let ping = WebSocketMessage::Ping(Bytes::new());
                        if let Err(e) = self.sock.send(ping).await {
                            log::warn!("Error sending ping: {e:?}");
                            break;
                        }
                        want_pong = true;
                    }
                },

                Some(outgoing) = async { Some(outgoing_messages.as_mut()?.next().await) } => match outgoing {
                    Some(outgoing) => {
                        let msg = Self::encode_message(outgoing);
                        if let Err(e) = self.sock.send(msg).await {
                            log::warn!("Error sending outgoing message: {e:?}");
                            break;
                        }
                    }
                    None => {
                        maybe_log_error!("Error sending close frame", SinkExt::close(&mut self.sock).await);
                        outgoing_messages = None;
                    }
                },
            }
        }
    }

    // === WASM message loop (no tokio, no ping/pong, browser manages connection) ===

    #[cfg(feature = "web")]
    async fn message_loop(
        self,
        incoming_messages: mpsc::UnboundedSender<ServerMessage<BsatnFormat>>,
        outgoing_messages: mpsc::UnboundedReceiver<ClientMessage<Bytes>>,
    ) {
        let websocket_received = CLIENT_METRICS
            .websocket_received
            .with_label_values(&self.db_name);
        let websocket_received_msg_size = CLIENT_METRICS
            .websocket_received_msg_size
            .with_label_values(&self.db_name);
        let record_metrics = |msg_size: usize| {
            websocket_received.inc();
            websocket_received_msg_size.observe(msg_size as f64);
        };

        let (mut ws_writer, ws_reader) = self.sock.split();
        let mut ws_reader = ws_reader.fuse();
        let mut outgoing_messages = outgoing_messages.fuse();

        loop {
            futures::select! {
                incoming = ws_reader.next() => match incoming {
                    None => {
                        log::info!("Connection closed");
                        break;
                    }
                    Some(Err(e)) => {
                        log::warn!("Error reading WebSocket message: {:?}", e);
                        break;
                    }
                    Some(Ok(WebSocketMessage::Binary(bytes))) => {
                        record_metrics(bytes.len());
                        match Self::parse_response(&bytes) {
                            Err(e) => log::warn!("Error decoding message: {:?}", e),
                            Ok(msg) => maybe_log_error!(
                                "Error sending decoded message",
                                incoming_messages.unbounded_send(msg)
                            ),
                        }
                    }
                    Some(Ok(_)) => {},
                },

                outgoing = outgoing_messages.next() => match outgoing {
                    Some(outgoing) => {
                        let msg = Self::encode_message(outgoing);
                        if let Err(e) = ws_writer.send(msg).await {
                            log::warn!("Error sending outgoing message: {:?}", e);
                            break;
                        }
                    }
                    None => {
                        maybe_log_error!("Error closing", SinkExt::close(&mut ws_writer).await);
                        break;
                    }
                },
            }
        }
    }

    // === Native spawn ===

    #[cfg(not(feature = "web"))]
    pub(crate) fn spawn_message_loop(
        self,
        runtime: &runtime::Handle,
    ) -> (
        JoinHandle<()>,
        mpsc::UnboundedReceiver<ServerMessage<BsatnFormat>>,
        mpsc::UnboundedSender<ClientMessage<Bytes>>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded();
        let (incoming_send, incoming_recv) = mpsc::unbounded();
        let handle = runtime.spawn(self.message_loop(incoming_send, outgoing_recv));
        (handle, incoming_recv, outgoing_send)
    }

    // === WASM spawn (no JoinHandle, uses spawn_local) ===

    #[cfg(feature = "web")]
    pub(crate) fn spawn_message_loop(
        self,
    ) -> (
        mpsc::UnboundedReceiver<ServerMessage<BsatnFormat>>,
        mpsc::UnboundedSender<ClientMessage<Bytes>>,
    ) {
        let (outgoing_send, outgoing_recv) = mpsc::unbounded();
        let (incoming_send, incoming_recv) = mpsc::unbounded();
        wasm_bindgen_futures::spawn_local(self.message_loop(incoming_send, outgoing_recv));
        (incoming_recv, outgoing_send)
    }
}
