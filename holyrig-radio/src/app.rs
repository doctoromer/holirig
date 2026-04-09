use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText, Ui, Vec2};
use egui_dock::{AllowedSplits, DockArea, DockState, TabViewer};
use tokio::sync::mpsc::UnboundedSender;

use crate::commands::RadioCommand;
use crate::state::{RadioState, TuningStep, format_freq, format_freq_entry, parse_freq_input};

const AMBER: Color32 = Color32::from_rgb(255, 176, 0);
const DIM_AMBER: Color32 = Color32::from_rgb(130, 90, 0);
const TX_RED: Color32 = Color32::from_rgb(220, 50, 50);
const RX_GREEN: Color32 = Color32::from_rgb(60, 200, 60);
const PANEL_BG: Color32 = Color32::from_rgb(18, 18, 12);
const ACTIVE_PANEL_BG: Color32 = Color32::from_rgb(25, 25, 15);
const TX_PANEL_BG: Color32 = Color32::from_rgb(35, 10, 10);

pub enum AppMessage {
    Connecting,
    Connected,
    ConnectionError(String),
    Disconnected,
    AddRig {
        rig_id: usize,
        state: Arc<Mutex<RadioState>>,
        cmd_tx: UnboundedSender<(usize, RadioCommand)>,
    },
}

pub struct RigTab {
    pub rig_id: usize,
    pub state: Arc<Mutex<RadioState>>,
    pub global_cmd_tx: UnboundedSender<(usize, RadioCommand)>,
    pub tuning_step: TuningStep,
    pub freq_input: Option<String>,
}

impl RigTab {
    pub fn new(
        rig_id: usize,
        state: Arc<Mutex<RadioState>>,
        global_cmd_tx: UnboundedSender<(usize, RadioCommand)>,
    ) -> Self {
        Self {
            rig_id,
            state,
            global_cmd_tx,
            tuning_step: TuningStep::KHz1,
            freq_input: None,
        }
    }

    pub fn send(&self, cmd: RadioCommand) {
        {
            let s = self.state.lock().unwrap();
            if !s.supports(cmd.rig_command_name()) {
                return;
            }
        }
        let _ = self.global_cmd_tx.send((self.rig_id, cmd));
    }
}

struct RadioTabViewer {
    active_rig_id: Option<usize>,
}

impl TabViewer for RadioTabViewer {
    type Tab = RigTab;

    fn title(&mut self, tab: &mut Self::Tab) -> egui::WidgetText {
        let s = tab.state.lock().unwrap();
        let dot = if s.connected { "●" } else { "○" };
        format!("Rig {}  {}", s.rig_id, dot).into()
    }

    fn ui(&mut self, ui: &mut Ui, tab: &mut Self::Tab) {
        let state = tab.state.lock().unwrap().clone();
        if self.active_rig_id == Some(tab.rig_id) {
            handle_keys(ui.ctx(), tab, &state);
        }
        draw_rig_panel(ui, tab, &state);
    }

    fn is_closeable(&self, _tab: &Self::Tab) -> bool {
        false
    }
}

enum ConnectionStatus {
    Connecting,
    Connected,
    Error(String),
}

pub struct RadioApp {
    dock_state: Option<DockState<RigTab>>,
    app_rx: std::sync::mpsc::Receiver<AppMessage>,
    connection_status: ConnectionStatus,
}

impl RadioApp {
    pub fn new(app_rx: std::sync::mpsc::Receiver<AppMessage>) -> Self {
        Self {
            dock_state: None,
            app_rx,
            connection_status: ConnectionStatus::Connecting,
        }
    }

    fn drain_messages(&mut self) {
        while let Ok(msg) = self.app_rx.try_recv() {
            match msg {
                AppMessage::Connecting => {
                    self.connection_status = ConnectionStatus::Connecting;
                }
                AppMessage::Connected => {
                    self.connection_status = ConnectionStatus::Connected;
                }
                AppMessage::ConnectionError(err) => {
                    self.connection_status = ConnectionStatus::Error(err);
                }
                AppMessage::Disconnected => {
                    self.connection_status = ConnectionStatus::Connecting;
                    if let Some(dock) = &self.dock_state {
                        for (_, tab) in dock.iter_all_tabs() {
                            tab.state.lock().unwrap().connected = false;
                        }
                    }
                }
                AppMessage::AddRig {
                    rig_id,
                    state,
                    cmd_tx,
                } => {
                    let tab = RigTab::new(rig_id, state.clone(), cmd_tx.clone());
                    match &mut self.dock_state {
                        None => {
                            self.dock_state = Some(DockState::new(vec![tab]));
                        }
                        Some(dock) => {
                            let mut updated = false;
                            for (_, t) in dock.iter_all_tabs_mut() {
                                if t.rig_id == rig_id {
                                    t.global_cmd_tx = cmd_tx.clone();
                                    t.state = state.clone();
                                    updated = true;
                                    break;
                                }
                            }
                            if !updated {
                                dock.main_surface_mut().push_to_first_leaf(tab);
                            }
                        }
                    }
                }
            }
        }
    }

    fn draw_no_rigs_screen(&self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.centered_and_justified(|ui| {
                ui.vertical_centered(|ui| match &self.connection_status {
                    ConnectionStatus::Connecting => {
                        ui.label(
                            RichText::new("Connecting to server…")
                                .size(18.0)
                                .color(DIM_AMBER),
                        );
                        ui.label(
                            RichText::new("Make sure holyrig is running.")
                                .size(13.0)
                                .color(Color32::GRAY),
                        );
                    }
                    ConnectionStatus::Error(err) => {
                        ui.label(RichText::new(format!("⚠  {err}")).size(16.0).color(TX_RED));
                        ui.label(
                            RichText::new("Retrying in a moment…")
                                .size(13.0)
                                .color(Color32::GRAY),
                        );
                    }
                    ConnectionStatus::Connected => {
                        ui.label(
                            RichText::new("Connected — no rigs configured.")
                                .size(16.0)
                                .color(DIM_AMBER),
                        );
                    }
                });
            });
        });
    }
}

impl eframe::App for RadioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        ctx.set_visuals(egui::Visuals::dark());
        ctx.request_repaint_after(Duration::from_millis(50));

        self.drain_messages();

        if self.dock_state.is_none() {
            self.draw_no_rigs_screen(ctx);
            return;
        }

        let active_rig_id = self
            .dock_state
            .as_mut()
            .and_then(|d| d.find_active_focused())
            .map(|(_, tab)| tab.rig_id);

        let mut tab_viewer = RadioTabViewer { active_rig_id };

        egui::CentralPanel::default().show(ctx, |ui| {
            DockArea::new(self.dock_state.as_mut().unwrap())
                .show_add_buttons(false)
                .show_close_buttons(false)
                .draggable_tabs(true)
                .allowed_splits(AllowedSplits::None)
                .show_leaf_close_all_buttons(false)
                .show_leaf_collapse_buttons(false)
                .tab_context_menus(false)
                .show_inside(ui, &mut tab_viewer);
        });
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(dock) = &self.dock_state {
            for (_, tab) in dock.iter_all_tabs() {
                if tab.state.lock().unwrap().transmitting {
                    let _ = tab
                        .global_cmd_tx
                        .send((tab.rig_id, RadioCommand::Transmit(false)));
                }
            }
        }
    }
}

fn draw_rig_panel(ui: &mut Ui, tab: &mut RigTab, state: &RadioState) {
    ui.spacing_mut().item_spacing = Vec2::new(6.0, 8.0);
    ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);

    draw_vfo_a(ui, tab, state);
    ui.add_space(2.0);
    draw_vfo_b(ui, state);
    ui.add_space(4.0);
    draw_vfo_controls(ui, tab, state);
    ui.separator();
    draw_step_selector(ui, tab);
    ui.separator();
    draw_mode_buttons(ui, tab, state);

    if state.has_status("rit") || state.has_status("rit_offset") {
        ui.separator();
        draw_rit_xit(ui, tab, state);
    }

    if !state.connected {
        ui.separator();
        ui.colored_label(TX_RED, "⚠  Rig disconnected — reconnecting…");
    }
}

fn draw_vfo_a(ui: &mut Ui, tab: &mut RigTab, state: &RadioState) {
    let is_tx = state.transmitting;
    let bg = if is_tx { TX_PANEL_BG } else { ACTIVE_PANEL_BG };
    let text_color = if is_tx { TX_RED } else { AMBER };

    let freq_str = match &tab.freq_input {
        Some(buf) => format_freq_entry(buf),
        None => state
            .freq_a
            .map(format_freq)
            .unwrap_or_else(|| "---.---.---".into()),
    };

    egui::Frame::default()
        .fill(bg)
        .inner_margin(12.0)
        .corner_radius(6.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                let resp = ui.add(
                    egui::Label::new(
                        RichText::new(&freq_str)
                            .color(text_color)
                            .size(38.0)
                            .monospace(),
                    )
                    .sense(egui::Sense::click()),
                );
                if resp.clicked() && tab.freq_input.is_none() {
                    tab.freq_input = Some(String::new());
                }
                resp.on_hover_text("Click or type digits to enter frequency");

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (tx_str, tx_col) = if is_tx {
                        ("■ TX", TX_RED)
                    } else {
                        ("■ RX", RX_GREEN)
                    };
                    ui.label(RichText::new(tx_str).color(tx_col).size(13.0));
                    if let Some(mode) = &state.mode {
                        ui.label(RichText::new(mode.as_str()).color(DIM_AMBER).size(15.0));
                    }
                    let vfo_label = match state.vfo.as_deref() {
                        Some("B") => "VFO-B",
                        _ => "VFO-A",
                    };
                    ui.label(RichText::new(vfo_label).color(DIM_AMBER).size(12.0));
                });
            });
        });
}

fn draw_vfo_b(ui: &mut Ui, state: &RadioState) {
    let freq_str = state
        .freq_b
        .map(format_freq)
        .unwrap_or_else(|| "---.---.---".into());

    egui::Frame::default()
        .fill(PANEL_BG)
        .inner_margin(egui::Margin {
            left: 12,
            right: 12,
            top: 6,
            bottom: 6,
        })
        .corner_radius(4.0)
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new("VFO-B").color(DIM_AMBER).size(11.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(mode) = &state.mode {
                        ui.label(RichText::new(mode.as_str()).color(DIM_AMBER).size(12.0));
                    }
                    ui.label(
                        RichText::new(&freq_str)
                            .color(DIM_AMBER)
                            .size(22.0)
                            .monospace(),
                    );
                });
            });
        });
}

fn draw_vfo_controls(ui: &mut Ui, tab: &mut RigTab, state: &RadioState) {
    ui.horizontal(|ui| {
        let active_is_a = state.vfo.as_deref() != Some("B");
        if ui.selectable_label(active_is_a, "VFO A").clicked() && !active_is_a {
            tab.send(RadioCommand::SetVfo {
                rx: "A".into(),
                tx: "A".into(),
            });
        }
        if ui.selectable_label(!active_is_a, "VFO B").clicked() && active_is_a {
            tab.send(RadioCommand::SetVfo {
                rx: "B".into(),
                tx: "B".into(),
            });
        }
        if ui.button("Swap").clicked() {
            tab.send(RadioCommand::VfoSwap);
        }
        if ui.button("Equal").clicked() {
            tab.send(RadioCommand::VfoEqual);
        }

        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let (ptt_str, ptt_col) = if state.transmitting {
                ("● PTT  ", TX_RED)
            } else {
                ("○ PTT  ", Color32::GRAY)
            };
            if ui.button(RichText::new(ptt_str).color(ptt_col)).clicked() {
                tab.send(RadioCommand::Transmit(!state.transmitting));
            }
            if state.split {
                ui.label(RichText::new("SPLIT").color(AMBER).size(11.0));
            }
        });
    });
}

fn draw_step_selector(ui: &mut Ui, tab: &mut RigTab) {
    ui.horizontal(|ui| {
        ui.label("Step:");
        for &step in TuningStep::all() {
            if ui
                .selectable_label(tab.tuning_step == step, step.label())
                .clicked()
            {
                tab.tuning_step = step;
            }
        }
    });
}

fn draw_mode_buttons(ui: &mut Ui, tab: &mut RigTab, state: &RadioState) {
    let modes = ["LSB", "USB", "CWU", "CWL", "AM", "FM", "DIGIU", "DIGIL"];
    ui.horizontal(|ui| {
        ui.label("Mode:");
        for mode in modes {
            let active = state.mode.as_deref() == Some(mode);
            if ui.selectable_label(active, mode).clicked() && !active {
                tab.send(RadioCommand::SetMode(mode.into()));
            }
        }
    });
}

fn draw_rit_xit(ui: &mut Ui, tab: &mut RigTab, state: &RadioState) {
    ui.horizontal(|ui| {
        if state.has_status("rit") {
            let rit_str = if state.rit { "RIT [ON] " } else { "RIT [OFF]" };
            let rit_col = if state.rit { AMBER } else { Color32::GRAY };
            if ui.button(RichText::new(rit_str).color(rit_col)).clicked() {
                tab.send(RadioCommand::SetRit(!state.rit));
            }
        }
        if state.has_status("xit") {
            let xit_str = if state.xit { "XIT [ON] " } else { "XIT [OFF]" };
            let xit_col = if state.xit { AMBER } else { Color32::GRAY };
            if ui.button(RichText::new(xit_str).color(xit_col)).clicked() {
                tab.send(RadioCommand::SetXit(!state.xit));
            }
        }
        if let Some(offset) = state.rit_offset {
            ui.label(format!("Δ {:+} Hz", offset));
            if ui.small_button("+10").clicked() {
                tab.send(RadioCommand::RitOffset(offset + 10));
            }
            if ui.small_button("−10").clicked() {
                tab.send(RadioCommand::RitOffset(offset - 10));
            }
        }
    });
}

fn handle_keys(ctx: &egui::Context, tab: &mut RigTab, state: &RadioState) {
    if let Some(freq_input) = &mut tab.freq_input {
        let text_events: Vec<String> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|err| {
                    if let egui::Event::Text(t) = err {
                        Some(t.clone())
                    } else {
                        None
                    }
                })
                .collect()
        });
        let key_events: Vec<egui::Key> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|err| {
                    if let egui::Event::Key {
                        key, pressed: true, ..
                    } = err
                    {
                        Some(*key)
                    } else {
                        None
                    }
                })
                .collect()
        });

        {
            for t in &text_events {
                for c in t.chars() {
                    if c.is_ascii_digit() || c == '.' {
                        freq_input.push(c);
                    }
                }
            }
            if key_events.contains(&egui::Key::Backspace) {
                freq_input.pop();
            }
        }

        if key_events.contains(&egui::Key::Escape) {
            tab.freq_input = None;
        } else if key_events.contains(&egui::Key::Enter) {
            let buf = tab.freq_input.take().unwrap();
            if let Some(freq) = parse_freq_input(&buf) {
                let vfo = state.vfo.clone().unwrap_or_else(|| "A".into());
                tab.send(RadioCommand::SetFreq { freq, vfo });
            }
        }
        return;
    }

    let step_hz = tab.tuning_step.hz_value();
    let active_vfo = state.vfo.clone().unwrap_or_else(|| "A".into());

    let (freq_delta, step_delta, ptt_toggle) = {
        let mut fd = 0i64;
        let mut sd = 0i32;
        let mut ptt = false;
        ctx.input_mut(|i| {
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) {
                fd += step_hz;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown) {
                fd -= step_hz;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight) {
                sd += 1;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft) {
                sd -= 1;
            }
            if i.consume_key(egui::Modifiers::NONE, egui::Key::Space) {
                ptt = true;
            }
        });
        (fd, sd, ptt)
    };

    match step_delta.cmp(&0) {
        std::cmp::Ordering::Greater => tab.tuning_step = tab.tuning_step.step_up(),
        std::cmp::Ordering::Less => tab.tuning_step = tab.tuning_step.step_down(),
        std::cmp::Ordering::Equal => {}
    }

    if ptt_toggle {
        tab.send(RadioCommand::Transmit(!state.transmitting));
    }

    if freq_delta != 0
        && let Some(freq) = state.active_freq()
    {
        let new_freq = (freq + freq_delta).clamp(1, 9_999_999_999);
        tab.send(RadioCommand::SetFreq {
            freq: new_freq,
            vfo: active_vfo.clone(),
        });
    }

    let scroll_y = ctx.input(|i| i.smooth_scroll_delta.y);
    if scroll_y.abs() >= 1.0
        && let Some(freq) = state.active_freq()
    {
        let steps = (scroll_y / 10.0).round() as i64;
        if steps != 0 {
            let new_freq = (freq + steps * step_hz).clamp(1, 9_999_999_999);
            tab.send(RadioCommand::SetFreq {
                freq: new_freq,
                vfo: active_vfo.clone(),
            });
        }
    }

    let text_events: Vec<String> = ctx.input(|i| {
        i.events
            .iter()
            .filter_map(|err| {
                if let egui::Event::Text(t) = err {
                    Some(t.clone())
                } else {
                    None
                }
            })
            .collect()
    });

    for t in text_events {
        match t.as_str() {
            "u" => tab.send(RadioCommand::SetMode("USB".into())),
            "l" => tab.send(RadioCommand::SetMode("LSB".into())),
            "c" => tab.send(RadioCommand::SetMode("CWU".into())),
            "C" => tab.send(RadioCommand::SetMode("CWL".into())),
            "a" => tab.send(RadioCommand::SetMode("AM".into())),
            "f" => tab.send(RadioCommand::SetMode("FM".into())),
            "d" => tab.send(RadioCommand::SetMode("DIGIU".into())),
            "D" => tab.send(RadioCommand::SetMode("DIGIL".into())),
            "v" => {
                let new_vfo = if active_vfo == "A" { "B" } else { "A" };
                tab.send(RadioCommand::SetVfo {
                    rx: new_vfo.into(),
                    tx: new_vfo.into(),
                });
            }
            "s" => tab.send(RadioCommand::VfoSwap),
            "err" => tab.send(RadioCommand::VfoEqual),
            "r" => tab.send(RadioCommand::SetRit(!state.rit)),
            "x" => tab.send(RadioCommand::SetXit(!state.xit)),
            "+" => tab.send(RadioCommand::RitOffset(state.rit_offset.unwrap_or(0) + 10)),
            "-" => tab.send(RadioCommand::RitOffset(state.rit_offset.unwrap_or(0) - 10)),
            digit
                if digit
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false) =>
            {
                tab.freq_input = Some(t.clone());
            }
            _ => {}
        }
    }
}
