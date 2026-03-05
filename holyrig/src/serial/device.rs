use anyhow::{Context, Result};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;
use tokio::time::{Duration, sleep};
use tokio_serial::{SerialPortBuilderExt, SerialStream};

use crate::rig_settings::{DataBits, RigSettings, StopBits};

#[derive(Debug)]
pub enum DeviceCommand {
    Write {
        data: Vec<u8>,
    },
    ReadExact {
        length: usize,
        response_tx: mpsc::Sender<Result<Vec<u8>>>,
    },
    Shutdown,
}

#[derive(Debug)]
pub enum DeviceMessage {
    Error { device_id: usize, error: String },
    Disconnected { device_id: usize },
    Connected { device_id: usize },
}

pub struct SerialDevice {
    id: usize,
    port: SerialStream,
    settings: RigSettings,
    command_tx: mpsc::Sender<DeviceCommand>,
    device_tx: mpsc::Sender<DeviceMessage>,
}

impl SerialDevice {
    pub async fn new(
        id: usize,
        settings: RigSettings,
        device_tx: mpsc::Sender<DeviceMessage>,
    ) -> Result<(Self, mpsc::Receiver<DeviceCommand>)> {
        let port = Self::open_port(&settings)?;
        let (command_tx, command_rx) = mpsc::channel(32);

        Ok((
            Self {
                id,
                port,
                settings,
                command_tx,
                device_tx,
            },
            command_rx,
        ))
    }

    fn open_port(settings: &RigSettings) -> Result<SerialStream> {
        let data_bits = match settings.data_bits {
            DataBits::Bits8 => tokio_serial::DataBits::Eight,
            DataBits::Bits7 => tokio_serial::DataBits::Seven,
            DataBits::Bits6 => tokio_serial::DataBits::Six,
            DataBits::Bits5 => tokio_serial::DataBits::Five,
        };
        let stop_bits = match settings.stop_bits {
            StopBits::Bits1 => tokio_serial::StopBits::One,
            StopBits::Bits2 => tokio_serial::StopBits::Two,
        };
        let parity = if settings.parity {
            tokio_serial::Parity::Even
        } else {
            tokio_serial::Parity::None
        };

        tokio_serial::new(&settings.port, settings.baud_rate.into())
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .parity(parity)
            .flow_control(tokio_serial::FlowControl::None)
            .open_native_async()
            .with_context(|| format!("Failed to open serial port {}", settings.port))
    }

    pub fn command_sender(&self) -> mpsc::Sender<DeviceCommand> {
        self.command_tx.clone()
    }

    async fn attempt_reconnect(&mut self) -> Result<()> {
        println!("[device {}] Disconnected, attempting to reconnect on {}...", self.id, self.settings.port);
        loop {
            sleep(Duration::from_millis(self.settings.poll_interval as u64)).await;
            if let Ok(new_port) = Self::open_port(&self.settings) {
                self.port = new_port;
                println!("[device {}] Reconnected on {}", self.id, self.settings.port);
                self.device_tx
                    .send(DeviceMessage::Connected { device_id: self.id })
                    .await
                    .ok();
                return Ok(());
            }
        }
    }

    async fn write_only(&mut self, data: &[u8]) -> Result<()> {
        println!("[device {}] TX: {}", self.id, data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
        self.port.write_all(data).await?;
        Ok(())
    }

    async fn read_exact(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; length];
        self.port.read_exact(&mut buf).await?;
        println!("[device {}] RX: {}", self.id, buf.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "));
        Ok(buf)
    }

    pub async fn run(mut self, mut command_rx: mpsc::Receiver<DeviceCommand>) -> Result<()> {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                DeviceCommand::Write { data } => {
                    let result = self.write_only(&data).await;
                    if result.is_err() {
                        self.handle_error().await;
                    }
                }
                DeviceCommand::ReadExact {
                    length,
                    response_tx,
                } => {
                    let result = self.read_exact(length).await;
                    if result.is_err() {
                        self.handle_error().await;
                    }
                    response_tx.send(result).await.ok();
                }
                DeviceCommand::Shutdown => break,
            }
        }
        Ok(())
    }

    async fn handle_error(&mut self) {
        self.device_tx
            .send(DeviceMessage::Disconnected { device_id: self.id })
            .await
            .ok();

        if let Err(reconnect_err) = self.attempt_reconnect().await {
            self.device_tx
                .send(DeviceMessage::Error {
                    device_id: self.id,
                    error: reconnect_err.to_string(),
                })
                .await
                .ok();
        }
    }
}
