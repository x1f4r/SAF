use crate::{CoflCommandError, CoflEnvelope, CoflParseError, encode_cofl_command};
use async_trait::async_trait;
use futures_util::{SinkExt, StreamExt};
use saf_core::ids::AccountId;
use saf_core::ports::{CoflClient, PortError};
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async, tungstenite::protocol::Message,
};

fn install_rustls_crypto_provider() {
    static INSTALL: std::sync::Once = std::sync::Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

pub struct CoflWebSocket {
    stream: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl CoflWebSocket {
    pub async fn connect(link: &str) -> Result<Self, CoflWebSocketError> {
        install_rustls_crypto_provider();
        let (stream, _) = connect_async(link).await?;
        Ok(Self { stream })
    }

    pub async fn send_raw(&mut self, message: &str) -> Result<(), CoflWebSocketError> {
        self.stream
            .send(Message::Text(message.to_string().into()))
            .await?;
        Ok(())
    }

    pub async fn send_command(&mut self, command: &str) -> Result<(), CoflWebSocketError> {
        self.send_raw(&encode_cofl_command(command)?).await
    }

    pub async fn next_envelope(&mut self) -> Result<Option<CoflEnvelope>, CoflWebSocketError> {
        while let Some(message) = self.stream.next().await {
            match message? {
                Message::Text(text) => return Ok(Some(CoflEnvelope::from_wire(text.as_bytes())?)),
                Message::Binary(bytes) => return Ok(Some(CoflEnvelope::from_wire(bytes)?)),
                Message::Close(_) => return Ok(None),
                Message::Ping(payload) => self.stream.send(Message::Pong(payload)).await?,
                Message::Pong(_) | Message::Frame(_) => {}
            }
        }
        Ok(None)
    }
}

pub struct CoflWebSocketClient {
    account: AccountId,
    socket: Mutex<CoflWebSocket>,
}

impl CoflWebSocketClient {
    pub async fn connect(account: AccountId, link: &str) -> Result<Self, CoflWebSocketError> {
        Ok(Self {
            account,
            socket: Mutex::new(CoflWebSocket::connect(link).await?),
        })
    }

    pub fn account(&self) -> &AccountId {
        &self.account
    }

    pub async fn next_envelope(&self) -> Result<Option<CoflEnvelope>, CoflWebSocketError> {
        self.socket.lock().await.next_envelope().await
    }

    pub async fn send_raw(&self, message: &str) -> Result<(), CoflWebSocketError> {
        self.socket.lock().await.send_raw(message).await
    }
}

#[async_trait]
impl CoflClient for CoflWebSocketClient {
    async fn send_command(&self, account: &AccountId, command: &str) -> Result<(), PortError> {
        if account != &self.account {
            return Err(PortError::Unavailable(format!(
                "cofl websocket is connected for {}, not {account}",
                self.account
            )));
        }
        self.socket
            .lock()
            .await
            .send_command(command)
            .await
            .map_err(|error| PortError::Failed(error.to_string()))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum CoflWebSocketError {
    #[error("cofl websocket error: {0}")]
    WebSocket(#[from] tokio_tungstenite::tungstenite::Error),
    #[error("cofl parse error: {0}")]
    Parse(#[from] CoflParseError),
    #[error("cofl command error: {0}")]
    Command(#[from] CoflCommandError),
}
