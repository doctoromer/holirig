mod app;
mod commands;
mod state;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use anyhow::{Result, bail};
use serde_json::Value;
use tokio::sync::mpsc;

use holyrig_client::capabilities::parse_capabilities;
use holyrig_client::net::TcpClient;
use holyrig_client::protocol::{self, ServerMessage};

use app::RadioApp;
use commands::RadioCommand;
use state::RadioState;

pub async fn run(rig: Option<usize>, addr: SocketAddr) -> Result<()> {
    let mut client = TcpClient::connect(addr).await?;

    let rigs_value = match client.send_and_wait(&protocol::list_rigs_request()).await {
        Ok(response) => response.result.unwrap_or(Value::Null),
        Err(err) => bail!("Cannot reach server at {addr}: {err}"),
    };

    let rig_id = match &rigs_value {
        Value::Object(map) => {
            if let Some(id) = rig {
                if !map.contains_key(&id.to_string()) {
                    let available = map.keys().cloned().collect::<Vec<_>>().join(", ");
                    bail!("Rig {id} not found. Available: [{available}]");
                }
                id
            } else {
                map.iter()
                    .find(|(_, v)| v.as_bool().unwrap_or(false))
                    .and_then(|(k, _)| k.parse::<usize>().ok())
                    .ok_or_else(|| anyhow::anyhow!("No connected rig found. Is holyrig running?"))?
            }
        }
        _ => bail!("Unexpected response from server"),
    };

    let capabilities = match client
        .send_and_wait(&protocol::get_capabilities_request(rig_id))
        .await
    {
        Ok(response) => response
            .result
            .map(|value| parse_capabilities(&value))
            .unwrap_or_default(),
        Err(err) => bail!("Failed to get capabilities for rig {rig_id}: {err}"),
    };

    let fields: Vec<String> = capabilities.status_fields.keys().cloned().collect();
    if !fields.is_empty() {
        let _ = client
            .send_and_wait(&protocol::subscribe_status_request(rig_id, fields))
            .await;
    }

    let mut initial = RadioState::new(rig_id, capabilities);
    initial.connected = true;

    if let Ok(resp) = client
        .send_and_wait(&protocol::get_status_request(rig_id))
        .await
        && let Some(Value::Object(map)) = resp.result
    {
        initial.apply_updates(map.into_iter().collect());
    }

    let (mut tcp_sender, tcp_receiver) = client.into_split();
    let shared = Arc::new(Mutex::new(initial));
    let (cmd_tx, mut cmd_rx) = mpsc::unbounded_channel::<RadioCommand>();

    let state_rx = shared.clone();
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(64);
    tokio::spawn(async move {
        let _ = tcp_receiver.run(msg_tx).await;
    });
    tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if let ServerMessage::Notification(notif) = msg {
                let rid = notif.params.get("rig_id").and_then(|v| v.as_u64());
                match (notif.method.as_str(), rid) {
                    ("status_update", Some(rid)) if rid as usize == rig_id => {
                        if let Some(obj) = notif.params.get("updates").and_then(|v| v.as_object()) {
                            let updates: HashMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            state_rx.lock().unwrap().apply_updates(updates);
                        }
                    }
                    ("connection_update", Some(rid)) if rid as usize == rig_id => {
                        if let Some(connected) =
                            notif.params.get("connected").and_then(|v| v.as_bool())
                        {
                            state_rx.lock().unwrap().connected = connected;
                        }
                    }
                    _ => {}
                }
            }
        }
    });

    let state_tx = shared.clone();
    tokio::spawn(async move {
        while let Some(cmd) = cmd_rx.recv().await {
            let state = state_tx.lock().unwrap().clone();
            if let Some(req) = command_to_request(&state, rig_id, cmd) {
                let _ = tcp_sender.send_request(&req).await;
            }
        }
    });

    let radio_app = RadioApp::new(shared, cmd_tx);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([520.0, 400.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "HolyRig Radio",
        options,
        Box::new(move |_cc| Ok(Box::new(radio_app))),
    )
    .map_err(|err| anyhow::anyhow!("{err}"))?;

    Ok(())
}

fn command_to_request(
    state: &RadioState,
    rig_id: usize,
    cmd: RadioCommand,
) -> Option<holyrig_client::protocol::Request> {
    use RadioCommand::*;

    let (name, params): (&str, HashMap<String, Value>) = match cmd {
        SetFreq { freq, vfo } => {
            if !state.supports("set_freq") {
                return None;
            }
            (
                "set_freq",
                [
                    ("freq".into(), Value::Number(freq.into())),
                    ("target".into(), Value::String(vfo)),
                ]
                .into_iter()
                .collect(),
            )
        }
        SetMode(mode) => {
            if !state.supports("set_mode") {
                return None;
            }
            (
                "set_mode",
                [("mode".into(), Value::String(mode))].into_iter().collect(),
            )
        }
        SetVfo { rx, tx } => {
            if !state.supports("set_vfo") {
                return None;
            }
            (
                "set_vfo",
                [
                    ("rx".into(), Value::String(rx)),
                    ("tx".into(), Value::String(tx)),
                ]
                .into_iter()
                .collect(),
            )
        }
        VfoSwap => {
            if !state.supports("vfo_swap") {
                return None;
            }
            ("vfo_swap", HashMap::new())
        }
        VfoEqual => {
            if !state.supports("vfo_equal") {
                return None;
            }
            ("vfo_equal", HashMap::new())
        }
        Transmit(tx) => {
            if !state.supports("transmit") {
                return None;
            }
            (
                "transmit",
                [("tx".into(), Value::Bool(tx))].into_iter().collect(),
            )
        }
        SetSplit(split) => {
            if !state.supports("set_split") {
                return None;
            }
            (
                "set_split",
                [("split".into(), Value::Bool(split))].into_iter().collect(),
            )
        }
        SetRit(rit) => {
            if !state.supports("set_rit") {
                return None;
            }
            (
                "set_rit",
                [("rit".into(), Value::Bool(rit))].into_iter().collect(),
            )
        }
        SetXit(xit) => {
            if !state.supports("set_xit") {
                return None;
            }
            (
                "set_xit",
                [("xit".into(), Value::Bool(xit))].into_iter().collect(),
            )
        }
        RitOffset(offset) => {
            if !state.supports("rit_offset") {
                return None;
            }
            (
                "rit_offset",
                [("offset".into(), Value::Number(offset.into()))]
                    .into_iter()
                    .collect(),
            )
        }
        ClearRit => {
            if !state.supports("clear_rit") {
                return None;
            }
            ("clear_rit", HashMap::new())
        }
    };

    Some(protocol::execute_command_request(
        rig_id,
        name.into(),
        params,
    ))
}
