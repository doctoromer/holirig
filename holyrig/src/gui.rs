use crate::{
    rig_settings::{BaudRate, DataBits, RigConfig, RigId, RigSettings, StopBits},
    serial::ManagerCommand,
    serial::manager::{ConnectionStatus, ManagerMessage, SerialPortEntry},
};
use eframe::egui;
use egui::{ComboBox, Grid, Ui};
use egui_dock::{
    AllowedSplits, DockArea, DockState, NodeIndex, SurfaceIndex, TabViewer,
    tab_viewer::OnCloseResponse,
};
use tokio::sync::broadcast;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
struct TabId(u64);

fn next_tab_id(counter: &mut u64) -> TabId {
    let id = TabId(*counter);
    *counter += 1;
    id
}

enum RigTabState {
    Draft,
    Registered {
        rig_id: RigId,
        status: ConnectionStatus,
    },
}

struct RigTab {
    tab_id: TabId,
    state: RigTabState,
    config: RigConfig,
}

struct AppTabViewer<'a> {
    current_index: usize,
    add_tab_request: bool,
    rig_types: Vec<String>,
    available_ports: Vec<SerialPortEntry>,
    sender: Sender<ManagerCommand>,
    error_message: Option<String>,
    active_tab_id: Option<TabId>,
    pending_creates: &'a mut Vec<(TabId, oneshot::Receiver<RigId>)>,
}

impl<'a> AppTabViewer<'a> {
    fn new(
        sender: Sender<ManagerCommand>,
        rig_types: Vec<String>,
        available_ports: Vec<SerialPortEntry>,
        active_tab_id: Option<TabId>,
        pending_creates: &'a mut Vec<(TabId, oneshot::Receiver<RigId>)>,
    ) -> Self {
        AppTabViewer {
            current_index: 0,
            add_tab_request: false,
            rig_types,
            available_ports,
            sender,
            error_message: None,
            active_tab_id,
            pending_creates,
        }
    }
}

impl<'a> TabViewer for AppTabViewer<'a> {
    type Tab = RigTab;

    fn title(&mut self, _tab: &mut Self::Tab) -> egui::WidgetText {
        self.current_index += 1;
        format!("RIG {}", self.current_index).as_str().into()
    }

    fn ui(&mut self, ui: &mut egui::Ui, tab: &mut Self::Tab) {
        let config = &mut tab.config;

        ui.group(|ui| {
            if let Some(error) = &self.error_message {
                ui.colored_label(egui::Color32::RED, error);
                ui.separator();
            }

            ui.style_mut().spacing.combo_width *= 0.75;

            Grid::new("rig_settings").num_columns(2).show(ui, |ui| {
                ui.label("Rig type:");
                ComboBox::from_id_salt("rig_type")
                    .selected_text(config.rig_type.to_string())
                    .show_ui(ui, |ui| {
                        for rig_type in &self.rig_types {
                            ui.selectable_value(
                                &mut config.rig_type,
                                rig_type.clone(),
                                rig_type.to_string(),
                            );
                        }
                    });
                ui.end_row();

                ui.label("Port:");
                ComboBox::from_id_salt("port")
                    .selected_text(if config.port.is_empty() {
                        "Select port...".to_string()
                    } else {
                        let mut display_name = self
                            .available_ports
                            .iter()
                            .find(|p| p.port_name == config.port)
                            .map(|p| p.display_name.clone())
                            .unwrap_or_else(|| config.port.clone());
                        display_name.truncate(30);
                        display_name
                    })
                    .show_ui(ui, |ui| {
                        for entry in &self.available_ports {
                            ui.selectable_value(
                                &mut config.port,
                                entry.port_name.clone(),
                                &entry.display_name,
                            );
                        }
                    });
                ui.end_row();

                ui.label("Baud Rate:");
                ComboBox::from_id_salt("baud_rate")
                    .selected_text(format!("{}", config.baud_rate))
                    .show_ui(ui, |ui| {
                        for rate in BaudRate::iter_rates() {
                            ui.selectable_value(&mut config.baud_rate, rate, format!("{rate}"));
                        }
                    });
                ui.end_row();

                ui.label("Data Bits:");
                ComboBox::from_id_salt("data_bits")
                    .selected_text(format!("{}", config.data_bits))
                    .show_ui(ui, |ui| {
                        for bits in DataBits::iter_data_bits() {
                            ui.selectable_value(&mut config.data_bits, bits, format!("{bits}"));
                        }
                    });
                ui.end_row();

                ui.label("Stop Bits:");
                ComboBox::from_id_salt("stop_bits")
                    .selected_text(format!("{}", config.stop_bits))
                    .show_ui(ui, |ui| {
                        for bits in [StopBits::Bits1, StopBits::Bits2] {
                            ui.selectable_value(&mut config.stop_bits, bits, format!("{bits}"));
                        }
                    });
                ui.end_row();

                ui.label("Parity:");
                ui.checkbox(&mut config.parity, "");
                ui.end_row();

                ui.label("RTS:");
                ui.checkbox(&mut config.rts, "");
                ui.end_row();

                ui.label("DTR:");
                ui.checkbox(&mut config.dtr, "");
                ui.end_row();

                ui.label("Poll Interval (ms):");
                ui.add(egui::DragValue::new(&mut config.poll_interval).range(10..=1000));
                ui.end_row();

                ui.label("Timeout (ms):");
                ui.add(egui::DragValue::new(&mut config.timeout).range(10..=5000));
                ui.end_row();
            });

            ui.separator();

            Grid::new("status_and_buttons")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.horizontal(|ui| match &tab.state {
                        RigTabState::Draft => {}
                        RigTabState::Registered { status, .. } => {
                            let (color, text) = match status {
                                ConnectionStatus::Connecting => {
                                    (egui::Color32::YELLOW, "Connecting...".to_string())
                                }
                                ConnectionStatus::Connected => {
                                    (egui::Color32::GREEN, "Connected".to_string())
                                }
                                ConnectionStatus::Error(err) => {
                                    let mut msg = format!("Error: {err}");
                                    msg.truncate(50);
                                    (egui::Color32::RED, msg)
                                }
                            };

                            let (rect, _) =
                                ui.allocate_exact_size(egui::vec2(8.0, 8.0), egui::Sense::hover());
                            ui.painter().circle_filled(rect.center(), 4.0, color);
                            ui.colored_label(color, &text);
                        }
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("OK").clicked() {
                            match config.validate() {
                                Ok(_) => {
                                    let sender = self.sender.clone();
                                    let config = config.clone();
                                    match &tab.state {
                                        RigTabState::Registered { rig_id, .. } => {
                                            let rig_id = *rig_id;
                                            tokio::task::spawn(async move {
                                                let _ = sender
                                                    .send(ManagerCommand::UpdateDevice {
                                                        device_id: rig_id,
                                                        config,
                                                    })
                                                    .await;
                                            });
                                        }
                                        RigTabState::Draft => {
                                            let (tx, rx) = oneshot::channel();
                                            self.pending_creates.push((tab.tab_id, rx));
                                            tokio::task::spawn(async move {
                                                let _ = sender
                                                    .send(ManagerCommand::CreateDevice {
                                                        config,
                                                        response: tx,
                                                    })
                                                    .await;
                                            });
                                        }
                                    }
                                }
                                Err(err) => {
                                    self.error_message = Some(err);
                                }
                            }
                        }
                        if ui.button("Cancel").clicked() {
                            self.error_message = None;
                        }
                    });
                });
        });
    }

    fn is_closeable(&self, tab: &Self::Tab) -> bool {
        Some(tab.tab_id) == self.active_tab_id
    }

    fn on_close(&mut self, tab: &mut Self::Tab) -> OnCloseResponse {
        if let RigTabState::Registered { rig_id, .. } = tab.state {
            let sender = self.sender.clone();
            tokio::task::spawn(async move {
                let _ = sender
                    .send(ManagerCommand::RemoveDevice { device_id: rig_id })
                    .await;
            });
        }
        OnCloseResponse::Close
    }

    fn on_add(&mut self, _surface: SurfaceIndex, _node: NodeIndex) {
        self.add_tab_request = true;
    }
}

struct AppTabs {
    dock_state: DockState<RigTab>,
    rig_types: Vec<String>,
    available_ports: Vec<SerialPortEntry>,
    sender: Sender<ManagerCommand>,
    next_tab_id: u64,
    pending_creates: Vec<(TabId, oneshot::Receiver<RigId>)>,
}

impl AppTabs {
    fn new(sender: Sender<ManagerCommand>, rig_types: Vec<String>) -> Self {
        let mut next = 0u64;
        let draft = RigTab {
            tab_id: next_tab_id(&mut next),
            state: RigTabState::Draft,
            config: RigConfig::default(),
        };
        let dock_state = DockState::new(vec![draft]);
        Self {
            dock_state,
            rig_types,
            available_ports: Vec::new(),
            sender,
            next_tab_id: next,
            pending_creates: Vec::new(),
        }
    }

    fn set_tabs(&mut self, settings: Vec<RigSettings>) {
        let tabs: Vec<RigTab> = if settings.is_empty() {
            vec![RigTab {
                tab_id: next_tab_id(&mut self.next_tab_id),
                state: RigTabState::Draft,
                config: RigConfig::default(),
            }]
        } else {
            settings
                .into_iter()
                .map(|s| RigTab {
                    tab_id: next_tab_id(&mut self.next_tab_id),
                    state: RigTabState::Registered {
                        rig_id: s.id,
                        status: ConnectionStatus::Connecting,
                    },
                    config: s.config,
                })
                .collect()
        };
        self.dock_state = DockState::new(tabs);
    }

    fn poll_pending_creates(&mut self) {
        self.pending_creates
            .retain_mut(|(tab_id, rx)| match rx.try_recv() {
                Ok(rig_id) => {
                    for (_, tab) in self.dock_state.iter_all_tabs_mut() {
                        if tab.tab_id == *tab_id {
                            tab.state = RigTabState::Registered {
                                rig_id,
                                status: ConnectionStatus::Connecting,
                            };
                            break;
                        }
                    }
                    false
                }
                Err(oneshot::error::TryRecvError::Empty) => true,
                Err(oneshot::error::TryRecvError::Closed) => {
                    for (_, tab) in self.dock_state.iter_all_tabs_mut() {
                        if tab.tab_id == *tab_id {
                            tab.state = RigTabState::Draft;
                            break;
                        }
                    }
                    false
                }
            });
    }

    fn update_device_status(&mut self, device_id: RigId, status: ConnectionStatus) {
        for (_, tab) in self.dock_state.iter_all_tabs_mut() {
            if let RigTabState::Registered { rig_id, status: s } = &mut tab.state
                && *rig_id == device_id
            {
                *s = status;
                break;
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui) {
        let active_tab_id = self
            .dock_state
            .find_active_focused()
            .map(|(_, tab)| tab.tab_id)
            .or_else(|| {
                self.dock_state
                    .iter_leaves()
                    .next()
                    .map(|(_, node)| node.tabs[0].tab_id)
            });

        let mut tab_viewer = AppTabViewer::new(
            self.sender.clone(),
            self.rig_types.clone(),
            self.available_ports.clone(),
            active_tab_id,
            &mut self.pending_creates,
        );

        DockArea::new(&mut self.dock_state)
            .show_add_buttons(true)
            .show_close_buttons(true)
            .tab_context_menus(false)
            .draggable_tabs(false)
            .show_leaf_close_all_buttons(false)
            .show_leaf_collapse_buttons(false)
            .allowed_splits(AllowedSplits::None)
            .show_inside(ui, &mut tab_viewer);

        if tab_viewer.add_tab_request {
            self.dock_state
                .main_surface_mut()
                .push_to_first_leaf(RigTab {
                    tab_id: next_tab_id(&mut self.next_tab_id),
                    state: RigTabState::Draft,
                    config: RigConfig::default(),
                });
        }
    }
}

pub struct App {
    message_receiver: broadcast::Receiver<ManagerMessage>,
    tabs: AppTabs,
}

impl App {
    pub fn new(
        message_receiver: broadcast::Receiver<ManagerMessage>,
        serial_sender: Sender<ManagerCommand>,
        rig_types: Vec<String>,
        initial_rigs: Vec<RigSettings>,
    ) -> Self {
        let mut tabs = AppTabs::new(serial_sender, rig_types);
        tabs.set_tabs(initial_rigs);
        App {
            message_receiver,
            tabs,
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.tabs.poll_pending_creates();

        let mut has_messages = false;
        loop {
            match self.message_receiver.try_recv() {
                Ok(message) => {
                    has_messages = true;
                    match message {
                        ManagerMessage::AvailablePorts(ports) => {
                            self.tabs.available_ports = ports;
                        }
                        ManagerMessage::ConnectionStatusChanged { device_id, status } => {
                            self.tabs.update_device_status(device_id, status);
                        }
                        ManagerMessage::StatusUpdate { .. } => {}
                    }
                }
                Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
        if has_messages {
            ctx.request_repaint();
        } else {
            ctx.request_repaint_after(std::time::Duration::from_millis(100));
        }

        // TODO
        ctx.set_pixels_per_point(1.3);
        egui::CentralPanel::default().show(ctx, |ui| self.tabs.ui(ui));
    }
}
