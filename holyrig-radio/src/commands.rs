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
