use std::sync::RwLock;

use crate::enums::{RigParamX, RigStatusX};

pub trait OmniRigProvider: Send + Sync + 'static {
    fn create_rig1(&self) -> Box<dyn RigControl>;
    fn create_rig2(&self) -> Box<dyn RigControl>;
}

pub trait RigControl: Send + Sync {
    fn rig_type(&self) -> String;
    fn status(&self) -> RigStatusX;
    fn status_str(&self) -> String;
    fn readable_params(&self) -> i32;
    fn writeable_params(&self) -> i32;

    fn freq(&self) -> i32;
    fn set_freq(&self, value: i32);
    fn freq_a(&self) -> i32;
    fn set_freq_a(&self, value: i32);
    fn freq_b(&self) -> i32;
    fn set_freq_b(&self, value: i32);
    fn rit_offset(&self) -> i32;
    fn set_rit_offset(&self, value: i32);
    fn pitch(&self) -> i32;
    fn set_pitch(&self, value: i32);

    fn vfo(&self) -> RigParamX;
    fn set_vfo(&self, value: RigParamX);
    fn split(&self) -> RigParamX;
    fn set_split(&self, value: RigParamX);
    fn rit(&self) -> RigParamX;
    fn set_rit(&self, value: RigParamX);
    fn xit(&self) -> RigParamX;
    fn set_xit(&self, value: RigParamX);
    fn tx(&self) -> RigParamX;
    fn set_tx(&self, value: RigParamX);
    fn mode(&self) -> RigParamX;
    fn set_mode(&self, value: RigParamX);

    fn send_custom_command(&self, command: &[u8], reply_length: i32, reply_end: &[u8]);

    fn port_bits(&self) -> Option<Box<dyn PortBitsControl>>;
}

pub trait PortBitsControl: Send + Sync {
    fn lock(&self) -> bool;
    fn unlock(&self);
    fn rts(&self) -> bool;
    fn set_rts(&self, value: bool);
    fn dtr(&self) -> bool;
    fn set_dtr(&self, value: bool);
    fn cts(&self) -> bool;
    fn dsr(&self) -> bool;
}

// --- Dummy implementations ---

pub struct DummyProvider;

impl OmniRigProvider for DummyProvider {
    fn create_rig1(&self) -> Box<dyn RigControl> {
        Box::new(DummyRig::new())
    }

    fn create_rig2(&self) -> Box<dyn RigControl> {
        Box::new(DummyRig::new())
    }
}

pub struct DummyRig {
    freq: RwLock<i32>,
    freq_a: RwLock<i32>,
    freq_b: RwLock<i32>,
    rit_offset: RwLock<i32>,
    pitch: RwLock<i32>,
    vfo: RwLock<RigParamX>,
    split: RwLock<RigParamX>,
    rit: RwLock<RigParamX>,
    xit: RwLock<RigParamX>,
    tx: RwLock<RigParamX>,
    mode: RwLock<RigParamX>,
}

impl Default for DummyRig {
    fn default() -> Self {
        Self::new()
    }
}

impl DummyRig {
    pub fn new() -> Self {
        Self {
            freq: RwLock::new(0),
            freq_a: RwLock::new(0),
            freq_b: RwLock::new(0),
            rit_offset: RwLock::new(0),
            pitch: RwLock::new(0),
            vfo: RwLock::new(RigParamX::default()),
            split: RwLock::new(RigParamX::default()),
            rit: RwLock::new(RigParamX::default()),
            xit: RwLock::new(RigParamX::default()),
            tx: RwLock::new(RigParamX::default()),
            mode: RwLock::new(RigParamX::default()),
        }
    }
}

impl RigControl for DummyRig {
    fn rig_type(&self) -> String {
        "DummyRig".to_string()
    }

    fn status(&self) -> RigStatusX {
        RigStatusX::Online
    }

    fn status_str(&self) -> String {
        "online".to_string()
    }

    fn readable_params(&self) -> i32 {
        0xFFFFFFFFu32 as i32
    }

    fn writeable_params(&self) -> i32 {
        0xFFFFFFFFu32 as i32
    }

    fn freq(&self) -> i32 {
        *self.freq.read().unwrap()
    }

    fn set_freq(&self, value: i32) {
        *self.freq.write().unwrap() = value;
    }

    fn freq_a(&self) -> i32 {
        *self.freq_a.read().unwrap()
    }

    fn set_freq_a(&self, value: i32) {
        *self.freq_a.write().unwrap() = value;
    }

    fn freq_b(&self) -> i32 {
        *self.freq_b.read().unwrap()
    }

    fn set_freq_b(&self, value: i32) {
        *self.freq_b.write().unwrap() = value;
    }

    fn rit_offset(&self) -> i32 {
        *self.rit_offset.read().unwrap()
    }

    fn set_rit_offset(&self, value: i32) {
        *self.rit_offset.write().unwrap() = value;
    }

    fn pitch(&self) -> i32 {
        *self.pitch.read().unwrap()
    }

    fn set_pitch(&self, value: i32) {
        *self.pitch.write().unwrap() = value;
    }

    fn vfo(&self) -> RigParamX {
        *self.vfo.read().unwrap()
    }

    fn set_vfo(&self, value: RigParamX) {
        *self.vfo.write().unwrap() = value;
    }

    fn split(&self) -> RigParamX {
        *self.split.read().unwrap()
    }

    fn set_split(&self, value: RigParamX) {
        *self.split.write().unwrap() = value;
    }

    fn rit(&self) -> RigParamX {
        *self.rit.read().unwrap()
    }

    fn set_rit(&self, value: RigParamX) {
        *self.rit.write().unwrap() = value;
    }

    fn xit(&self) -> RigParamX {
        *self.xit.read().unwrap()
    }

    fn set_xit(&self, value: RigParamX) {
        *self.xit.write().unwrap() = value;
    }

    fn tx(&self) -> RigParamX {
        *self.tx.read().unwrap()
    }

    fn set_tx(&self, value: RigParamX) {
        *self.tx.write().unwrap() = value;
    }

    fn mode(&self) -> RigParamX {
        *self.mode.read().unwrap()
    }

    fn set_mode(&self, value: RigParamX) {
        *self.mode.write().unwrap() = value;
    }

    fn send_custom_command(&self, _command: &[u8], _reply_length: i32, _reply_end: &[u8]) {}

    fn port_bits(&self) -> Option<Box<dyn PortBitsControl>> {
        Some(Box::new(DummyPortBits::new()))
    }
}

pub struct DummyPortBits {
    rts: RwLock<bool>,
    dtr: RwLock<bool>,
    cts: RwLock<bool>,
    dsr: RwLock<bool>,
    locked: RwLock<bool>,
}

impl Default for DummyPortBits {
    fn default() -> Self {
        Self::new()
    }
}

impl DummyPortBits {
    pub fn new() -> Self {
        Self {
            rts: RwLock::new(false),
            dtr: RwLock::new(false),
            cts: RwLock::new(false),
            dsr: RwLock::new(false),
            locked: RwLock::new(false),
        }
    }
}

impl PortBitsControl for DummyPortBits {
    fn lock(&self) -> bool {
        *self.locked.write().unwrap() = true;
        true
    }

    fn unlock(&self) {
        *self.locked.write().unwrap() = false;
    }

    fn rts(&self) -> bool {
        *self.rts.read().unwrap()
    }

    fn set_rts(&self, value: bool) {
        *self.rts.write().unwrap() = value;
    }

    fn dtr(&self) -> bool {
        *self.dtr.read().unwrap()
    }

    fn set_dtr(&self, value: bool) {
        *self.dtr.write().unwrap() = value;
    }

    fn cts(&self) -> bool {
        *self.cts.read().unwrap()
    }

    fn dsr(&self) -> bool {
        *self.dsr.read().unwrap()
    }
}
