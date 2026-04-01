use serde::{Deserialize, Serialize};
use std::fmt::Display;

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct RigId(usize);

impl std::fmt::Display for RigId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BaudRate {
    #[default]
    Baud1200,
    Baud2400,
    Baud4800,
    Baud9600,
    Baud19200,
    Baud38400,
    Baud57600,
    Baud115200,
}

impl Display for BaudRate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            BaudRate::Baud1200 => "1200",
            BaudRate::Baud2400 => "2400",
            BaudRate::Baud4800 => "4800",
            BaudRate::Baud9600 => "9600",
            BaudRate::Baud19200 => "19200",
            BaudRate::Baud38400 => "38400",
            BaudRate::Baud57600 => "57600",
            BaudRate::Baud115200 => "115200",
        };
        write!(f, "{result}")
    }
}

impl BaudRate {
    pub fn iter_rates() -> impl Iterator<Item = BaudRate> {
        [
            BaudRate::Baud1200,
            BaudRate::Baud2400,
            BaudRate::Baud4800,
            BaudRate::Baud9600,
            BaudRate::Baud19200,
            BaudRate::Baud38400,
            BaudRate::Baud57600,
            BaudRate::Baud115200,
        ]
        .into_iter()
    }
}

impl From<BaudRate> for u32 {
    fn from(value: BaudRate) -> Self {
        match value {
            BaudRate::Baud1200 => 1200,
            BaudRate::Baud2400 => 2400,
            BaudRate::Baud4800 => 4800,
            BaudRate::Baud9600 => 9600,
            BaudRate::Baud19200 => 19200,
            BaudRate::Baud38400 => 38400,
            BaudRate::Baud57600 => 57600,
            BaudRate::Baud115200 => 115200,
        }
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DataBits {
    Bits5,
    Bits6,
    Bits7,
    #[default]
    Bits8,
}

impl Display for DataBits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            DataBits::Bits5 => "5",
            DataBits::Bits6 => "6",
            DataBits::Bits7 => "7",
            DataBits::Bits8 => "8",
        };
        write!(f, "{result}")
    }
}

impl DataBits {
    pub fn iter_data_bits() -> impl Iterator<Item = DataBits> {
        [
            DataBits::Bits5,
            DataBits::Bits6,
            DataBits::Bits7,
            DataBits::Bits8,
        ]
        .into_iter()
    }
}

#[derive(Debug, Copy, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StopBits {
    #[default]
    Bits1,
    Bits2,
}

impl Display for StopBits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let result = match self {
            StopBits::Bits1 => "1",
            StopBits::Bits2 => "2",
        };
        write!(f, "{result}")
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RigConfig {
    #[serde(default = "default_rig_type")]
    pub rig_type: String,
    pub port: String,
    pub baud_rate: BaudRate,
    pub data_bits: DataBits,
    pub parity: bool,
    pub stop_bits: StopBits,
    // true is high, false is low
    pub rts: bool,
    pub dtr: bool,
    #[serde(default = "default_poll_interval")]
    pub poll_interval: u16,
    #[serde(default = "default_timeout")]
    pub timeout: u16,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RigSettings {
    pub id: RigId,
    #[serde(flatten)]
    pub config: RigConfig,
}

fn default_rig_type() -> String {
    "unspecified".to_string()
}

fn default_poll_interval() -> u16 {
    500
}

fn default_timeout() -> u16 {
    1000
}

impl RigConfig {
    pub fn validate(&self) -> Result<(), String> {
        if self.rig_type == "unspecified" {
            return Err("Rig type must be specified".to_string());
        }

        if self.port.is_empty() {
            return Err("Serial port must be specified".to_string());
        }

        if !(100..=5000).contains(&self.poll_interval) {
            return Err("Poll interval must be between 100ms and 5000ms".to_string());
        }

        if !(100..=10000).contains(&self.timeout) {
            return Err("Timeout must be between 100ms and 10000ms".to_string());
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct Settings {
    rigs: Vec<RigSettings>,
    #[serde(skip)]
    next_id: usize,
}

impl Settings {
    fn compute_next_id(rigs: &[RigSettings]) -> usize {
        rigs.iter().map(|r| r.id.0).max().map_or(0, |m| m + 1)
    }

    pub fn get_rig(&self, id: RigId) -> Option<&RigSettings> {
        self.rigs.iter().find(|r| r.id == id)
    }

    pub fn get_rig_mut(&mut self, id: RigId) -> Option<&mut RigSettings> {
        self.rigs.iter_mut().find(|r| r.id == id)
    }

    pub fn add_rig(&mut self, config: RigConfig) -> RigSettings {
        let id = RigId(self.next_id);
        self.next_id += 1;
        let settings = RigSettings { id, config };
        self.rigs.push(settings.clone());
        settings
    }

    pub fn remove_rig(&mut self, id: RigId) -> Option<RigSettings> {
        let pos = self.rigs.iter().position(|r| r.id == id)?;
        Some(self.rigs.remove(pos))
    }

    pub fn rigs(&self) -> impl Iterator<Item = &RigSettings> {
        self.rigs.iter()
    }
}

impl<'de> Deserialize<'de> for Settings {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Raw {
            rigs: Vec<RigSettings>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let next_id = Settings::compute_next_id(&raw.rigs);
        Ok(Settings {
            rigs: raw.rigs,
            next_id,
        })
    }
}
