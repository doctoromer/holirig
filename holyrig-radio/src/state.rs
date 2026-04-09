use std::collections::HashMap;

use serde_json::Value;

use holyrig_client::capabilities::Capabilities;

#[derive(Clone, Copy, PartialEq)]
pub enum TuningStep {
    Hz1,
    Hz10,
    Hz100,
    KHz1,
    KHz5,
    KHz10,
    KHz100,
    KHz500,
    MHz1,
}

impl TuningStep {
    pub fn all() -> &'static [TuningStep] {
        &[
            TuningStep::Hz1,
            TuningStep::Hz10,
            TuningStep::Hz100,
            TuningStep::KHz1,
            TuningStep::KHz5,
            TuningStep::KHz10,
            TuningStep::KHz100,
            TuningStep::KHz500,
            TuningStep::MHz1,
        ]
    }

    pub fn hz_value(self) -> i64 {
        match self {
            TuningStep::Hz1 => 1,
            TuningStep::Hz10 => 10,
            TuningStep::Hz100 => 100,
            TuningStep::KHz1 => 1_000,
            TuningStep::KHz5 => 5_000,
            TuningStep::KHz10 => 10_000,
            TuningStep::KHz100 => 100_000,
            TuningStep::KHz500 => 500_000,
            TuningStep::MHz1 => 1_000_000,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            TuningStep::Hz1 => "1 Hz",
            TuningStep::Hz10 => "10 Hz",
            TuningStep::Hz100 => "100 Hz",
            TuningStep::KHz1 => "1 kHz",
            TuningStep::KHz5 => "5 kHz",
            TuningStep::KHz10 => "10 kHz",
            TuningStep::KHz100 => "100 kHz",
            TuningStep::KHz500 => "500 kHz",
            TuningStep::MHz1 => "1 MHz",
        }
    }

    pub fn step_up(self) -> Self {
        let all = Self::all();
        let pos = all.iter().position(|s| *s == self).unwrap_or(0);
        all.get(pos + 1).copied().unwrap_or(self)
    }

    pub fn step_down(self) -> Self {
        let all = Self::all();
        let pos = all.iter().position(|s| *s == self).unwrap_or(0);
        if pos == 0 { self } else { all[pos - 1] }
    }
}

#[derive(Clone)]
pub struct RadioState {
    pub rig_id: usize,
    pub connected: bool,
    pub capabilities: Capabilities,
    pub freq_a: Option<i64>,
    pub freq_b: Option<i64>,
    pub mode: Option<String>,
    pub vfo: Option<String>,
    pub transmitting: bool,
    pub split: bool,
    pub rit: bool,
    pub xit: bool,
    pub rit_offset: Option<i64>,
    pub cw_pitch: Option<i64>,
}

impl RadioState {
    pub fn new(rig_id: usize, capabilities: Capabilities) -> Self {
        Self {
            rig_id,
            connected: false,
            capabilities,
            freq_a: None,
            freq_b: None,
            mode: None,
            vfo: None,
            transmitting: false,
            split: false,
            rit: false,
            xit: false,
            rit_offset: None,
            cw_pitch: None,
        }
    }

    pub fn apply_updates(&mut self, updates: HashMap<String, Value>) {
        for (k, v) in updates {
            match k.as_str() {
                "freq_a" => self.freq_a = v.as_i64(),
                "freq_b" => self.freq_b = v.as_i64(),
                "mode" => self.mode = v.as_str().map(|s| s.to_string()),
                "vfo" => self.vfo = v.as_str().map(|s| s.to_string()),
                "transmit" => self.transmitting = v.as_bool().unwrap_or(false),
                "split" => self.split = v.as_bool().unwrap_or(false),
                "rit" => self.rit = v.as_bool().unwrap_or(false),
                "xit" => self.xit = v.as_bool().unwrap_or(false),
                "rit_offset" => self.rit_offset = v.as_i64(),
                "cw_pitch" => self.cw_pitch = v.as_i64(),
                _ => {}
            }
        }
    }

    pub fn active_freq(&self) -> Option<i64> {
        match self.vfo.as_deref() {
            Some("B") => self.freq_b,
            _ => self.freq_a,
        }
    }

    pub fn supports(&self, cmd: &str) -> bool {
        self.capabilities.commands.contains_key(cmd)
    }

    pub fn has_status(&self, field: &str) -> bool {
        self.capabilities.status_fields.contains_key(field)
    }
}

pub fn format_freq(hz: i64) -> String {
    let mhz = hz / 1_000_000;
    let khz = (hz % 1_000_000) / 1_000;
    let hz_rem = hz % 1_000;
    format!("{}.{:03}.{:03}", mhz, khz, hz_rem)
}

pub fn format_freq_entry(buf: &str) -> String {
    if buf.is_empty() {
        return "[ enter freq ]".to_string();
    }
    if let Ok(hz) = buf.parse::<i64>() {
        return format!("{} ▌", format_freq(hz));
    }
    format!("{}_", buf)
}

pub fn parse_freq_input(buf: &str) -> Option<i64> {
    let hz = if buf.contains('.') {
        let mhz: f64 = buf.parse().ok()?;
        (mhz * 1_000_000.0).round() as i64
    } else {
        buf.parse::<i64>().ok()?
    };
    if (1..=9_999_999_999).contains(&hz) {
        Some(hz)
    } else {
        None
    }
}
