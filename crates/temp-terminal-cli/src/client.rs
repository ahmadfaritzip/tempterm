//! Client IPC connection to temp-terminal daemon.
//! Corresponds to Go `tempterm/internal/client/client.go`.

use std::sync::atomic::{AtomicI64, Ordering};

use serde_json::Value;
use temp_terminal_proto::proto::{Hello, Message, Request, Welcome, PROTOCOL_VERSION};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

#[derive(Debug, thiserror::Error)]
pub enum ClientError {
    #[error("usage: {0}")]
    Usage(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("timeout: {0}")]
    Timeout(String),
    #[error("unreachable: {0}")]
    Unreachable(String),
    #[error("{0}")]
    Other(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
}

pub struct Client {
    reader: BufReader<tokio::net::tcp::OwnedReadHalf>,
    writer: tokio::net::tcp::OwnedWriteHalf,
    next_id: AtomicI64,
    #[allow(dead_code)]
    welcome: Welcome,
}

impl Client {
    pub async fn connect(addr: &str) -> Result<Self, ClientError> {
        let stream = match TcpStream::connect(addr).await {
            Ok(s) => s,
            Err(e) => return Err(ClientError::Unreachable(e.to_string())),
        };
        let (read_half, mut writer) = stream.into_split();
        let mut reader = BufReader::new(read_half);

        let hello = Hello {
            kind: "cli".to_string(),
            version: PROTOCOL_VERSION,
            client: "temp-terminal-cli".to_string(),
            pane: std::env::var("TEMPTERM_PANE")
                .or_else(|_| std::env::var("TEMP_TERMINAL_PANE"))
                .or_else(|_| std::env::var("OMATERM_PANE"))
                .unwrap_or_default(),
        };

        let mut hello_bytes = serde_json::to_vec(&hello)?;
        hello_bytes.push(b'\n');
        writer.write_all(&hello_bytes).await?;

        let mut line = String::new();
        if reader.read_line(&mut line).await? == 0 {
            return Err(ClientError::Unreachable(
                "daemon closed connection".to_string(),
            ));
        }

        let welcome: Welcome = serde_json::from_str(&line)?;
        if !welcome.error.is_empty() {
            return Err(ClientError::Other(welcome.error));
        }

        Ok(Self {
            reader,
            writer,
            next_id: AtomicI64::new(1),
            welcome,
        })
    }

    #[allow(dead_code)]
    pub fn welcome(&self) -> &Welcome {
        &self.welcome
    }

    pub async fn call(&mut self, method: &str, params: Value) -> Result<Value, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let req = Request {
            id,
            method: method.to_string(),
            params: if params.is_null() { None } else { Some(params) },
        };

        let mut req_bytes = serde_json::to_vec(&req)?;
        req_bytes.push(b'\n');
        self.writer.write_all(&req_bytes).await?;

        let mut line = String::new();
        if self.reader.read_line(&mut line).await? == 0 {
            return Err(ClientError::Unreachable(
                "daemon connection dropped".to_string(),
            ));
        }

        let msg: Message = serde_json::from_str(&line)?;
        if !msg.error.is_empty() {
            return match msg.code.as_str() {
                "usage" => Err(ClientError::Usage(msg.error)),
                "not_found" => Err(ClientError::NotFound(msg.error)),
                "timeout" => Err(ClientError::Timeout(msg.error)),
                _ => Err(ClientError::Other(msg.error)),
            };
        }

        Ok(msg.result.unwrap_or(Value::Null))
    }
}
