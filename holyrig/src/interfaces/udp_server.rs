use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::net::UdpSocket;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

use crate::resources::Resources;
use crate::serial::ManagerCommand;
use crate::serial::manager::{CommandResponse, ManagerMessage};

// Parse a command string in format: "DEVICE_ID COMMAND_NAME PARAM1=VALUE1 PARAM2=VALUE2"
fn parse_command(cmd: &str) -> Result<(usize, String, HashMap<String, String>)> {
    let mut parts = cmd.split_whitespace();

    let device_id = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing device ID"))?
        .parse()?;

    let command_name = parts
        .next()
        .ok_or_else(|| anyhow::anyhow!("Missing command name"))?
        .to_string();

    let mut params = HashMap::new();
    for param in parts {
        let mut kv = param.split('=');
        let key = kv
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid parameter format"))?;
        let value = kv
            .next()
            .ok_or_else(|| anyhow::anyhow!("Invalid parameter format"))?;
        params.insert(key.to_string(), value.to_string());
    }

    Ok((device_id, command_name, params))
}

fn format_connect_response(resources: &Resources, devices: &HashMap<usize, String>) -> String {
    let mut response = String::new();

    response.push_str("=== Devices ===\n");
    if devices.is_empty() {
        response.push_str("No devices configured\n");
    } else {
        let mut sorted: Vec<_> = devices.iter().collect();
        sorted.sort_by_key(|(id, _)| *id);
        for (id, rig_type) in sorted {
            response.push_str(&format!("{id}: {rig_type}\n"));
        }
    }

    for schema in resources.schemas.values() {
        response.push_str(&format!("\n=== Schema: {} ===\n", schema.name));

        if !schema.enums.is_empty() {
            response.push_str("\nEnums:\n");
            for (name, variants) in &schema.enums {
                response.push_str(&format!("  {name}: {}\n", variants.join(", ")));
            }
        }

        if !schema.commands.is_empty() {
            response.push_str("\nCommands:\n");
            for (name, params) in &schema.commands {
                if params.is_empty() {
                    response.push_str(&format!("  {name}()\n"));
                } else {
                    let param_strs: Vec<_> = params
                        .iter()
                        .map(|p| format!("{} {}", p.param_type, p.name))
                        .collect();
                    response.push_str(&format!("  {name}({})\n", param_strs.join(", ")));
                }
            }
        }

        if !schema.status.is_empty() {
            response.push_str("\nStatus:\n");
            for (name, data_type) in &schema.status {
                response.push_str(&format!("  {data_type} {name}\n"));
            }
        }
    }

    response
}

pub async fn run_server(
    resources: Arc<Resources>,
    command_sender: Sender<ManagerCommand>,
    mut message_receiver: Receiver<ManagerMessage>,
) -> Result<()> {
    let socket = UdpSocket::bind("127.0.0.1:8888").await?;
    println!("UDP debug interface listening on 127.0.0.1:8888");

    let mut buf = [0; 1024];

    let mut device_id_to_addr = HashMap::new();

    loop {
        let (len, addr) = tokio::select! {
            result = socket.recv_from(&mut buf) => {
                result?
            },
            response = message_receiver.recv() => {
                let (mut udp_response, device_id) = match response? {
                    ManagerMessage::InitialState { rigs } => {
                        let mut response = "Available rigs:".to_string();
                        for rig in &rigs {
                            response.push_str(format!("{}: {}\n", rig.id, rig.rig_type).as_str());
                        }
                        (response, None)
                    },
                    ManagerMessage::DeviceConnected { device_id, rig_model: _ } => {
                        (format!("Device {device_id} connected"), Some(device_id))
                    },
                    ManagerMessage::DeviceDisconnected { device_id } => {
                        (format!("Device {device_id} disconnected"), Some(device_id))
                    },
                    ManagerMessage::StatusUpdate { device_id, values } => {
                        let formatted_values: Vec<_> = values
                            .into_iter()
                            .map(|(name, value)| format!("{name} = {value:?}"))
                            .collect();

                        (format!("Device {device_id} status update:\n{}\n", formatted_values.join("\n")), Some(device_id))
                    }
                    ManagerMessage::DeviceError { .. } | ManagerMessage::AvailablePorts(_) => {
                        continue;
                    }
                };
                udp_response.push('\n');

                if let Some(device_id) = device_id {
                    if let Some(addr) = device_id_to_addr.get(&device_id) {
                        socket.send_to(udp_response.as_bytes(), addr).await?;
                    }
                } else {
                    for addr in device_id_to_addr.values() {
                        socket.send_to(udp_response.as_bytes(), addr).await?;
                    }
                }
                continue;
            }
        };

        let cmd = String::from_utf8_lossy(&buf[..len]);
        let trimmed = cmd.trim();

        if trimmed.eq_ignore_ascii_case("connect") {
            let (tx, rx) = oneshot::channel();
            command_sender
                .send(ManagerCommand::ListDevices {
                    response_channel: tx,
                })
                .await?;

            let devices = rx.await?;
            let mut response = format_connect_response(&resources, &devices);
            response.push('\n');
            socket.send_to(response.as_bytes(), addr).await?;
            continue;
        }

        match parse_command(trimmed) {
            Ok((device_id, command_name, params)) => {
                println!("Received command from {addr}: {device_id} {command_name} {params:?}");

                device_id_to_addr.insert(device_id, addr);

                let (tx, rx) = oneshot::channel();
                command_sender
                    .send(ManagerCommand::ExecuteCommand {
                        device_id,
                        command_name: command_name.clone(),
                        params,
                        response_channel: Some(tx),
                    })
                    .await?;

                let response = match rx.await? {
                    CommandResponse::Success(response) => {
                        let mut message =
                            format!("Executed command {command_name} on device {device_id}");
                        if !response.is_empty() {
                            message.push_str(&format!(" {response:?}"));
                        }
                        message.push('\n');
                        message
                    }
                    CommandResponse::Error(err) => {
                        format!(
                            "Failed executing command {command_name} on device {device_id}: {err}\n"
                        )
                    }
                };
                socket.send_to(response.as_bytes(), addr).await?;
            }
            Err(err) => {
                let error_str = format!("ERROR: Invalid command format - {err}\n");
                socket.send_to(error_str.as_bytes(), addr).await?;
            }
        }
    }
}
