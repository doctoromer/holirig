mod app;
mod commands;
mod state;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::Value;
use tokio::sync::mpsc;

use holyrig_client::capabilities::parse_capabilities;
use holyrig_client::net::TcpClient;
use holyrig_client::protocol::{self, ServerMessage};

use app::{AppMessage, RadioApp};
use commands::RadioCommand;
use state::RadioState;

pub async fn run(addr: SocketAddr) -> Result<()> {
    let (app_tx, app_rx) = std::sync::mpsc::channel::<AppMessage>();

    tokio::spawn(async move {
        loop {
            let _ = app_tx.send(AppMessage::Connecting);
            match connect_and_run(addr, app_tx.clone()).await {
                Ok(()) => {}
                Err(e) => {
                    let _ = app_tx.send(AppMessage::ConnectionError(e.to_string()));
                }
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
        }
    });

    let radio_app = RadioApp::new(app_rx);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([560.0, 440.0])
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "HolyRig Radio",
        options,
        Box::new(move |_cc| Ok(Box::new(radio_app))),
    )
    .map_err(|e| anyhow::anyhow!("{e}"))?;

    Ok(())
}

async fn connect_and_run(
    addr: SocketAddr,
    app_tx: std::sync::mpsc::Sender<AppMessage>,
) -> Result<()> {
    let mut client = TcpClient::connect(addr).await?;

    let rigs_value = match client.send_and_wait(&protocol::list_rigs_request()).await {
        Ok(resp) => resp.result.unwrap_or(Value::Null),
        Err(e) => bail!("list_rigs failed: {e}"),
    };

    let rig_ids: Vec<usize> = match &rigs_value {
        Value::Object(map) => map.keys().filter_map(|k| k.parse().ok()).collect(),
        _ => bail!("Unexpected response from list_rigs"),
    };

    if rig_ids.is_empty() {
        bail!("Server has no rigs configured");
    }

    let (global_cmd_tx, mut global_cmd_rx) = mpsc::unbounded_channel::<(usize, RadioCommand)>();

    let mut rig_states: HashMap<usize, Arc<Mutex<RadioState>>> = HashMap::new();

    for &rig_id in &rig_ids {
        let caps = match client
            .send_and_wait(&protocol::get_capabilities_request(rig_id))
            .await
        {
            Ok(resp) => resp
                .result
                .map(|v| parse_capabilities(&v))
                .unwrap_or_default(),
            Err(e) => bail!("get_capabilities failed for rig {rig_id}: {e}"),
        };

        let fields: Vec<String> = caps.status_fields.keys().cloned().collect();
        if !fields.is_empty() {
            let _ = client
                .send_and_wait(&protocol::subscribe_status_request(rig_id, fields))
                .await;
        }

        let mut initial = RadioState::new(rig_id, caps);
        initial.connected = true;

        if let Ok(resp) = client
            .send_and_wait(&protocol::get_status_request(rig_id))
            .await
            && let Some(Value::Object(map)) = resp.result
        {
            initial.apply_updates(map.into_iter().collect());
        }

        let state = Arc::new(Mutex::new(initial));
        rig_states.insert(rig_id, state.clone());

        let _ = app_tx.send(AppMessage::AddRig {
            rig_id,
            state,
            cmd_tx: global_cmd_tx.clone(),
        });
    }

    let _ = app_tx.send(AppMessage::Connected);

    let (mut tcp_sender, tcp_receiver) = client.into_split();

    let states_rx = rig_states.clone();
    let app_tx_rx = app_tx.clone();
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(64);
    tokio::spawn(async move {
        let _ = tcp_receiver.run(msg_tx).await;
    });
    tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if let ServerMessage::Notification(notif) = msg {
                let rid = notif.params.get("rig_id").and_then(|v| v.as_u64());
                match (notif.method.as_str(), rid) {
                    ("status_update", Some(rid)) => {
                        if let Some(s) = states_rx.get(&(rid as usize))
                            && let Some(obj) =
                                notif.params.get("updates").and_then(|v| v.as_object())
                        {
                            let updates: HashMap<String, Value> =
                                obj.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
                            s.lock().unwrap().apply_updates(updates);
                        }
                    }
                    ("connection_update", Some(rid)) => {
                        if let Some(s) = states_rx.get(&(rid as usize))
                            && let Some(connected) =
                                notif.params.get("connected").and_then(|v| v.as_bool())
                        {
                            s.lock().unwrap().connected = connected;
                        }
                    }
                    _ => {}
                }
            }
        }
        let _ = app_tx_rx.send(AppMessage::Disconnected);
    });

    tokio::spawn(async move {
        while let Some((rig_id, cmd)) = global_cmd_rx.recv().await {
            if let Some((name, params)) = command_to_params(cmd) {
                let req = protocol::execute_command_request(rig_id, name, params);
                let _ = tcp_sender.send_request(&req).await;
            }
        }
    });

    Ok(())
}

fn command_to_params(cmd: RadioCommand) -> Option<(String, HashMap<String, Value>)> {
    let (name, params): (&str, HashMap<String, Value>) = match cmd {
        RadioCommand::SetFreq { freq, vfo } => (
            "set_freq",
            [
                ("freq".into(), Value::Number(freq.into())),
                ("target".into(), Value::String(vfo)),
            ]
            .into_iter()
            .collect(),
        ),
        RadioCommand::SetMode(mode) => (
            "set_mode",
            [("mode".into(), Value::String(mode))].into_iter().collect(),
        ),
        RadioCommand::SetVfo { rx, tx } => (
            "set_vfo",
            [
                ("rx".into(), Value::String(rx)),
                ("tx".into(), Value::String(tx)),
            ]
            .into_iter()
            .collect(),
        ),
        RadioCommand::VfoSwap => ("vfo_swap", HashMap::new()),
        RadioCommand::VfoEqual => ("vfo_equal", HashMap::new()),
        RadioCommand::Transmit(tx) => (
            "transmit",
            [("tx".into(), Value::Bool(tx))].into_iter().collect(),
        ),
        RadioCommand::SetSplit(split) => (
            "set_split",
            [("split".into(), Value::Bool(split))].into_iter().collect(),
        ),
        RadioCommand::SetRit(rit) => (
            "set_rit",
            [("rit".into(), Value::Bool(rit))].into_iter().collect(),
        ),
        RadioCommand::SetXit(xit) => (
            "set_xit",
            [("xit".into(), Value::Bool(xit))].into_iter().collect(),
        ),
        RadioCommand::RitOffset(offset) => (
            "rit_offset",
            [("offset".into(), Value::Number(offset.into()))]
                .into_iter()
                .collect(),
        ),
        _clear_rit => ("clear_rit", HashMap::new()),
    };
    Some((name.into(), params))
}
