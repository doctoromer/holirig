use std::collections::HashMap;
use std::sync::Arc;

use omnirig::{DummyPortBits, OmniRigProvider, PortBitsControl, RigControl, RigParamX, RigStatusX};
use parking_lot::RwLock;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;
use tracing::{error, warn};

use crate::resources::Resources;
use crate::rig_settings::{RigId, RigSettings};
use crate::runtime::RigFile;
use crate::runtime::Value;
use crate::serial::ManagerCommand;
use crate::serial::manager::{ConnectionStatus, ManagerMessage};

#[derive(Default)]
struct CachedStatus {
    freq_a: i32,
    freq_b: i32,
    mode: String,
    vfo: String,
    cw_pitch: i32,
    transmit: bool,
    split: bool,
    rit: bool,
    xit: bool,
    rit_offset: i32,
    connected: bool,
    supported_modes: i32,
    readable_params: i32,
}

fn mode_str_to_rigparam(mode: &str) -> RigParamX {
    match mode {
        "CWU" => RigParamX::CwU,
        "CWL" => RigParamX::CwL,
        "USB" => RigParamX::SsbU,
        "LSB" => RigParamX::SsbL,
        "DIGIU" => RigParamX::DigU,
        "DIGIL" => RigParamX::DigL,
        "AM" => RigParamX::Am,
        "FM" => RigParamX::Fm,
        _ => RigParamX::Unknown,
    }
}

fn rigparam_to_mode_str(param: RigParamX) -> &'static str {
    match param {
        RigParamX::CwU => "CWU",
        RigParamX::CwL => "CWL",
        RigParamX::SsbU => "USB",
        RigParamX::SsbL => "LSB",
        RigParamX::DigU => "DIGIU",
        RigParamX::DigL => "DIGIL",
        RigParamX::Am => "AM",
        RigParamX::Fm => "FM",
        _ => "USB",
    }
}

// --- VFO translation ---

fn vfo_str_to_rigparam(vfo: &str) -> RigParamX {
    match vfo {
        "A" => RigParamX::VfoA,
        "B" => RigParamX::VfoB,
        _ => RigParamX::Unknown,
    }
}

fn mode_variant_to_rigparam(variant_name: &str) -> Option<RigParamX> {
    match variant_name {
        "CWU" => Some(RigParamX::CwU),
        "CWL" => Some(RigParamX::CwL),
        "USB" => Some(RigParamX::SsbU),
        "LSB" => Some(RigParamX::SsbL),
        "DIGIU" => Some(RigParamX::DigU),
        "DIGIL" => Some(RigParamX::DigL),
        "AM" => Some(RigParamX::Am),
        "FM" => Some(RigParamX::Fm),
        _ => None,
    }
}

fn compute_supported_modes(rig_file: &RigFile) -> i32 {
    rig_file
        .impl_block
        .enums
        .iter()
        .find(|e| e.name == "Mode")
        .map(|mode_enum| {
            mode_enum
                .variants
                .keys()
                .filter_map(|name| mode_variant_to_rigparam(name))
                .fold(0i32, |acc, p| acc | p as i32)
        })
        .unwrap_or(0)
}

fn status_field_to_rigparams(field: &str) -> i32 {
    match field {
        "freq_a" => RigParamX::FreqA as i32,
        "freq_b" => RigParamX::FreqB as i32,
        "cw_pitch" => RigParamX::Pitch as i32,
        "rit_offset" => RigParamX::RitOffset as i32,
        "transmit" => (RigParamX::Rx as i32) | (RigParamX::Tx as i32),
        "split" => (RigParamX::SplitOn as i32) | (RigParamX::SplitOff as i32),
        "rit" => (RigParamX::RitOn as i32) | (RigParamX::RitOff as i32),
        "xit" => (RigParamX::XitOn as i32) | (RigParamX::XitOff as i32),
        "vfo" => (RigParamX::VfoA as i32) | (RigParamX::VfoB as i32),
        _ => 0,
    }
}

fn compute_readable_params(rig_file: &RigFile) -> i32 {
    let status_fields = rig_file.get_supported_status_fields();
    let mut bitmask: i32 = 0;
    for field in &status_fields {
        if field == "mode" {
            bitmask |= compute_supported_modes(rig_file);
        } else {
            bitmask |= status_field_to_rigparams(field);
        }
    }
    bitmask
}

fn rigparam_to_vfo_args(param: RigParamX) -> Option<(&'static str, &'static str)> {
    match param {
        RigParamX::VfoAA => Some(("A", "A")),
        RigParamX::VfoAB => Some(("A", "B")),
        RigParamX::VfoBA => Some(("B", "A")),
        RigParamX::VfoBB => Some(("B", "B")),
        RigParamX::VfoA => Some(("A", "Current")),
        RigParamX::VfoB => Some(("B", "Current")),
        _ => None,
    }
}

pub struct HolyRigProvider {
    command_sender: Sender<ManagerCommand>,
    statuses: [Arc<RwLock<CachedStatus>>; 2],
    rig_ids: [Option<RigId>; 2],
    tokio_runtime: tokio::runtime::Handle,
}

impl HolyRigProvider {
    pub fn new(
        command_sender: Sender<ManagerCommand>,
        mut message_receiver: Receiver<ManagerMessage>,
        resources: Arc<Resources>,
        tokio_runtime: tokio::runtime::Handle,
        initial_rigs: &[RigSettings],
    ) -> Self {
        let statuses = [
            Arc::new(RwLock::new(CachedStatus::default())),
            Arc::new(RwLock::new(CachedStatus::default())),
        ];

        let mut rig_ids: [Option<RigId>; 2] = [None; 2];
        for (i, rig) in initial_rigs.iter().take(2).enumerate() {
            rig_ids[i] = Some(rig.id);
            if let Some(interpreter) = resources.rigs.get(&rig.config.rig_type) {
                let rig_file = interpreter.rig_file();
                let modes = compute_supported_modes(rig_file);
                let readable = compute_readable_params(rig_file);
                let mut status = statuses[i].write();
                status.supported_modes = modes;
                status.readable_params = readable;
            }
        }

        let statuses_clone = statuses.clone();
        let rig_ids_clone = rig_ids;
        tokio_runtime.spawn(async move {
            loop {
                match message_receiver.recv().await {
                    Ok(ManagerMessage::StatusUpdate { device_id, values }) => {
                        let slot = rig_ids_clone.iter().position(|id| *id == Some(device_id));
                        if let Some(slot) = slot {
                            if let Some(status) = statuses_clone.get(slot) {
                                let mut s = status.write();
                                s.connected = true;
                                for (name, value) in values {
                                    match (name.as_str(), &value) {
                                        ("freq_a", Value::Integer(f)) => s.freq_a = *f as i32,
                                        ("freq_b", Value::Integer(f)) => s.freq_b = *f as i32,
                                        ("mode", Value::String(m)) => s.mode = m.clone(),
                                        ("vfo", Value::String(v)) => s.vfo = v.clone(),
                                        ("cw_pitch", Value::Integer(p)) => s.cw_pitch = *p as i32,
                                        ("transmit", Value::Boolean(t)) => s.transmit = *t,
                                        ("split", Value::Boolean(sp)) => s.split = *sp,
                                        ("rit", Value::Boolean(r)) => s.rit = *r,
                                        ("xit", Value::Boolean(x)) => s.xit = *x,
                                        ("rit_offset", Value::Integer(o)) => {
                                            s.rit_offset = *o as i32
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                    Ok(ManagerMessage::ConnectionStatusChanged { device_id, status }) => {
                        let slot = rig_ids_clone.iter().position(|id| *id == Some(device_id));
                        if let Some(slot) = slot {
                            if let Some(cached) = statuses_clone.get(slot) {
                                cached.write().connected =
                                    matches!(status, ConnectionStatus::Connected);
                            }
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    Err(err) => {
                        error!(%err, "OmniRig recv error");
                        break;
                    }
                    Ok(ManagerMessage::AvailablePorts(_)) => {}
                }
            }
        });

        Self {
            command_sender,
            statuses,
            rig_ids,
            tokio_runtime,
        }
    }

    fn create_rig(&self, slot: usize) -> Box<dyn RigControl> {
        let Some(device_id) = self.rig_ids[slot] else {
            warn!(slot, "OmniRig requested rig for unconfigured slot");
            return Box::new(UnconfiguredRig);
        };
        Box::new(HolyRigControl {
            device_id,
            command_sender: self.command_sender.clone(),
            status: self.statuses[slot].clone(),
            tokio_runtime: self.tokio_runtime.clone(),
        })
    }
}

impl OmniRigProvider for HolyRigProvider {
    fn create_rig1(&self) -> Box<dyn RigControl> {
        self.create_rig(0)
    }

    fn create_rig2(&self) -> Box<dyn RigControl> {
        self.create_rig(1)
    }
}

struct UnconfiguredRig;

impl RigControl for UnconfiguredRig {
    fn rig_type(&self) -> String {
        "Not configured".to_string()
    }

    fn status(&self) -> RigStatusX {
        RigStatusX::NotConfigured
    }

    fn status_str(&self) -> String {
        "Not configured".to_string()
    }

    fn readable_params(&self) -> i32 {
        0
    }

    fn writeable_params(&self) -> i32 {
        0
    }

    fn freq(&self) -> i32 {
        0
    }

    fn set_freq(&self, _value: i32) {}

    fn freq_a(&self) -> i32 {
        0
    }

    fn set_freq_a(&self, _value: i32) {}

    fn freq_b(&self) -> i32 {
        0
    }

    fn set_freq_b(&self, _value: i32) {}

    fn rit_offset(&self) -> i32 {
        0
    }

    fn set_rit_offset(&self, _value: i32) {}

    fn pitch(&self) -> i32 {
        0
    }

    fn set_pitch(&self, _value: i32) {}

    fn vfo(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_vfo(&self, _value: RigParamX) {}

    fn split(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_split(&self, _value: RigParamX) {}

    fn rit(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_rit(&self, _value: RigParamX) {}

    fn xit(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_xit(&self, _value: RigParamX) {}

    fn tx(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_tx(&self, _value: RigParamX) {}

    fn mode(&self) -> RigParamX {
        RigParamX::Unknown
    }

    fn set_mode(&self, _value: RigParamX) {}

    fn send_custom_command(&self, _command: &[u8], _reply_length: i32, _reply_end: &[u8]) {}

    fn port_bits(&self) -> Option<Box<dyn PortBitsControl>> {
        Some(Box::new(DummyPortBits::new()))
    }
}

struct HolyRigControl {
    device_id: RigId,
    command_sender: Sender<ManagerCommand>,
    status: Arc<RwLock<CachedStatus>>,
    tokio_runtime: tokio::runtime::Handle,
}

impl HolyRigControl {
    fn send_command(&self, command_name: &str, params: HashMap<String, String>) {
        let sender = self.command_sender.clone();
        let device_id = self.device_id;
        let command_name = command_name.to_string();
        self.tokio_runtime.block_on(async {
            let _ = sender
                .send(ManagerCommand::ExecuteCommand {
                    device_id,
                    command_name,
                    params,
                    response_channel: None,
                })
                .await;
        });
    }
}

impl RigControl for HolyRigControl {
    fn rig_type(&self) -> String {
        // TODO: replace with the real rig type
        "HolyRig".to_string()
    }

    fn status(&self) -> RigStatusX {
        if self.status.read().connected {
            RigStatusX::Online
        } else {
            RigStatusX::NotResponding
        }
    }

    fn status_str(&self) -> String {
        if self.status.read().connected {
            "Online".to_string()
        } else {
            "Not responding".to_string()
        }
    }

    fn readable_params(&self) -> i32 {
        self.status.read().readable_params
    }

    fn writeable_params(&self) -> i32 {
        self.readable_params()
            | (RigParamX::VfoAA as i32)
            | (RigParamX::VfoAB as i32)
            | (RigParamX::VfoBA as i32)
            | (RigParamX::VfoBB as i32)
            | (RigParamX::VfoEqual as i32)
            | (RigParamX::VfoSwap as i32)
            | (RigParamX::Rit0 as i32)
            | self.status.read().supported_modes
    }

    fn freq(&self) -> i32 {
        let s = self.status.read();
        match s.vfo.as_str() {
            "B" => s.freq_b,
            // TODO: Handle unknown vfo
            _ => s.freq_a,
        }
    }

    fn freq_a(&self) -> i32 {
        self.status.read().freq_a
    }

    fn freq_b(&self) -> i32 {
        self.status.read().freq_b
    }

    fn rit_offset(&self) -> i32 {
        self.status.read().rit_offset
    }

    fn pitch(&self) -> i32 {
        self.status.read().cw_pitch
    }

    fn set_freq(&self, value: i32) {
        self.send_command(
            "set_freq",
            HashMap::from([
                ("freq".to_string(), value.to_string()),
                ("target".to_string(), "Current".to_string()),
            ]),
        );
    }

    fn set_freq_a(&self, value: i32) {
        self.send_command(
            "set_freq",
            HashMap::from([
                ("freq".to_string(), value.to_string()),
                ("target".to_string(), "A".to_string()),
            ]),
        );
    }

    fn set_freq_b(&self, value: i32) {
        self.send_command(
            "set_freq",
            HashMap::from([
                ("freq".to_string(), value.to_string()),
                ("target".to_string(), "B".to_string()),
            ]),
        );
    }

    fn set_rit_offset(&self, value: i32) {
        self.send_command(
            "rit_offset",
            HashMap::from([("offset".to_string(), value.to_string())]),
        );
    }

    fn set_pitch(&self, value: i32) {
        self.send_command(
            "cw_pitch",
            HashMap::from([("pitch".to_string(), value.to_string())]),
        );
    }

    fn vfo(&self) -> RigParamX {
        vfo_str_to_rigparam(&self.status.read().vfo)
    }

    fn set_vfo(&self, value: RigParamX) {
        match value {
            RigParamX::VfoEqual => {
                self.send_command("vfo_equal", HashMap::new());
            }
            RigParamX::VfoSwap => {
                self.send_command("vfo_swap", HashMap::new());
            }
            _ => {
                if let Some((rx, tx)) = rigparam_to_vfo_args(value) {
                    self.send_command(
                        "set_vfo",
                        HashMap::from([
                            ("rx".to_string(), rx.to_string()),
                            ("tx".to_string(), tx.to_string()),
                        ]),
                    );
                }
            }
        }
    }

    fn split(&self) -> RigParamX {
        if self.status.read().split {
            RigParamX::SplitOn
        } else {
            RigParamX::SplitOff
        }
    }

    fn set_split(&self, value: RigParamX) {
        // TODO: handle invalid rig param
        let on = value == RigParamX::SplitOn;
        self.send_command(
            "set_split",
            HashMap::from([("split".to_string(), on.to_string())]),
        );
    }

    fn rit(&self) -> RigParamX {
        if self.status.read().rit {
            RigParamX::RitOn
        } else {
            RigParamX::RitOff
        }
    }

    fn set_rit(&self, value: RigParamX) {
        // TODO: handle invalid rig param
        let on = value == RigParamX::RitOn;
        self.send_command(
            "set_rit",
            HashMap::from([("rit".to_string(), on.to_string())]),
        );
    }

    fn xit(&self) -> RigParamX {
        if self.status.read().xit {
            RigParamX::XitOn
        } else {
            RigParamX::XitOff
        }
    }

    fn set_xit(&self, value: RigParamX) {
        // TODO: handle invalid rig param
        let on = value == RigParamX::XitOn;
        self.send_command(
            "set_xit",
            HashMap::from([("xit".to_string(), on.to_string())]),
        );
    }

    fn tx(&self) -> RigParamX {
        if self.status.read().transmit {
            RigParamX::Tx
        } else {
            RigParamX::Rx
        }
    }

    fn set_tx(&self, value: RigParamX) {
        // TODO: handle invalid rig param
        let on = value == RigParamX::Tx;
        self.send_command(
            "transmit",
            HashMap::from([("tx".to_string(), on.to_string())]),
        );
    }

    fn mode(&self) -> RigParamX {
        mode_str_to_rigparam(&self.status.read().mode)
    }

    fn set_mode(&self, value: RigParamX) {
        self.send_command(
            "set_mode",
            HashMap::from([("mode".to_string(), rigparam_to_mode_str(value).to_string())]),
        );
    }

    // Unsupported right now
    fn send_custom_command(&self, _command: &[u8], _reply_length: i32, _reply_end: &[u8]) {}

    // Unsupported right now
    fn port_bits(&self) -> Option<Box<dyn PortBitsControl>> {
        Some(Box::new(DummyPortBits::new()))
    }
}
