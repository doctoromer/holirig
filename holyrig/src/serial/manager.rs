use anyhow::{Context, Result, anyhow};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio::time::{Duration, sleep};

use crate::gui::GuiMessage;
use crate::resources::Resources;
use crate::rig_settings::{RigSettings, Settings};
use crate::runtime::ExternalApi;
use crate::runtime::{Interpreter, Value};
use crate::serial::device::{DeviceCommand, DeviceMessage, SerialDevice};

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
        device_id: usize,
        command_name: String,
        params: HashMap<String, String>,
        response_channel: Option<oneshot::Sender<CommandResponse>>,
    },
    RemoveDevice {
        device_id: usize,
    },
    ListDevices {
        response_channel: oneshot::Sender<HashMap<usize, String>>,
    },
}

#[derive(Debug, Clone)]
pub enum ManagerMessage {
    InitialState {
        // DeviceId, RigFile name
        rigs: HashMap<usize, String>,
    },
    DeviceConnected {
        device_id: usize,
        rig_model: String,
    },
    DeviceDisconnected {
        device_id: usize,
    },
    StatusUpdate {
        device_id: usize,
        values: HashMap<String, Value>,
    },
}

pub struct DeviceManager {
    resources: Arc<Resources>,
    devices: HashMap<usize, Device>,
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
                .unwrap_or_else(|e| eprintln!("Failed to create data directory: {}", e));
        }

        Self {
            resources,
            devices: HashMap::new(),
            settings: Default::default(),
            data_dir,
            manager_message_tx,
            manager_command_tx,
            manager_command_rx,
            device_tx,
            device_rx,
        }
    }

    pub fn receiver(&self) -> broadcast::Receiver<ManagerMessage> {
        self.manager_message_tx.subscribe()
    }

    pub fn sender(&self) -> mpsc::Sender<ManagerCommand> {
        self.manager_command_tx.clone()
    }

    pub async fn load_rigs(&mut self, gui_sender: &mpsc::Sender<GuiMessage>) -> Result<()> {
        let settings_path = self.data_dir.join(RIGS_FILE);
        let settings = if !settings_path.exists() {
            Settings::default()
        } else {
            let content = std::fs::read_to_string(&settings_path)?;
            toml::from_str(&content)?
        };

        for (rig_id, settings) in settings.rigs.iter().enumerate() {
            if let Err(err) = self.add_device(rig_id, settings.clone()).await {
                eprintln!("Failed to load rig {rig_id}: {err}");
            }
        }

        self.manager_message_tx.send(ManagerMessage::InitialState {
            rigs: settings
                .rigs
                .iter()
                .map(|settings| (settings.id, settings.rig_type.clone()))
                .collect(),
        })?;
        gui_sender
            .send(GuiMessage::InitialState(settings.rigs.clone()))
            .await?;

        self.settings = settings;

        Ok(())
    }

    fn handle_device_message(&mut self, device_message: DeviceMessage) {
        match device_message {
            DeviceMessage::Connected { device_id } => {
                let Some(device) = self.devices.get(&device_id).cloned() else {
                    eprintln!("[manager] Unknown device {device_id} connected");
                    return;
                };
                let rig_model = device.settings.rig_type.clone();
                let poll_interval = device.settings.poll_interval;
                let manager_tx = self.manager_message_tx.clone();

                println!("[manager] Device {device_id} ({rig_model}) connected, initializing...");

                tokio::spawn(async move {
                    let external_api = DeviceExternalApi::new(device.command_tx.clone());
                    let init_result = device.rig_wrapper.execute_init(&external_api).await;

                    if let Err(ref err) = init_result {
                        eprintln!("[manager] Device {device_id} initialization failed: {err}");
                    } else {
                        println!("[manager] Device {device_id} initialized");
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
                                eprintln!(
                                    "[manager] Status polling for device {device_id} failed: {err}"
                                );
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
                            println!("[manager] Status update for {device_id}: {changed_values:?}");
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
                println!("[manager] Device {device_id} disconnected");
                let _ = self
                    .manager_message_tx
                    .send(ManagerMessage::DeviceDisconnected { device_id });
            }
            DeviceMessage::Error { device_id, error } => {
                eprintln!("[manager] Device (id: {device_id}) failed: {error}");
            }
        }
    }

    async fn handle_manager_command(&mut self, manager_command: ManagerCommand) -> Result<()> {
        match manager_command {
            ManagerCommand::CreateOrUpdateDevice { settings } => {
                self.devices.remove(&settings.id);

                let changed_settings = self
                    .settings
                    .rigs
                    .iter_mut()
                    .find(|rig| rig.id == settings.id);
                if let Some(changed_settings) = changed_settings {
                    *changed_settings = settings.clone();
                } else {
                    self.settings.rigs.push(settings.clone());
                };
                let path = self.data_dir.join(RIGS_FILE);
                let content = toml::to_string(&self.settings)?;
                std::fs::write(path, content)?;

                if let Err(err) = self.add_device(settings.id, settings).await {
                    eprintln!("Failed to add device: {err}");
                }
            }
            ManagerCommand::ExecuteCommand {
                device_id,
                command_name,
                params,
                response_channel,
            } => {
                if let Some(device) = self.devices.get(&device_id).cloned() {
                    println!(
                        "[manager] Executing command '{command_name}' on device {device_id} with params {params:?}"
                    );
                    tokio::spawn(async move {
                        let external_api = DeviceExternalApi::new(device.command_tx.clone());
                        let result = device
                            .rig_wrapper
                            .execute_command(&command_name, params, &external_api)
                            .await;

                        let response = match result {
                            Ok(values) => {
                                println!(
                                    "[manager] Command '{command_name}' on device {device_id} succeeded: {values:?}"
                                );
                                CommandResponse::Success(values)
                            }
                            Err(err) => {
                                eprintln!(
                                    "[manager] Command '{command_name}' on device {device_id} failed: {err}"
                                );
                                CommandResponse::Error(err.to_string())
                            }
                        };
                        if let Some(tx) = response_channel {
                            let _ = tx.send(response);
                        }
                    });
                } else {
                    eprintln!("[manager] Device not found: {device_id}");
                    if let Some(tx) = response_channel {
                        let _ = tx.send(CommandResponse::Error(format!(
                            "Device not found: {device_id}"
                        )));
                    }
                }
            }
            ManagerCommand::ListDevices { response_channel } => {
                let devices: HashMap<usize, String> = self
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

                if let Some(pos) = self
                    .settings
                    .rigs
                    .iter()
                    .position(|rig| rig.id == device_id)
                {
                    self.settings.rigs.remove(pos);
                    let path = self.data_dir.join(RIGS_FILE);
                    let content = toml::to_string(&self.settings)?;
                    std::fs::write(path, content)?;
                }
            }
        }
        Ok(())
    }

    pub async fn run(&mut self, gui_sender: mpsc::Sender<GuiMessage>) -> Result<()> {
        self.load_rigs(&gui_sender).await?;

        loop {
            tokio::select! {
                Some(device_message) = self.device_rx.recv() => {
                    self.handle_device_message(device_message);
                },
                Some(manager_command) = self.manager_command_rx.recv() => {
                    self.handle_manager_command(manager_command).await?
                },
            }
        }
    }

    async fn execute_status_commands(device: &Device) -> Result<HashMap<String, Value>> {
        let external_api = DeviceExternalApi::new(device.command_tx.clone());
        external_api.clear_status_values();
        device.rig_wrapper.execute_status(&external_api).await?;
        Ok(external_api.get_status_values())
    }

    pub async fn add_device(&mut self, device_id: usize, settings: RigSettings) -> Result<()> {
        let rig_wrapper = self
            .resources
            .rigs
            .get(&settings.rig_type)
            .context("Unknown rig type")?
            .clone();
        println!(
            "[manager] Opening device {device_id} ({}) on {}",
            settings.rig_type, settings.port
        );
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
