use anyhow::{Context, Result};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::Duration;
use tracing::{error, info};

use crate::resources::Resources;
use crate::rig_settings::{RigId, RigSettings, Settings};
use crate::runtime::Value;
use crate::serial::device::{DeviceTask, DeviceTaskCommand, SerialDevice};

#[derive(Debug, Clone, PartialEq)]
pub struct SerialPortEntry {
    pub port_name: String,
    pub display_name: String,
}

const RIGS_FILE: &str = "rigs.toml";

#[derive(Debug, Clone)]
pub enum CommandResponse {
    Success(HashMap<String, Value>),
    Error(String),
}

impl From<CommandResponse> for serde_json::Value {
    fn from(value: CommandResponse) -> Self {
        match value {
            CommandResponse::Success(values) => values
                .into_iter()
                .map(|(key, value)| (key, serde_json::Value::from(value)))
                .collect(),
            CommandResponse::Error(err) => {
                json!({"error": err})
            }
        }
    }
}

#[derive(Debug)]
pub enum ManagerCommand {
    CreateOrUpdateDevice {
        settings: RigSettings,
    },
    ExecuteCommand {
        device_id: RigId,
        command_name: String,
        params: HashMap<String, String>,
        response_channel: Option<oneshot::Sender<CommandResponse>>,
    },
    RemoveDevice {
        device_id: RigId,
    },
    ListDevices {
        response_channel: oneshot::Sender<HashMap<RigId, String>>,
    },
}

#[derive(Debug, Clone)]
pub enum ManagerMessage {
    DeviceConnected {
        device_id: RigId,
        rig_model: String,
    },
    DeviceDisconnected {
        device_id: RigId,
    },
    DeviceError {
        device_id: RigId,
        error: String,
    },
    StatusUpdate {
        device_id: RigId,
        values: HashMap<String, Value>,
    },
    AvailablePorts(Vec<SerialPortEntry>),
}

struct DeviceHandle {
    task_command_tx: mpsc::Sender<DeviceTaskCommand>,
    settings: RigSettings,
}

pub struct DeviceManager {
    resources: Arc<Resources>,
    devices: HashMap<RigId, DeviceHandle>,
    settings: Settings,
    data_dir: PathBuf,

    manager_message_tx: broadcast::Sender<ManagerMessage>,

    manager_command_tx: mpsc::Sender<ManagerCommand>,
    manager_command_rx: mpsc::Receiver<ManagerCommand>,

    prev_ports: Vec<SerialPortEntry>,
}

impl DeviceManager {
    pub fn new(resources: Arc<Resources>) -> Self {
        let (manager_command_tx, manager_command_rx) = mpsc::channel(10);
        let (manager_message_tx, _) = broadcast::channel(10);

        let data_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("holyrig");

        if !data_dir.exists() {
            std::fs::create_dir_all(&data_dir)
                .unwrap_or_else(|e| error!("Failed to create data directory: {e}"));
        }

        let settings_path = data_dir.join(RIGS_FILE);
        let settings = if !settings_path.exists() {
            Settings::default()
        } else {
            std::fs::read_to_string(&settings_path)
                .ok()
                .and_then(|content| toml::from_str(&content).ok())
                .unwrap_or_default()
        };

        Self {
            resources,
            devices: HashMap::new(),
            settings,
            data_dir,
            manager_message_tx,
            manager_command_tx,
            manager_command_rx,
            prev_ports: Vec::new(),
        }
    }

    pub fn initial_rigs(&self) -> impl Iterator<Item = &RigSettings> {
        self.settings.rigs()
    }

    pub fn receiver(&self) -> broadcast::Receiver<ManagerMessage> {
        self.manager_message_tx.subscribe()
    }

    pub fn sender(&self) -> mpsc::Sender<ManagerCommand> {
        self.manager_command_tx.clone()
    }

    async fn start_devices(&mut self) {
        let rigs: Vec<_> = self.settings.rigs().cloned().collect();
        for settings in rigs {
            if let Err(err) = self.add_device(settings.id, settings.clone()).await {
                error!(rig_id = %settings.id, %err, "Failed to load rig");
            }
        }
    }

    async fn handle_manager_command(&mut self, manager_command: ManagerCommand) -> Result<()> {
        match manager_command {
            ManagerCommand::CreateOrUpdateDevice { settings } => {
                if self.settings.get_rig(settings.id) == Some(&settings) {
                    return Ok(());
                }

                self.devices.remove(&settings.id);

                let device_id =
                    if let Some(changed_settings) = self.settings.get_rig_mut(settings.id) {
                        *changed_settings = settings.clone();
                        settings.id
                    } else {
                        self.settings.add_rig(settings.clone())
                    };
                let path = self.data_dir.join(RIGS_FILE);
                let content = toml::to_string(&self.settings)?;
                std::fs::write(path, content)?;

                let device_settings = self.settings.get_rig(device_id).unwrap().clone();
                if let Err(err) = self.add_device(device_id, device_settings).await {
                    error!(%err, "Failed to add device");
                }
            }
            ManagerCommand::ExecuteCommand {
                device_id,
                command_name,
                params,
                response_channel,
            } => {
                if let Some(handle) = self.devices.get(&device_id) {
                    let _ = handle
                        .task_command_tx
                        .send(DeviceTaskCommand::ExecuteCommand {
                            command_name,
                            params,
                            response_tx: response_channel,
                        })
                        .await;
                } else {
                    error!(%device_id, "Device not found");
                    if let Some(tx) = response_channel {
                        let _ = tx.send(CommandResponse::Error(format!(
                            "Device not found: {device_id}"
                        )));
                    }
                }
            }
            ManagerCommand::ListDevices { response_channel } => {
                let devices: HashMap<RigId, String> = self
                    .devices
                    .iter()
                    .map(|(id, handle)| (*id, handle.settings.rig_type.clone()))
                    .collect();
                let _ = response_channel.send(devices);
            }
            ManagerCommand::RemoveDevice { device_id } => {
                if let Some(handle) = self.devices.remove(&device_id) {
                    let _ = handle
                        .task_command_tx
                        .send(DeviceTaskCommand::Shutdown)
                        .await;
                }

                self.settings.remove_rig(device_id);
                let path = self.data_dir.join(RIGS_FILE);
                let content = toml::to_string(&self.settings)?;
                std::fs::write(path, content)?;
            }
        }
        Ok(())
    }

    pub async fn run(&mut self) -> Result<()> {
        self.start_devices().await;

        let mut port_interval = tokio::time::interval(Duration::from_millis(500));

        loop {
            tokio::select! {
                Some(manager_command) = self.manager_command_rx.recv() => {
                    self.handle_manager_command(manager_command).await?
                },
                _ = port_interval.tick() => {
                    self.poll_ports().await;
                },
            }
        }
    }

    async fn poll_ports(&mut self) {
        let ports = match tokio::task::spawn_blocking(serialport::available_ports).await {
            Ok(Ok(ports)) => ports,
            Ok(Err(err)) => {
                error!(%err, "Failed to enumerate serial ports");
                return;
            }
            Err(err) => {
                error!(%err, "Port enumeration task panicked");
                return;
            }
        };

        let ports: Vec<SerialPortEntry> = ports
            .into_iter()
            .filter_map(|p| match &p.port_type {
                serialport::SerialPortType::UsbPort(usb) => {
                    let display_name = if let Some(product) = &usb.product {
                        format!("{} ({})", p.port_name, product)
                    } else {
                        p.port_name.clone()
                    };
                    Some(SerialPortEntry {
                        port_name: p.port_name,
                        display_name,
                    })
                }
                _ => None,
            })
            .collect();

        if ports != self.prev_ports {
            let _ = self
                .manager_message_tx
                .send(ManagerMessage::AvailablePorts(ports.clone()));
            self.prev_ports = ports;
        }
    }

    pub async fn add_device(&mut self, device_id: RigId, settings: RigSettings) -> Result<()> {
        let interpreter = self
            .resources
            .rigs
            .get(&settings.rig_type)
            .context("Unknown rig type")?
            .clone();
        info!(%device_id, rig_type = %settings.rig_type, port = %settings.port, "Opening device");

        let (serial_message_tx, serial_message_rx) = mpsc::channel(8);
        let (serial_device, serial_command_tx, serial_command_rx) =
            SerialDevice::new(device_id, settings.clone(), serial_message_tx)?;

        let (task_command_tx, task_command_rx) = mpsc::channel(16);

        let device_task = DeviceTask::new(
            device_id,
            settings.clone(),
            interpreter,
            serial_command_tx,
            self.manager_message_tx.clone(),
        );

        tokio::spawn(async move {
            if let Err(err) = serial_device.run(serial_command_rx).await {
                error!(%err, "Serial device task failed");
            }
        });

        tokio::spawn(async move {
            device_task.run(task_command_rx, serial_message_rx).await;
        });

        self.devices.insert(
            device_id,
            DeviceHandle {
                task_command_tx,
                settings,
            },
        );

        Ok(())
    }
}
