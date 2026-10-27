//! Bare WebSocket transport with raw-byte injection.

use std::time::Duration;

use futures_util::{SinkExt, StreamExt};
use penwise_proto::{ClientMsg, ServerMsg, decode_server, encode_client};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};

/// One thing read from the socket.
#[derive(Debug, Clone, PartialEq)]
pub enum Recv {
    /// A decodable server frame.
    Msg(ServerMsg),
    /// A binary frame that does not decode as a server message.
    Undecodable(Vec<u8>),
    /// A text frame.
    Text(String),
    /// The peer closed (close code if one was sent) or the stream ended (`None`).
    Closed(Option<u16>),
    /// Nothing arrived within the timeout.
    Timeout,
}

/// A connected WebSocket.
#[derive(Debug)]
pub struct RawSocket {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl RawSocket {
    /// Connects to `url` (`ws://127.0.0.1:port/sync`).
    ///
    /// # Errors
    /// The transport error text.
    pub async fn connect(url: &str) -> Result<Self, String> {
        let (ws, _) = tokio_tungstenite::connect_async(url)
            .await
            .map_err(|e| e.to_string())?;
        Ok(Self { ws })
    }

    /// Sends one binary frame, bytes verbatim.
    ///
    /// # Errors
    /// The transport error text.
    pub async fn send_bytes(&mut self, bytes: Vec<u8>) -> Result<(), String> {
        self.ws
            .send(Message::Binary(bytes.into()))
            .await
            .map_err(|e| e.to_string())
    }

    /// Sends one text frame.
    ///
    /// # Errors
    /// The transport error text.
    pub async fn send_text(&mut self, text: &str) -> Result<(), String> {
        self.ws
            .send(Message::text(text.to_owned()))
            .await
            .map_err(|e| e.to_string())
    }

    /// Encodes and sends a client message.
    ///
    /// # Errors
    /// Encoding or transport error text.
    pub async fn send(&mut self, msg: &ClientMsg) -> Result<(), String> {
        let bytes = encode_client(msg).map_err(|e| e.to_string())?;
        self.send_bytes(bytes).await
    }

    /// Next application frame (pings/pongs are skipped).
    pub async fn recv(&mut self, timeout: Duration) -> Recv {
        loop {
            let Ok(next) = tokio::time::timeout(timeout, self.ws.next()).await else {
                return Recv::Timeout;
            };
            match next {
                None | Some(Err(_)) => return Recv::Closed(None),
                Some(Ok(Message::Binary(bytes))) => {
                    return match decode_server(&bytes) {
                        Ok(msg) => Recv::Msg(msg),
                        Err(_) => Recv::Undecodable(bytes.to_vec()),
                    };
                }
                Some(Ok(Message::Text(text))) => return Recv::Text(text.to_string()),
                Some(Ok(Message::Close(frame))) => {
                    return Recv::Closed(frame.map(|f| u16::from(f.code)));
                }
                Some(Ok(_)) => {}
            }
        }
    }

    /// Reads until a close or `timeout` of silence; returns the close code if any.
    pub async fn drain_until_closed(&mut self, timeout: Duration) -> Option<Option<u16>> {
        loop {
            match self.recv(timeout).await {
                Recv::Closed(code) => return Some(code),
                Recv::Timeout => return None,
                _ => {}
            }
        }
    }
}
