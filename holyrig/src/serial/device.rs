use anyhow::{Context, Result, anyhow, bail};
use serialport::SerialPort;
use std::collections::HashMap;
use std::sync::Mutex;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{Duration, interval, sleep};
use tokio_serial::{SerialPortBuilderExt, SerialStream};
use tracing::{debug, error, info, warn};

use crate::rig_settings::{DataBits, RigId, RigSettings, StopBits};
use crate::runtime::{ExternalApi, Interpreter, Value};
use crate::serial::manager::{CommandResponse, ManagerMessage, StatusCache};

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
    Disconnected,
    Reconnected,
    Error(String),
}

pub struct SerialDevice {
    id: RigId,
    port: Option<SerialStream>,
    settings: RigSettings,
    message_tx: mpsc::Sender<DeviceMessage>,
}

impl SerialDevice {
    pub fn new(
        id: RigId,
        settings: RigSettings,
        message_tx: mpsc::Sender<DeviceMessage>,
    ) -> Result<(
        Self,
        mpsc::Sender<DeviceCommand>,
        mpsc::Receiver<DeviceCommand>,
    )> {
        let port = Self::open_port(&settings)?;
        let (command_tx, command_rx) = mpsc::channel(32);

        Ok((
            Self {
                id,
                port: Some(port),
                settings,
                message_tx,
            },
            command_tx,
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

        let result = tokio_serial::new(&settings.port, settings.baud_rate.into())
            .data_bits(data_bits)
            .stop_bits(stop_bits)
            .parity(parity)
            .flow_control(tokio_serial::FlowControl::None)
            .open_native_async();

        if let Err(err) = &result {
            error!(
                "Failed to open device {}: {}",
                settings.port, err.description
            );
        }

        result.with_context(|| format!("Failed to open serial port {}", settings.port))
    }

    async fn attempt_reconnect(&mut self) -> Result<()> {
        warn!(device_id = %self.id, port = %self.settings.port, "Disconnected, attempting to reconnect");

        // Drop the old port immediately so the kernel releases the device node.
        let _ = self.port.take();

        loop {
            sleep(Duration::from_millis(self.settings.poll_interval as u64)).await;
            if let Ok(new_port) = Self::open_port(&self.settings) {
                if let Err(err) = new_port.clear(serialport::ClearBuffer::All) {
                    warn!(device_id = %self.id, %err, "Failed to clear serial buffers after reconnect");
                }
                self.port = Some(new_port);
                info!(device_id = %self.id, port = %self.settings.port, "Reconnected");
                self.message_tx.send(DeviceMessage::Reconnected).await.ok();
                return Ok(());
            }
        }
    }

    fn port(&mut self) -> Result<&mut SerialStream> {
        match &mut self.port {
            Some(port) => Ok(port),
            None => bail!("Serial port is not connected"),
        }
    }

    async fn write_only(&mut self, data: &[u8]) -> Result<()> {
        self.port()?.write_all(data).await?;
        Ok(())
    }

    async fn read_exact(&mut self, length: usize) -> Result<Vec<u8>> {
        let mut buf = vec![0u8; length];
        tokio::time::timeout(
            Duration::from_millis(self.settings.timeout as u64),
            self.port()?.read_exact(&mut buf),
        )
        .await??;
        Ok(buf)
    }

    pub async fn run(mut self, mut command_rx: mpsc::Receiver<DeviceCommand>) -> Result<()> {
        while let Some(cmd) = command_rx.recv().await {
            match cmd {
                DeviceCommand::Write { data } => {
                    if self.write_only(&data).await.is_err() {
                        self.handle_error(&mut command_rx).await;
                    }
                }
                DeviceCommand::ReadExact {
                    length,
                    response_tx,
                } => {
                    let result = self.read_exact(length).await;
                    let failed = result.is_err();
                    response_tx.send(result).await.ok();
                    if failed {
                        self.handle_error(&mut command_rx).await;
                    }
                }
                DeviceCommand::Shutdown => break,
            }
        }
        Ok(())
    }

    async fn handle_error(&mut self, command_rx: &mut mpsc::Receiver<DeviceCommand>) {
        self.message_tx.send(DeviceMessage::Disconnected).await.ok();

        while let Ok(cmd) = command_rx.try_recv() {
            if let DeviceCommand::ReadExact { response_tx, .. } = cmd {
                let _ = response_tx.try_send(Err(anyhow!("Device disconnected")));
            }
        }

        if let Err(reconnect_err) = self.attempt_reconnect().await {
            self.message_tx
                .send(DeviceMessage::Error(reconnect_err.to_string()))
                .await
                .ok();
        }
    }
}

#[derive(Debug)]
pub enum DeviceTaskCommand {
    ExecuteCommand {
        command_name: String,
        params: HashMap<String, String>,
        response_tx: Option<oneshot::Sender<CommandResponse>>,
    },
    Shutdown,
}

struct DeviceExternalApi {
    command_tx: mpsc::Sender<DeviceCommand>,
    status_values: Mutex<HashMap<String, Value>>,
}

impl DeviceExternalApi {
    fn new(command_tx: mpsc::Sender<DeviceCommand>) -> Self {
        Self {
            command_tx,
            status_values: Mutex::new(HashMap::new()),
        }
    }

    fn take_status_values(&self) -> HashMap<String, Value> {
        std::mem::take(&mut *self.status_values.lock().unwrap())
    }
}

impl ExternalApi for DeviceExternalApi {
    async fn write(&self, data: &[u8]) -> Result<()> {
        self.command_tx
            .send(DeviceCommand::Write {
                data: data.to_vec(),
            })
            .await
            .context("Failed to send write command to device")
    }

    async fn read(&self, length: usize) -> Result<Vec<u8>> {
        let (read_tx, mut read_rx) = mpsc::channel(1);

        self.command_tx
            .send(DeviceCommand::ReadExact {
                length,
                response_tx: read_tx,
            })
            .await
            .context("Failed to send read command to device")?;

        read_rx
            .recv()
            .await
            .ok_or_else(|| anyhow!("Device disconnected"))?
    }

    fn set_var(&self, var: &str, value: Value) -> Result<()> {
        self.status_values
            .lock()
            .unwrap()
            .insert(var.to_string(), value);
        Ok(())
    }
}

pub struct DeviceTask {
    id: RigId,
    settings: RigSettings,
    interpreter: Interpreter,
    serial_command_tx: mpsc::Sender<DeviceCommand>,
    previous_values: HashMap<String, Value>,
    manager_tx: broadcast::Sender<ManagerMessage>,
    status_cache: StatusCache,
    connected: bool,
}

impl DeviceTask {
    pub fn new(
        id: RigId,
        settings: RigSettings,
        interpreter: Interpreter,
        serial_command_tx: mpsc::Sender<DeviceCommand>,
        manager_tx: broadcast::Sender<ManagerMessage>,
        status_cache: StatusCache,
    ) -> Self {
        Self {
            id,
            settings,
            interpreter,
            serial_command_tx,
            previous_values: HashMap::new(),
            manager_tx,
            status_cache,
            connected: false,
        }
    }

    pub async fn run(
        mut self,
        mut task_command_rx: mpsc::Receiver<DeviceTaskCommand>,
        mut serial_message_rx: mpsc::Receiver<DeviceMessage>,
    ) {
        self.initialize_and_notify().await;

        let mut poll_interval = interval(Duration::from_millis(self.settings.poll_interval as u64));

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if self.connected {
                        self.poll_status().await;
                    }
                }
                cmd = task_command_rx.recv() => {
                    match cmd {
                        Some(DeviceTaskCommand::ExecuteCommand {
                            command_name, params, response_tx
                        }) => {
                            if self.connected {
                                self.handle_execute_command(command_name, params, response_tx).await;
                            } else if let Some(tx) = response_tx {
                                let _ = tx.send(CommandResponse::Error("Device disconnected".into()));
                            }
                        }
                        Some(DeviceTaskCommand::Shutdown) | None => {
                            let _ = self.serial_command_tx.send(DeviceCommand::Shutdown).await;
                            break;
                        }
                    }
                }
                msg = serial_message_rx.recv() => {
                    match msg {
                        Some(DeviceMessage::Disconnected) => {
                            self.connected = false;
                            info!(device_id = %self.id, "Device disconnected");
                            let _ = self.manager_tx.send(ManagerMessage::DeviceDisconnected {
                                device_id: self.id,
                            });
                        }
                        Some(DeviceMessage::Reconnected) => {
                            info!(device_id = %self.id, "Device reconnected, re-initializing");
                            self.initialize_and_notify().await;
                        }
                        Some(DeviceMessage::Error(err)) => {
                            error!(device_id = %self.id, %err, "Serial device error");
                            let _ = self.manager_tx.send(ManagerMessage::DeviceError {
                                device_id: self.id,
                                error: err,
                            });
                        }
                        None => {
                            warn!(device_id = %self.id, "Serial I/O task exited");
                            break;
                        }
                    }
                }
            }
        }
    }

    async fn initialize_and_notify(&mut self) {
        let api = DeviceExternalApi::new(self.serial_command_tx.clone());
        match self.interpreter.execute_init(&api).await {
            Ok(()) => {
                self.connected = true;
                info!(device_id = %self.id, "Device initialized");
                let _ = self.manager_tx.send(ManagerMessage::DeviceConnected {
                    device_id: self.id,
                    rig_model: self.settings.rig_type.clone(),
                });
            }
            Err(err) => {
                error!(device_id = %self.id, %err, "Device initialization failed");
            }
        }
    }

    async fn poll_status(&mut self) {
        let api = DeviceExternalApi::new(self.serial_command_tx.clone());

        if let Err(err) = self.interpreter.execute_status(&api).await {
            error!(device_id = %self.id, %err, "Status polling failed");
            return;
        }

        let values = api.take_status_values();
        let changed: HashMap<String, Value> = values
            .iter()
            .filter(|(k, v)| {
                self.previous_values
                    .get(*k)
                    .map(|prev| prev != *v)
                    .unwrap_or(true)
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        if !changed.is_empty() {
            debug!(device_id = %self.id, ?changed, "Status update");
            let json_changed: HashMap<String, serde_json::Value> = changed
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::from(v)))
                .collect();
            self.status_cache
                .write()
                .entry(self.id)
                .or_default()
                .extend(json_changed);
            let _ = self.manager_tx.send(ManagerMessage::StatusUpdate {
                device_id: self.id,
                values: changed,
            });
        }
        self.previous_values = values;
    }

    async fn handle_execute_command(
        &self,
        command_name: String,
        params: HashMap<String, String>,
        response_tx: Option<oneshot::Sender<CommandResponse>>,
    ) {
        let api = DeviceExternalApi::new(self.serial_command_tx.clone());
        let result = self
            .interpreter
            .execute_command(&command_name, params, &api)
            .await;
        let response = match result {
            Ok(values) => {
                debug!(device_id = %self.id, %command_name, ?values, "Command succeeded");
                CommandResponse::Success(values)
            }
            Err(err) => {
                error!(device_id = %self.id, %command_name, %err, "Command failed");
                CommandResponse::Error(err.to_string())
            }
        };
        if let Some(tx) = response_tx {
            let _ = tx.send(response);
        }
    }
}
