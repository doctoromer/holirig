mod app;
mod commands;
mod state;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use anyhow::Result;
use serde_json::Value;
use tokio::sync::mpsc;

use holyrig_client::client::HolyrigClient;
use holyrig_client::protocol::{ParsedNotification, ServerMessage};

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
    let (msg_tx, mut msg_rx) = mpsc::channel::<ServerMessage>(64);
    let client = HolyrigClient::connect(addr, msg_tx).await?;

    let (global_cmd_tx, mut global_cmd_rx) = mpsc::unbounded_channel::<(usize, RadioCommand)>();
    let mut rig_states: HashMap<usize, Arc<Mutex<RadioState>>> = HashMap::new();

    for info in client.rigs.values() {
        let mut initial = RadioState::new(info.id, info.capabilities.clone());
        initial.connected = info.connected;
        initial.apply_updates(info.status.clone());

        let state = Arc::new(Mutex::new(initial));
        rig_states.insert(info.id, state.clone());

        let _ = app_tx.send(AppMessage::AddRig {
            rig_id: info.id,
            state,
            cmd_tx: global_cmd_tx.clone(),
        });
    }

    let _ = app_tx.send(AppMessage::Connected);

    let states_rx = rig_states.clone();
    let app_tx_rx = app_tx.clone();
    tokio::spawn(async move {
        while let Some(msg) = msg_rx.recv().await {
            if let ServerMessage::Notification(notif) = msg {
                match notif.parse() {
                    ParsedNotification::StatusUpdate { rig_id, updates } => {
                        if let Some(s) = states_rx.get(&rig_id) {
                            s.lock().unwrap().apply_updates(updates);
                        }
                    }
                    ParsedNotification::ConnectionUpdate { rig_id, connected } => {
                        if let Some(s) = states_rx.get(&rig_id) {
                            s.lock().unwrap().connected = connected;
                        }
                    }
                    ParsedNotification::Unknown => {}
                }
            }
        }
        let _ = app_tx_rx.send(AppMessage::Disconnected);
    });

    tokio::spawn(async move {
        let mut client = client;
        while let Some((rig_id, cmd)) = global_cmd_rx.recv().await {
            if let Some((name, params)) = command_to_params(cmd) {
                let _ = client.execute_command(rig_id, name, params).await;
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
        RadioCommand::ClearRit => ("clear_rit", HashMap::new()),
    };
    Some((name.into(), params))
}
