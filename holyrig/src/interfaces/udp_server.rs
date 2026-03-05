use anyhow::Result;
use std::collections::HashMap;
use tokio::net::UdpSocket;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

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

pub async fn run_server(
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
                        for (device_id, rig_file_name) in rigs {
                            response.push_str(format!("{device_id}: {rig_file_name}\n").as_str());
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

        match parse_command(&cmd) {
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
