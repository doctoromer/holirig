use std::sync::{Arc, Mutex};
use std::time::Duration;

use egui::{Color32, RichText, Ui, Vec2};
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

pub struct RadioApp {
    state: Arc<Mutex<RadioState>>,
    cmd_tx: UnboundedSender<RadioCommand>,
    tuning_step: TuningStep,
    freq_input: Option<String>,
}

impl RadioApp {
    pub fn new(state: Arc<Mutex<RadioState>>, cmd_tx: UnboundedSender<RadioCommand>) -> Self {
        Self {
            state,
            cmd_tx,
            tuning_step: TuningStep::KHz1,
            freq_input: None,
        }
    }

    fn send(&self, cmd: RadioCommand) {
        let _ = self.cmd_tx.send(cmd);
    }

    fn draw_panel(&mut self, ctx: &egui::Context, state: &RadioState) {
        ctx.set_visuals(egui::Visuals::dark());

        let title = if state.connected {
            format!("HolyRig Radio  —  Rig {}  [CONNECTED]", state.rig_id)
        } else {
            format!("HolyRig Radio  —  Rig {}  [DISCONNECTED]", state.rig_id)
        };
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.spacing_mut().item_spacing = Vec2::new(6.0, 8.0);
            ui.spacing_mut().button_padding = Vec2::new(8.0, 4.0);

            self.draw_vfo_a(ui, state);
            ui.add_space(2.0);
            self.draw_vfo_b(ui, state);
            ui.add_space(4.0);

            self.draw_vfo_controls(ui, state);
            ui.separator();
            self.draw_step_selector(ui);
            ui.separator();
            self.draw_mode_buttons(ui, state);

            if state.has_status("rit") || state.has_status("rit_offset") {
                ui.separator();
                self.draw_rit_xit(ui, state);
            }

            if !state.connected {
                ui.separator();
                ui.colored_label(TX_RED, "⚠  Rig disconnected — waiting for reconnect");
            }
        });
    }

    fn draw_vfo_a(&mut self, ui: &mut Ui, state: &RadioState) {
        let is_tx = state.transmitting;
        let bg = if is_tx { TX_PANEL_BG } else { ACTIVE_PANEL_BG };
        let text_color = if is_tx { TX_RED } else { AMBER };

        let freq_str = match &self.freq_input {
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
                    let label_resp = ui.add(
                        egui::Label::new(
                            RichText::new(&freq_str)
                                .color(text_color)
                                .size(38.0)
                                .monospace(),
                        )
                        .sense(egui::Sense::click()),
                    );
                    if label_resp.clicked() && self.freq_input.is_none() {
                        self.freq_input = Some(String::new());
                    }
                    label_resp.on_hover_text("Click or type digits to enter frequency");

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

    fn draw_vfo_b(&mut self, ui: &mut Ui, state: &RadioState) {
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

    fn draw_vfo_controls(&mut self, ui: &mut Ui, state: &RadioState) {
        ui.horizontal(|ui| {
            let active_is_a = state.vfo.as_deref() != Some("B");

            if ui.selectable_label(active_is_a, "VFO A").clicked() && !active_is_a {
                self.send(RadioCommand::SetVfo {
                    rx: "A".into(),
                    tx: "A".into(),
                });
            }
            if ui.selectable_label(!active_is_a, "VFO B").clicked() && active_is_a {
                self.send(RadioCommand::SetVfo {
                    rx: "B".into(),
                    tx: "B".into(),
                });
            }
            if ui.button("Swap").clicked() {
                self.send(RadioCommand::VfoSwap);
            }
            if ui.button("Equal").clicked() {
                self.send(RadioCommand::VfoEqual);
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (ptt_str, ptt_col) = if state.transmitting {
                    ("● PTT  ", TX_RED)
                } else {
                    ("○ PTT  ", Color32::GRAY)
                };
                if ui.button(RichText::new(ptt_str).color(ptt_col)).clicked() {
                    self.send(RadioCommand::Transmit(!state.transmitting));
                }
                if state.split {
                    ui.label(RichText::new("SPLIT").color(AMBER).size(11.0));
                }
            });
        });
    }

    fn draw_step_selector(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.label("Step:");
            for &step in TuningStep::all() {
                if ui
                    .selectable_label(self.tuning_step == step, step.label())
                    .clicked()
                {
                    self.tuning_step = step;
                }
            }
        });
    }

    fn draw_mode_buttons(&mut self, ui: &mut Ui, state: &RadioState) {
        let modes = ["LSB", "USB", "CWU", "CWL", "AM", "FM", "DIGIU", "DIGIL"];
        ui.horizontal(|ui| {
            ui.label("Mode:");
            for mode in modes {
                let active = state.mode.as_deref() == Some(mode);
                if ui.selectable_label(active, mode).clicked() && !active {
                    self.send(RadioCommand::SetMode(mode.to_string()));
                }
            }
        });
    }

    fn draw_rit_xit(&mut self, ui: &mut Ui, state: &RadioState) {
        ui.horizontal(|ui| {
            if state.has_status("rit") {
                let rit_str = if state.rit { "RIT [ON] " } else { "RIT [OFF]" };
                let rit_col = if state.rit { AMBER } else { Color32::GRAY };
                if ui.button(RichText::new(rit_str).color(rit_col)).clicked() {
                    self.send(RadioCommand::SetRit(!state.rit));
                }
            }

            if state.has_status("xit") {
                let xit_str = if state.xit { "XIT [ON] " } else { "XIT [OFF]" };
                let xit_col = if state.xit { AMBER } else { Color32::GRAY };
                if ui.button(RichText::new(xit_str).color(xit_col)).clicked() {
                    self.send(RadioCommand::SetXit(!state.xit));
                }
            }

            if let Some(offset) = state.rit_offset {
                ui.label(format!("Δ {:+} Hz", offset));
                if ui.small_button("+10").clicked() {
                    self.send(RadioCommand::RitOffset(offset + 10));
                }
                if ui.small_button("−10").clicked() {
                    self.send(RadioCommand::RitOffset(offset - 10));
                }
            }
        });
    }

    fn handle_keys(&mut self, ctx: &egui::Context, state: &RadioState) {
        if let Some(freq_input) = &mut self.freq_input {
            let text_events: Vec<String> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| {
                        if let egui::Event::Text(t) = e {
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
                    .filter_map(|e| {
                        if let egui::Event::Key {
                            key, pressed: true, ..
                        } = e
                        {
                            Some(*key)
                        } else {
                            None
                        }
                    })
                    .collect()
            });

            // Append typed digits / decimal point
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
                self.freq_input = None;
            } else if key_events.contains(&egui::Key::Enter) {
                let buf = self.freq_input.take().unwrap();
                if let Some(freq) = parse_freq_input(&buf) {
                    let vfo = state.vfo.clone().unwrap_or_else(|| "A".into());
                    self.send(RadioCommand::SetFreq { freq, vfo });
                }
            }
            return;
        }

        let step_hz = self.tuning_step.hz_value();
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
            std::cmp::Ordering::Greater => self.tuning_step = self.tuning_step.step_up(),
            std::cmp::Ordering::Less => self.tuning_step = self.tuning_step.step_down(),
            std::cmp::Ordering::Equal => {}
        }

        if ptt_toggle {
            self.send(RadioCommand::Transmit(!state.transmitting));
        }

        if freq_delta != 0
            && let Some(freq) = state.active_freq()
        {
            let new_freq = (freq + freq_delta).clamp(1, 9_999_999_999);
            self.send(RadioCommand::SetFreq {
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
                self.send(RadioCommand::SetFreq {
                    freq: new_freq,
                    vfo: active_vfo.clone(),
                });
            }
        }

        let text_events: Vec<String> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| {
                    if let egui::Event::Text(t) = e {
                        Some(t.clone())
                    } else {
                        None
                    }
                })
                .collect()
        });

        for key in text_events {
            match key.as_str() {
                "u" => self.send(RadioCommand::SetMode("USB".into())),
                "l" => self.send(RadioCommand::SetMode("LSB".into())),
                "c" => self.send(RadioCommand::SetMode("CWU".into())),
                "C" => self.send(RadioCommand::SetMode("CWL".into())),
                "a" => self.send(RadioCommand::SetMode("AM".into())),
                "f" => self.send(RadioCommand::SetMode("FM".into())),
                "d" => self.send(RadioCommand::SetMode("DIGIU".into())),
                "D" => self.send(RadioCommand::SetMode("DIGIL".into())),
                "v" => {
                    let new_vfo = if active_vfo == "A" { "B" } else { "A" };
                    self.send(RadioCommand::SetVfo {
                        rx: new_vfo.into(),
                        tx: new_vfo.into(),
                    });
                }
                "s" => self.send(RadioCommand::VfoSwap),
                "e" => self.send(RadioCommand::VfoEqual),
                "r" => self.send(RadioCommand::SetRit(!state.rit)),
                "x" => self.send(RadioCommand::SetXit(!state.xit)),
                "+" => {
                    let offset = state.rit_offset.unwrap_or(0);
                    self.send(RadioCommand::RitOffset(offset + 10));
                }
                "-" => {
                    let offset = state.rit_offset.unwrap_or(0);
                    self.send(RadioCommand::RitOffset(offset - 10));
                }
                digit if digit
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false) =>
                {
                    self.freq_input = Some(key.clone());
                }
                _ => {}
            }
        }
    }
}

impl eframe::App for RadioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let state = self.state.lock().unwrap().clone();
        self.handle_keys(ctx, &state);
        self.draw_panel(ctx, &state);
        ctx.request_repaint_after(Duration::from_millis(50));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        let transmitting = self.state.lock().unwrap().transmitting;
        if transmitting {
            let _ = self.cmd_tx.send(RadioCommand::Transmit(false));
        }
    }
}
