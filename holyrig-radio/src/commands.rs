pub enum RadioCommand {
    SetFreq { freq: i64, vfo: String },
    SetMode(String),
    SetVfo { rx: String, tx: String },
    VfoSwap,
    VfoEqual,
    Transmit(bool),
    SetSplit(bool),
    SetRit(bool),
    SetXit(bool),
    RitOffset(i64),
    ClearRit,
}

impl RadioCommand {
    /// Returns the JSON-RPC command name for capabilities lookup.
    pub fn rig_command_name(&self) -> &'static str {
        match self {
            RadioCommand::SetFreq { .. } => "set_freq",
            RadioCommand::SetMode(_) => "set_mode",
            RadioCommand::SetVfo { .. } => "set_vfo",
            RadioCommand::VfoSwap => "vfo_swap",
            RadioCommand::VfoEqual => "vfo_equal",
            RadioCommand::Transmit(_) => "transmit",
            RadioCommand::SetSplit(_) => "set_split",
            RadioCommand::SetRit(_) => "set_rit",
            RadioCommand::SetXit(_) => "set_xit",
            RadioCommand::RitOffset(_) => "rit_offset",
            RadioCommand::ClearRit => "clear_rit",
        }
    }
}
