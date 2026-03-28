use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{Result, bail};
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::time::timeout;

use crate::protocol::{self, Request, Response, ServerMessage};

const TIMEOUT: Duration = Duration::from_secs(2);
const BUF_SIZE: usize = 2048;

pub struct UdpClient {
    socket: UdpSocket,
    server_addr: SocketAddr,
}

impl UdpClient {
    pub async fn connect(server_addr: SocketAddr) -> Result<Self> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        Ok(Self {
            socket,
            server_addr,
        })
    }

    pub async fn send_request(&self, request: &Request) -> Result<()> {
        let data = serde_json::to_vec(request)?;
        self.socket.send_to(&data, self.server_addr).await?;
        Ok(())
    }

    pub async fn send_and_wait(&self, request: &Request) -> Result<Response> {
        self.send_request(request).await?;
        let mut buf = vec![0u8; BUF_SIZE];
        let deadline = tokio::time::Instant::now() + TIMEOUT;
        loop {
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            if remaining.is_zero() {
                bail!("Server timeout");
            }
            let (len, _) = timeout(remaining, self.socket.recv_from(&mut buf))
                .await
                .map_err(|_| anyhow::anyhow!("Server timeout"))??;
            match protocol::parse_server_message(&buf[..len])? {
                ServerMessage::Response(resp) => return Ok(resp),
                ServerMessage::Notification(_) => continue,
            }
        }
    }

    pub fn into_split(self) -> (UdpSender, UdpReceiver) {
        let socket = Arc::new(self.socket);
        (
            UdpSender {
                socket: socket.clone(),
                server_addr: self.server_addr,
            },
            UdpReceiver { socket },
        )
    }
}

pub struct UdpSender {
    socket: Arc<UdpSocket>,
    server_addr: SocketAddr,
}

impl UdpSender {
    pub async fn send_request(&self, request: &Request) -> Result<()> {
        let data = serde_json::to_vec(request)?;
        self.socket.send_to(&data, self.server_addr).await?;
        Ok(())
    }
}

pub struct UdpReceiver {
    socket: Arc<UdpSocket>,
}

impl UdpReceiver {
    pub async fn run(self, tx: mpsc::Sender<ServerMessage>) -> Result<()> {
        let mut buf = vec![0u8; BUF_SIZE];
        loop {
            let (len, _) = self.socket.recv_from(&mut buf).await?;
            if let Ok(msg) = protocol::parse_server_message(&buf[..len])
                && tx.send(msg).await.is_err()
            {
                break;
            }
        }
        Ok(())
    }
}
