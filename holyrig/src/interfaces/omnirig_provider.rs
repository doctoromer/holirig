use std::collections::HashMap;
use std::sync::Arc;

use omnirig::{
    DummyPortBits, OmniRigProvider, PortBitsControl, RigControl, RigParamX, RigStatusX,
};
use parking_lot::RwLock;
use tokio::sync::broadcast::Receiver;
use tokio::sync::mpsc::Sender;

use crate::runtime::Value;
use crate::serial::ManagerCommand;
use crate::serial::manager::ManagerMessage;

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
    message_receiver: Receiver<ManagerMessage>,
    tokio_runtime: tokio::runtime::Handle,
}

impl HolyRigProvider {
    pub fn new(
        command_sender: Sender<ManagerCommand>,
        message_receiver: Receiver<ManagerMessage>,
        tokio_runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            command_sender,
            message_receiver,
            tokio_runtime,
        }
    }

    fn create_rig(&self, device_id: usize) -> Box<dyn RigControl> {
        let status = Arc::new(RwLock::new(CachedStatus::default()));
        let status_clone = status.clone();
        let mut receiver = self.message_receiver.resubscribe();

        self.tokio_runtime.spawn(async move {
            loop {
                match receiver.recv().await {
                    Ok(ManagerMessage::StatusUpdate { device_id: id, values }) if id == device_id => {
                        let mut s = status_clone.write();
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
                                ("rit_offset", Value::Integer(o)) => s.rit_offset = *o as i32,
                                _ => {}
                            }
                        }
                    }
                    Ok(ManagerMessage::DeviceConnected { device_id: id, .. }) if id == device_id => {
                        status_clone.write().connected = true;
                    }
                    Ok(ManagerMessage::DeviceDisconnected { device_id: id }) if id == device_id => {
                        status_clone.write().connected = false;
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => break,
                    _ => {}
                }
            }
        });

        Box::new(HolyRigControl {
            device_id,
            command_sender: self.command_sender.clone(),
            status,
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

struct HolyRigControl {
    device_id: usize,
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
        // TODO: Get the readable values from the rig file
        (RigParamX::FreqA as i32)
            | (RigParamX::FreqB as i32)
            | (RigParamX::Pitch as i32)
            | (RigParamX::CwU as i32) // mode bits indicate mode is readable
            | (RigParamX::RitOn as i32)
            | (RigParamX::XitOn as i32)
            | (RigParamX::Rx as i32)
            | (RigParamX::SplitOn as i32)
            | (RigParamX::VfoA as i32)
    }

    fn writeable_params(&self) -> i32 {
        // TODO: Get the writeable values from the rig file
        self.readable_params()
            | (RigParamX::VfoAA as i32)
            | (RigParamX::VfoAB as i32)
            | (RigParamX::VfoBA as i32)
            | (RigParamX::VfoBB as i32)
            | (RigParamX::VfoEqual as i32)
            | (RigParamX::VfoSwap as i32)
            | (RigParamX::Rit0 as i32)
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

    // --- Frequency setters ---

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

    // --- VFO ---

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
