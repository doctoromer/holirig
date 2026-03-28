use crate::runtime::Value;
use anyhow::{Context, Result, bail};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::{error, info, warn};

use crate::rig_settings::RigId;
use crate::serial::ManagerCommand;
use crate::serial::manager::ManagerMessage;

#[derive(Debug)]
enum RigctlCommand {
    GetPowerstat,
    CheckVfo,
    DumpState,

    SetFreq(f64),
    GetFreq(String),
    SetMode(String),
    GetMode(String),
    SetVfo(String),
    GetVfo,
    SetPtt(bool),
    GetPtt,
    SetSplit(bool),
    GetSplit(String),
    SetRit(i32),
    GetRit,
    SetXit(i32),
    GetXit,
    Quit,
}

fn parse_rigctl_command(line: &str) -> Result<RigctlCommand> {
    let mut params = line.split(' ');
    let command = params.next().context("Missing command name")?;

    let first_arg = params.next();

    let command = match command.chars().next().context("Empty command")? {
        'F' => RigctlCommand::SetFreq(first_arg.context("Missing freq")?.parse()?),
        'f' => RigctlCommand::GetFreq(first_arg.context("Missing VFO")?.to_string()),
        'M' => RigctlCommand::SetMode(first_arg.context("Missing mode")?.to_string()),
        'm' => RigctlCommand::GetMode(first_arg.context("Missing VFO")?.to_string()),
        'V' => RigctlCommand::SetVfo(first_arg.context("Missing VFO")?.to_string()),
        'v' => RigctlCommand::GetVfo,
        'T' => RigctlCommand::SetPtt(first_arg.context("Missing PTT")?.parse()?),
        't' => RigctlCommand::GetPtt,
        'S' => RigctlCommand::SetSplit(first_arg.context("Missing split")?.parse()?),
        's' => RigctlCommand::GetSplit(first_arg.context("Missing VFO")?.to_string()),
        'J' => RigctlCommand::SetRit(first_arg.context("Missing rit")?.parse()?),
        'j' => RigctlCommand::GetRit,
        'Z' => RigctlCommand::SetXit(first_arg.context("Missing xit")?.parse()?),
        'z' => RigctlCommand::GetXit,
        'q' => RigctlCommand::Quit,
        '\\' => match line[1..].trim() {
            "get_powerstat" => RigctlCommand::GetPowerstat,
            "chk_vfo" => RigctlCommand::CheckVfo,
            "dump_state" => RigctlCommand::DumpState,
            _ => bail!("Unknown command: {line}"),
        },
        _ => bail!("Unknown command: {command}"),
    };
    Ok(command)
}

struct DeviceStatus {
    freq_a: i64,
    freq_b: i64,
    vfo: String,
    mode: String,
    transmit: bool,
    rit: bool,
    xit: bool,
}

impl Default for DeviceStatus {
    fn default() -> Self {
        Self {
            freq_a: 0,
            freq_b: 0,
            vfo: "A".to_string(),
            mode: "USB".to_string(),
            transmit: false,
            rit: false,
            xit: false,
        }
    }
}

async fn handle_client(
    mut socket: TcpStream,
    device_status: Arc<RwLock<DeviceStatus>>,
    command_sender: Sender<ManagerCommand>,
    addr: SocketAddr,
) -> Result<()> {
    let (reader, mut writer) = socket.split();

    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    loop {
        line.clear();
        if reader.read_line(&mut line).await? == 0 {
            break;
        }

        match parse_rigctl_command(&line) {
            Ok(RigctlCommand::Quit) => break,
            Ok(command) => {
                let (command_name, params) = match command {
                    RigctlCommand::SetFreq(freq) => (
                        "set_freq",
                        HashMap::from([
                            ("freq".to_string(), freq.to_string()),
                            ("target".to_string(), "Current".to_string()),
                        ]),
                    ),
                    RigctlCommand::GetFreq(vfo) => {
                        let freq = {
                            let device_status = device_status.read();
                            match vfo.as_str().trim() {
                                "VFOA" => device_status.freq_a,
                                "VFOB" => device_status.freq_b,
                                _ => {
                                    bail!("Unknown vfo: {vfo}");
                                }
                            }
                        };
                        writer.write_all(format!("{}\n", freq).as_bytes()).await?;
                        continue;
                    }
                    RigctlCommand::SetMode(mode) => {
                        ("set_mode", HashMap::from([("mode".to_string(), mode)]))
                    }
                    RigctlCommand::GetMode(_vfo) => {
                        let mode = { device_status.read().mode.clone() };
                        writer.write_all(format!("{} 0\n", mode).as_bytes()).await?;
                        continue;
                    }
                    RigctlCommand::SetVfo(vfo) => (
                        "set_vfo",
                        HashMap::from([("rx".to_string(), vfo.clone()), ("tx".to_string(), vfo)]),
                    ),
                    RigctlCommand::GetVfo => {
                        let vfo = { device_status.read().vfo.clone() };
                        writer.write_all(format!("VFO{}\n", vfo).as_bytes()).await?;
                        continue;
                    }
                    RigctlCommand::SetPtt(ptt) => (
                        "transmit",
                        HashMap::from([("tx".to_string(), ptt.to_string())]),
                    ),
                    RigctlCommand::GetPtt => {
                        let transmit = { device_status.read().transmit };
                        writer
                            .write_all(format!("{}\n", transmit as i32).as_bytes())
                            .await?;
                        continue;
                    }
                    RigctlCommand::SetSplit(split) => (
                        "set_split",
                        HashMap::from([("split".to_string(), split.to_string())]),
                    ),
                    RigctlCommand::GetSplit(_vfo) => {
                        // Split status not available
                        let vfo = { device_status.read().vfo.clone() };
                        writer
                            .write_all(format!("0\nVFO{vfo}\n").as_bytes())
                            .await?;
                        continue;
                    }
                    RigctlCommand::SetRit(_offset) => {
                        // RIT offset not supported
                        writer.write_all(b"RPRT -1\n").await?;
                        continue;
                    }
                    RigctlCommand::GetRit => {
                        let rit = { device_status.read().rit };
                        writer
                            .write_all(format!("{}\n", rit as i32).as_bytes())
                            .await?;
                        continue;
                    }
                    RigctlCommand::SetXit(_offset) => {
                        // XIT offset not supported
                        writer.write_all(b"RPRT -1\n").await?;
                        continue;
                    }
                    RigctlCommand::GetXit => {
                        let xit = { device_status.read().xit };
                        writer
                            .write_all(format!("{}\n", xit as i32).as_bytes())
                            .await?;
                        continue;
                    }
                    RigctlCommand::GetPowerstat => {
                        writer.write_all(b"1\n").await?;
                        continue;
                    }
                    RigctlCommand::CheckVfo => {
                        writer.write_all(b"1\n").await?;
                        continue;
                    }
                    RigctlCommand::DumpState => {
                        let dump_state_output: [&[u8]; _] = [
                            // Protocol version
                            b"1\n",
                            // Always zero
                            b"0\n",
                            // Model id, currently 7300
                            b"7073\n",
                            // RX frequency list, currently empty
                            b"0 0 0 0 0 0 0\n",
                            // TX frequency list, currently empty
                            b"0 0 0 0 0 0 0\n",
                            // Tuning steps, currently empty
                            b"0 0\n",
                            // Filters, currently empty
                            b"0 0\n",
                            // Max rit
                            b"9999\n",
                            // Max xit
                            b"9999\n",
                            // Max ifshift
                            b"0\n",
                            // Announces?
                            b"0\n",
                            // Preamp
                            b"1 2\n ",
                            // Attenuator
                            b"20\n ",
                            // Some getter and setter functions
                            b"0xfc00c90133fe\n",
                            b"0xfc00c90133fe\n",
                            b"0xc7fff74677f3f\n",
                            b"0xc7f7000677f3f\n",
                            b"0x35\n",
                            b"0x35\n",
                            // Other parameters
                            b"vfo_ops=0x81f\n",
                            b"ptt_type=0x1\n",
                            b"targetable_vfo=0x3\n",
                            b"has_set_vfo=1\n",
                            b"has_get_vfo=0\n",
                            b"has_set_freq=1\n",
                            b"has_get_freq=1\n",
                            b"has_set_conf=1\n",
                            b"has_get_conf=1\n",
                            b"has_power2mW=1\n",
                            b"has_mW2power=1\n",
                            b"timeout=1000\n",
                            b"rig_model=3073\n",
                            b"rigctld_version=Hamlib 4.5.5 Apr 05 11:43:08Z 2023 SHA=6eecd3\n",
                            b"agc_levels=0=OFF 1=FAST 2=MEDIUM 3=SLOW\n",
                            b"ctcss_list= 60.0 67.0 69.3 71.9 74.4 77.0 79.7 82.5 85.4 88.5 91.5 94.8 97.4 100.0 103.5 107.2 110.9 114.8 118.8 120.0 123.0 127.3 131.8 136.5 141.3 146.2 151.4 156.7 159.8 162.2 165.5 167.9 171.3 173.8 177.3 179.9 183.5 186.2 189.9 192.8 196.6 199.5 203.5 206.5 210.7 218.1 225.7 229.1 233.6 241.8 250.3 254.1\n",
                            b"done\n",
                        ];
                        for line in dump_state_output {
                            writer.write_all(line).await?;
                        }
                        continue;
                    }
                    RigctlCommand::Quit => unreachable!(),
                };

                if let Err(e) = command_sender
                    .send(ManagerCommand::ExecuteCommand {
                        device_id: RigId::default(), // TODO: Support multiple devices
                        command_name: command_name.to_string(),
                        params,
                        response_channel: None,
                    })
                    .await
                {
                    error!(%e, "Failed to send command");
                    break;
                }

                writer.write_all(b"RPRT 0\n").await?;
            }
            Err(e) => {
                warn!(%addr, %e, "Error parsing command");
                writer.write_all(b"RPRT 1\n").await?;
            }
        }
    }
    Ok(())
}

pub async fn run_server(
    command_sender: Sender<ManagerCommand>,
    mut message_receiver: Receiver<ManagerMessage>,
) -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4532").await?;
    info!("Rigctld server listening on 127.0.0.1:4532");

    let device_status = Arc::new(RwLock::new(DeviceStatus::default()));

    loop {
        tokio::select! {
            accept_result = listener.accept() => {
                let (socket, addr) = accept_result?;
                let command_sender = command_sender.clone();

                let device_status = device_status.clone();
                tokio::spawn(async move {
                    handle_client(socket, device_status, command_sender, addr).await.unwrap()
                });
            }
            Ok(msg) = message_receiver.recv() => {
                let device_status = device_status.clone();
                if let ManagerMessage::StatusUpdate { values, .. } = msg {
                    let mut device_status = device_status.write();
                    for (name, value) in values {
                        match (name.as_str(), value) {
                            ("freq_a", Value::Integer(f)) => device_status.freq_a = f,
                            ("freq_b", Value::Integer(f)) => device_status.freq_b = f,
                            ("vfo", Value::String(v)) => device_status.vfo = v,
                            ("mode", Value::String(m)) => device_status.mode = m,
                            ("transmit", Value::Boolean(t)) => device_status.transmit = t,
                            ("rit", Value::Boolean(r)) => device_status.rit = r,
                            ("xit", Value::Boolean(x)) => device_status.xit = x,
                            _ => {}
                        }
                    }
                }
            }
        }
    }
}
