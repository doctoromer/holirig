use anyhow::{Context, Result, anyhow};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{Duration, sleep};
use tracing::{debug, error, info, warn};

use crate::resources::Resources;
use crate::rig_settings::{RigId, RigSettings, Settings};
use crate::runtime::ExternalApi;
use crate::runtime::{Interpreter, Value};
use crate::serial::device::{DeviceCommand, DeviceMessage, SerialDevice};

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

pub struct DeviceManager {
    resources: Arc<Resources>,
    devices: HashMap<RigId, Device>,
    settings: Settings,
    data_dir: PathBuf,

    // manager -> ...
    manager_message_tx: broadcast::Sender<ManagerMessage>,

    // ... -> manager
    manager_command_tx: mpsc::Sender<ManagerCommand>,
    manager_command_rx: mpsc::Receiver<ManagerCommand>,

    // devices -> manager
    device_tx: mpsc::Sender<DeviceMessage>,
    device_rx: mpsc::Receiver<DeviceMessage>,

    prev_ports: Vec<SerialPortEntry>,
}

#[derive(Clone)]
struct Device {
    // Manager to devices channel
    command_tx: mpsc::Sender<DeviceCommand>,
    rig_wrapper: Interpreter,
    settings: RigSettings,
}

struct DeviceExternalApi {
    command_tx: mpsc::Sender<DeviceCommand>,
    status_values: Arc<Mutex<HashMap<String, Value>>>,
}

impl DeviceExternalApi {
    fn new(command_tx: mpsc::Sender<DeviceCommand>) -> Self {
        Self {
            command_tx,
            status_values: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn get_status_values(&self) -> HashMap<String, Value> {
        self.status_values.lock().unwrap().clone()
    }

    fn clear_status_values(&self) {
        self.status_values.lock().unwrap().clear();
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

impl DeviceManager {
    pub fn new(resources: Arc<Resources>) -> Self {
        let (manager_command_tx, manager_command_rx) = mpsc::channel(10);
        let (device_tx, device_rx) = mpsc::channel(10);

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
            device_tx,
            device_rx,
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

    fn handle_device_message(&mut self, device_message: DeviceMessage) {
        match device_message {
            DeviceMessage::Connected { device_id } => {
                let Some(device) = self.devices.get(&device_id).cloned() else {
                    warn!(%device_id, "Unknown device connected");
                    return;
                };
                let rig_model = device.settings.rig_type.clone();
                let poll_interval = device.settings.poll_interval;
                let manager_tx = self.manager_message_tx.clone();

                info!(%device_id, %rig_model, "Device connected, initializing");

                tokio::spawn(async move {
                    let external_api = DeviceExternalApi::new(device.command_tx.clone());
                    let init_result = device.rig_wrapper.execute_init(&external_api).await;

                    if let Err(ref err) = init_result {
                        error!(%device_id, %err, "Device initialization failed");
                    } else {
                        info!(%device_id, "Device initialized");
                    }

                    let _ = manager_tx.send(ManagerMessage::DeviceConnected {
                        device_id,
                        rig_model,
                    });

                    if init_result.is_err() {
                        return;
                    }

                    let mut previous_values = HashMap::new();
                    loop {
                        sleep(Duration::from_millis(poll_interval as u64)).await;

                        let values = match DeviceManager::execute_status_commands(&device).await {
                            Ok(v) => v,
                            Err(err) => {
                                error!(%device_id, %err, "Status polling failed");
                                break;
                            }
                        };

                        let changed_values: HashMap<String, Value> = values
                            .iter()
                            .filter(|(name, value)| {
                                previous_values
                                    .get(*name)
                                    .map(|prev_value| prev_value != *value)
                                    .unwrap_or(true)
                            })
                            .map(|(name, value)| (name.clone(), value.clone()))
                            .collect();

                        if !changed_values.is_empty() {
                            debug!(%device_id, ?changed_values, "Status update");
                            let _ = manager_tx.send(ManagerMessage::StatusUpdate {
                                device_id,
                                values: changed_values,
                            });
                        }
                        previous_values = values;
                    }
                });
            }
            DeviceMessage::Disconnected { device_id } => {
                info!(%device_id, "Device disconnected");
                let _ = self
                    .manager_message_tx
                    .send(ManagerMessage::DeviceDisconnected { device_id });
            }
            DeviceMessage::Error { device_id, error } => {
                error!(%device_id, %error, "Device failed");
                let _ = self
                    .manager_message_tx
                    .send(ManagerMessage::DeviceError { device_id, error });
            }
        }
    }

    async fn handle_manager_command(&mut self, manager_command: ManagerCommand) -> Result<()> {
        match manager_command {
            ManagerCommand::CreateOrUpdateDevice { settings } => {
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
                if let Some(device) = self.devices.get(&device_id).cloned() {
                    debug!(%device_id, %command_name, ?params, "Executing command");
                    tokio::spawn(async move {
                        let external_api = DeviceExternalApi::new(device.command_tx.clone());
                        let result = device
                            .rig_wrapper
                            .execute_command(&command_name, params, &external_api)
                            .await;

                        let response = match result {
                            Ok(values) => {
                                debug!(%device_id, %command_name, ?values, "Command succeeded");
                                CommandResponse::Success(values)
                            }
                            Err(err) => {
                                error!(%device_id, %command_name, %err, "Command failed");
                                CommandResponse::Error(err.to_string())
                            }
                        };
                        if let Some(tx) = response_channel {
                            let _ = tx.send(response);
                        }
                    });
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
                    .map(|(id, device)| (*id, device.settings.rig_type.clone()))
                    .collect();
                let _ = response_channel.send(devices);
            }
            ManagerCommand::RemoveDevice { device_id } => {
                if let Some(device) = self.devices.remove(&device_id) {
                    let _ = device.command_tx.send(DeviceCommand::Shutdown).await;
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
                Some(device_message) = self.device_rx.recv() => {
                    self.handle_device_message(device_message);
                },
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

    async fn execute_status_commands(device: &Device) -> Result<HashMap<String, Value>> {
        let external_api = DeviceExternalApi::new(device.command_tx.clone());
        external_api.clear_status_values();
        device.rig_wrapper.execute_status(&external_api).await?;
        Ok(external_api.get_status_values())
    }

    pub async fn add_device(&mut self, device_id: RigId, settings: RigSettings) -> Result<()> {
        let rig_wrapper = self
            .resources
            .rigs
            .get(&settings.rig_type)
            .context("Unknown rig type")?
            .clone();
        info!(%device_id, rig_type = %settings.rig_type, port = %settings.port, "Opening device");
        let (serial_device, command_rx) =
            SerialDevice::new(device_id, settings.clone(), self.device_tx.clone()).await?;

        let id = settings.id;

        let device = Device {
            command_tx: serial_device.command_sender(),
            rig_wrapper,
            settings,
        };

        self.devices.insert(device_id, device);

        let device_tx = self.device_tx.clone();
        tokio::spawn(async move {
            let device_id = id;

            device_tx
                .send(DeviceMessage::Connected { device_id })
                .await
                .unwrap();

            if let Err(err) = serial_device.run(command_rx).await {
                device_tx
                    .send(DeviceMessage::Error {
                        device_id,
                        error: err.to_string(),
                    })
                    .await
                    .unwrap();
            }
            device_tx
                .send(DeviceMessage::Disconnected { device_id })
                .await
                .unwrap();
        });

        Ok(())
    }
}
