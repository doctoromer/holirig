use std::net::SocketAddr;
use std::time::Duration;

use anyhow::{Result, bail};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::net::tcp::{OwnedReadHalf, OwnedWriteHalf};
use tokio::sync::mpsc;
use tokio::time::timeout;
use tracing::trace;

use crate::protocol::{self, Request, Response, ServerMessage};

const TIMEOUT: Duration = Duration::from_secs(2);

pub struct TcpClient {
    sender: TcpSender,
    receiver: TcpReceiver,
}

impl TcpClient {
    pub async fn connect(server_addr: SocketAddr) -> Result<Self> {
        let stream = TcpStream::connect(server_addr).await?;
        let (reader, writer) = stream.into_split();
        Ok(Self {
            sender: TcpSender { writer },
            receiver: TcpReceiver { reader: BufReader::new(reader) },
        })
    }

    pub async fn send_and_wait(&mut self, request: &Request) -> Result<Response> {
        self.sender.send_request(request).await?;
        let mut line = String::new();
        loop {
            line.clear();
            let n = timeout(TIMEOUT, self.receiver.reader.read_line(&mut line))
                .await
                .map_err(|_| anyhow::anyhow!("Server timeout"))??;
            if n == 0 {
                bail!("Server disconnected");
            }
            trace!("Received: {line}");
            match protocol::parse_server_message(line.trim_end().as_bytes())? {
                ServerMessage::Response(resp) => return Ok(resp),
                ServerMessage::Notification(_) => continue,
            }
        }
    }

    pub fn into_split(self) -> (TcpSender, TcpReceiver) {
        (self.sender, self.receiver)
    }
}

pub struct TcpSender {
    writer: OwnedWriteHalf,
}

impl TcpSender {
    pub async fn send_request(&mut self, request: &Request) -> Result<()> {
        trace!("Sending: {request:?}");
        let mut data = serde_json::to_vec(request)?;
        data.push(b'\n');
        self.writer.write_all(&data).await?;
        Ok(())
    }
}

pub struct TcpReceiver {
    reader: BufReader<OwnedReadHalf>,
}

impl TcpReceiver {
    pub async fn run(mut self, tx: mpsc::Sender<ServerMessage>) -> Result<()> {
        let mut line = String::new();
        loop {
            line.clear();
            let n = self.reader.read_line(&mut line).await?;
            if n == 0 {
                break;
            }
            trace!("Received: {line}");
            if let Ok(msg) = protocol::parse_server_message(line.trim_end().as_bytes())
                && tx.send(msg).await.is_err()
            {
                break;
            }
        }
        Ok(())
    }
}
